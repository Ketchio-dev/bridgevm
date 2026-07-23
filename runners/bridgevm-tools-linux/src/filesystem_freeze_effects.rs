//! fsfreeze/thaw backend, mount-path normalization, best-effort rollback, result messages.

use crate::*;
use anyhow::Result;
use std::collections::BTreeSet;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command as ProcessCommand;
use std::process::Stdio;
use std::thread;
use std::time::Duration;

pub(crate) struct FilesystemFreezer {
    pub(crate) mode: FilesystemFreezerMode,
}

pub(crate) enum FilesystemFreezerMode {
    Simulated,
    Real {
        mounts: Vec<PathBuf>,
        frozen_mounts: Vec<PathBuf>,
        backend: Box<dyn FilesystemFreezeBackend>,
    },
}

pub(crate) trait FilesystemFreezeBackend {
    fn freeze(&mut self, mount: &Path, timeout_millis: Option<u64>) -> Result<(), String>;
    fn thaw(&mut self, mount: &Path) -> Result<(), String>;
}

pub(crate) struct CommandFilesystemFreezeBackend;

pub(crate) fn run_fsfreeze_command(
    flag: &str,
    mount: &Path,
    timeout: Duration,
) -> Result<(), String> {
    let mut child = ProcessCommand::new("fsfreeze")
        .arg(flag)
        .arg(mount)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("failed to execute fsfreeze: {error}"))?;
    let mut stdout = match child.stdout.take() {
        Some(stdout) => stdout,
        None => {
            let _ = child.kill();
            let _ = child.wait();
            return Err("failed to capture fsfreeze stdout".to_string());
        }
    };
    let mut stderr = match child.stderr.take() {
        Some(stderr) => stderr,
        None => {
            let _ = child.kill();
            let _ = child.wait();
            return Err("failed to capture fsfreeze stderr".to_string());
        }
    };
    let stdout_thread =
        thread::spawn(move || drain_output_bounded(&mut stdout, MAX_FSFREEZE_OUTPUT_BYTES));
    let stderr_thread =
        thread::spawn(move || drain_output_bounded(&mut stderr, MAX_FSFREEZE_OUTPUT_BYTES));
    let status = wait_bounded_for(&mut child, "fsfreeze", timeout);
    let (stdout, stdout_exceeded) = stdout_thread
        .join()
        .map_err(|_| "fsfreeze stdout drain thread panicked".to_string())?
        .map_err(|error| format!("failed to read fsfreeze stdout: {error}"))?;
    let (stderr, stderr_exceeded) = stderr_thread
        .join()
        .map_err(|_| "fsfreeze stderr drain thread panicked".to_string())?
        .map_err(|error| format!("failed to read fsfreeze stderr: {error}"))?;
    let status = status?;
    if stdout_exceeded || stderr_exceeded {
        return Err(format!(
            "fsfreeze output exceeded {MAX_FSFREEZE_OUTPUT_BYTES}-byte per-stream limit"
        ));
    }
    if status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&stdout).trim().to_string();
    let detail = if !stderr.is_empty() {
        stderr
    } else if !stdout.is_empty() {
        stdout
    } else {
        format!("exit status {status}")
    };
    Err(detail)
}

pub(crate) fn thaw_mounts_best_effort(
    backend: &mut dyn FilesystemFreezeBackend,
    frozen_mounts: &[PathBuf],
) -> Vec<String> {
    let mut errors = Vec::new();
    for mount in frozen_mounts.iter().rev() {
        if let Err(error) = backend.thaw(mount) {
            errors.push(format!("{}: {error}", mount.display()));
        }
    }
    errors
}

