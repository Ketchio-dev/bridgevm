//! The subprocess AppleVzRunner launcher with timeout and bounded output drain.

use crate::*;
use std::io::Read;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Stdio;
use std::thread;
use std::time::Duration;
use std::time::Instant;

pub(crate) const APPLE_VZ_LAUNCH_TIMEOUT: Duration = Duration::from_secs(30);

pub(crate) const MAX_LAUNCHER_STREAM_BYTES: usize = 1024 * 1024;

pub(crate) const LAUNCHER_DRAIN_CHUNK_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone)]
pub struct AppleVzCommandLauncher {
    pub(crate) program: PathBuf,
    pub(crate) args: Vec<String>,
    pub(crate) env: Vec<(String, String)>,
}

pub(crate) fn drain_launcher_stream(
    reader: &mut impl Read,
    maximum: usize,
) -> std::io::Result<(Vec<u8>, bool)> {
    let mut captured = Vec::new();
    let mut chunk = [0_u8; LAUNCHER_DRAIN_CHUNK_BYTES];
    let mut exceeded = false;
    loop {
        let read = reader.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        let keep = read.min(maximum.saturating_sub(captured.len()));
        captured.extend_from_slice(&chunk[..keep]);
        exceeded |= keep < read;
    }
    Ok((captured, exceeded))
}

pub(crate) fn join_launcher_drain(
    drain: thread::JoinHandle<std::io::Result<(Vec<u8>, bool)>>,
    program: &Path,
) -> Result<(Vec<u8>, bool), AppleVzLaunchError> {
    drain
        .join()
        .map_err(|_| AppleVzLaunchError::LauncherSpawn {
            program: program.to_path_buf(),
            source: std::io::Error::other("launcher output drain panicked"),
        })?
        .map_err(|source| AppleVzLaunchError::LauncherSpawn {
            program: program.to_path_buf(),
            source,
        })
}

pub(crate) fn format_launcher_output(stdout: &str, stderr: &str) -> String {
    match (stdout.is_empty(), stderr.is_empty()) {
        (true, true) => "no output".to_string(),
        (true, false) => stderr.to_string(),
        (false, true) => format!("stdout: {stdout}"),
        (false, false) => format!("stderr: {stderr}; stdout: {stdout}"),
    }
}

impl AppleVzCommandLauncher {
    pub fn new(program: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            env: Vec::new(),
        }
    }

    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }
}

impl AppleVzLauncher for AppleVzCommandLauncher {
    fn launch(
        &self,
        handoff: AppleVzLaunchHandoff,
    ) -> Result<AppleVzLaunchAttempt, AppleVzLaunchError> {
        let input = serde_json::to_vec(&handoff).map_err(|source| {
            AppleVzLaunchError::LauncherSerialize {
                program: self.program.clone(),
                source,
            }
        })?;
        let mut child = Command::new(&self.program)
            .args(&self.args)
            .envs(self.env.iter().map(|(key, value)| (key, value)))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|source| AppleVzLaunchError::LauncherSpawn {
                program: self.program.clone(),
                source,
            })?;
        let mut stdout = child.stdout.take().expect("piped stdout should be present");
        let mut stderr = child.stderr.take().expect("piped stderr should be present");
        let stdout_drain =
            thread::spawn(move || drain_launcher_stream(&mut stdout, MAX_LAUNCHER_STREAM_BYTES));
        let stderr_drain =
            thread::spawn(move || drain_launcher_stream(&mut stderr, MAX_LAUNCHER_STREAM_BYTES));
        let mut stdin = child.stdin.take().expect("piped stdin should be present");
        if let Err(source) = stdin.write_all(&input) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdout_drain.join();
            let _ = stderr_drain.join();
            return Err(AppleVzLaunchError::LauncherWrite {
                program: self.program.clone(),
                source,
            });
        }
        drop(stdin);
        let deadline = Instant::now() + APPLE_VZ_LAUNCH_TIMEOUT;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
                Ok(None) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = stdout_drain.join();
                    let _ = stderr_drain.join();
                    return Err(AppleVzLaunchError::LauncherTimeout {
                        program: self.program.clone(),
                        seconds: APPLE_VZ_LAUNCH_TIMEOUT.as_secs(),
                    });
                }
                Err(source) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = stdout_drain.join();
                    let _ = stderr_drain.join();
                    return Err(AppleVzLaunchError::LauncherSpawn {
                        program: self.program.clone(),
                        source,
                    });
                }
            }
        };
        let (stdout_bytes, stdout_exceeded) = join_launcher_drain(stdout_drain, &self.program)?;
        let (stderr_bytes, stderr_exceeded) = join_launcher_drain(stderr_drain, &self.program)?;
        if stdout_exceeded || stderr_exceeded {
            return Err(AppleVzLaunchError::LauncherOutputTooLarge {
                program: self.program.clone(),
                stream: if stdout_exceeded { "stdout" } else { "stderr" },
                maximum: MAX_LAUNCHER_STREAM_BYTES,
            });
        }
        let stdout = String::from_utf8_lossy(&stdout_bytes).trim().to_string();
        let stderr = String::from_utf8_lossy(&stderr_bytes).trim().to_string();
        if !status.success() {
            return Err(AppleVzLaunchError::LauncherFailed {
                program: self.program.clone(),
                status: status.to_string(),
                stdout: stdout.clone(),
                stderr: stderr.clone(),
                output: format_launcher_output(&stdout, &stderr),
            });
        }
        Ok(AppleVzLaunchAttempt {
            backend: handoff.backend,
            vm_name: handoff.vm_name,
            stdout,
            stderr,
        })
    }
}