pub(crate) fn display_mounts(mounts: &[PathBuf]) -> String {
    if mounts.is_empty() {
        "none".to_string()
    } else {
        mounts
            .iter()
            .map(|mount| mount.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

pub(crate) fn freeze_thaw_message(
    action: &str,
    timeout_millis: Option<u64>,
    state: &str,
) -> String {
    let timeout = timeout_millis.map_or_else(
        || "without a timeout".to_string(),
        |timeout_millis| format!("with timeout {timeout_millis} ms"),
    );
    format!(
        "{state} simulated filesystem {action} scaffold boundary {timeout}; no OS fsfreeze was executed and application consistency is not guaranteed"
    )
}

pub(crate) const DEFAULT_FSFREEZE_TIMEOUT: Duration = Duration::from_secs(30);

pub(crate) const MAX_FSFREEZE_OUTPUT_BYTES: usize = 64 * 1024;

pub(crate) fn normalize_fsfreeze_mounts(mounts: Vec<PathBuf>) -> Result<Vec<PathBuf>> {
    let mut seen = BTreeSet::new();
    let mut normalized = Vec::new();
    for mount in mounts {
        if !mount.is_absolute() {
            anyhow::bail!(
                "fsfreeze mount must be an absolute path: {}",
                mount.display()
            );
        }
        let normalized_mount = normalize_absolute_path(&mount)?;
        if normalized_mount.as_os_str().is_empty() {
            anyhow::bail!("fsfreeze mount cannot be empty");
        }
        if !seen.insert(normalized_mount.clone()) {
            anyhow::bail!("duplicate fsfreeze mount: {}", normalized_mount.display());
        }
        normalized.push(normalized_mount);
    }
    Ok(normalized)
}

pub(crate) fn normalize_absolute_path(path: &Path) -> Result<PathBuf> {
    let mut normalized = PathBuf::from("/");
    for component in path.components() {
        match component {
            Component::RootDir => {}
            Component::CurDir => {}
            Component::Normal(part) => normalized.push(part),
            Component::ParentDir => {
                if !normalized.pop() {
                    anyhow::bail!("fsfreeze mount escapes root: {}", path.display());
                }
            }
            Component::Prefix(_) => {
                anyhow::bail!("fsfreeze mount must be a Unix path: {}", path.display());
            }
        }
    }
    if normalized.as_os_str().is_empty() {
        Ok(PathBuf::from("/"))
    } else {
        Ok(normalized)
    }
}

impl FilesystemFreezer {
    pub(crate) fn simulated() -> Self {
        Self {
            mode: FilesystemFreezerMode::Simulated,
        }
    }

    pub(crate) fn real(mounts: Vec<PathBuf>, backend: Box<dyn FilesystemFreezeBackend>) -> Self {
        Self {
            mode: FilesystemFreezerMode::Real {
                mounts,
                frozen_mounts: Vec::new(),
                backend,
            },
        }
    }

    pub(crate) fn freeze(&mut self, timeout_millis: Option<u64>) -> Result<String, String> {
        match &mut self.mode {
            FilesystemFreezerMode::Simulated => {
                Ok(freeze_thaw_message("freeze", timeout_millis, "entered"))
            }
            FilesystemFreezerMode::Real {
                mounts,
                frozen_mounts,
                backend,
            } => {
                frozen_mounts.clear();
                for mount in mounts {
                    if let Err(error) = backend.freeze(mount, timeout_millis) {
                        let rollback = thaw_mounts_best_effort(backend.as_mut(), frozen_mounts);
                        frozen_mounts.clear();
                        let rollback_suffix = if rollback.is_empty() {
                            "rollback thaw succeeded".to_string()
                        } else {
                            format!("rollback thaw errors: {}", rollback.join("; "))
                        };
                        return Err(format!(
                            "failed to freeze {}: {error}; {rollback_suffix}",
                            mount.display()
                        ));
                    }
                    frozen_mounts.push(mount.clone());
                }

                Ok(format!(
                    "entered real fsfreeze boundary for {}; application consistency still depends on guest applications flushing state",
                    display_mounts(frozen_mounts)
                ))
            }
        }
    }

    pub(crate) fn thaw(&mut self) -> Result<String, String> {
        match &mut self.mode {
            FilesystemFreezerMode::Simulated => Ok(
                "left simulated filesystem thaw scaffold boundary; no OS fsfreeze was executed and application consistency is not guaranteed"
                    .to_string(),
            ),
            FilesystemFreezerMode::Real {
                frozen_mounts,
                backend,
                ..
            } => {
                let errors = thaw_mounts_best_effort(backend.as_mut(), frozen_mounts);
                if errors.is_empty() {
                    let thawed = display_mounts(frozen_mounts);
                    frozen_mounts.clear();
                    Ok(format!(
                        "left real fsfreeze boundary for {thawed}; application consistency still depends on guest applications flushing state"
                    ))
                } else {
                    Err(format!("failed to thaw all filesystems: {}", errors.join("; ")))
                }
            }
        }
    }
}

impl FilesystemFreezeBackend for CommandFilesystemFreezeBackend {
    fn freeze(&mut self, mount: &Path, timeout_millis: Option<u64>) -> Result<(), String> {
        let timeout = timeout_millis
            .map(Duration::from_millis)
            .unwrap_or(DEFAULT_FSFREEZE_TIMEOUT);
        run_fsfreeze_command("-f", mount, timeout)
    }

    fn thaw(&mut self, mount: &Path) -> Result<(), String> {
        run_fsfreeze_command("-u", mount, DEFAULT_FSFREEZE_TIMEOUT)
    }
}
