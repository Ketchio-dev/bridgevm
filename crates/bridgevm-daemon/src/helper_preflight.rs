//! Locating bundled helper executables and validating them: executable bit, codesign entitlement.

use anyhow::Context;
use anyhow::Result;
use std::env;
use std::fs;
use std::io::ErrorKind;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Output;
use std::process::Stdio;
use std::thread;
use std::time::Duration;
use std::time::Instant;

#[cfg(target_os = "macos")]
pub(crate) const CODESIGN_PREFLIGHT_TIMEOUT: Duration = Duration::from_secs(10);

#[cfg(target_os = "macos")]
pub(crate) const CODESIGN_PREFLIGHT_OUTPUT_LIMIT: usize = 1024 * 1024;

pub(crate) fn bundled_helper_path(name: &str) -> Option<PathBuf> {
    let exe = env::current_exe().ok()?;
    bundled_helper_path_from_exe(&exe, name)
}

pub(crate) fn bundled_helper_path_from_exe(exe: &Path, name: &str) -> Option<PathBuf> {
    let helper = exe.parent()?.join(name);
    if helper.is_file() && is_executable(&helper) {
        Some(helper)
    } else {
        None
    }
}

pub(crate) fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::metadata(path)
            .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        path.is_file()
    }
}

pub(crate) fn require_executable(path: &Path, label: &str) -> Result<()> {
    let metadata = fs::metadata(path)
        .with_context(|| format!("{label} is missing or not readable: {}", path.display()))?;
    if !metadata.is_file() {
        anyhow::bail!("{label} is not a file: {}", path.display());
    }
    if !is_executable(path) {
        anyhow::bail!("{label} is not executable: {}", path.display());
    }
    Ok(())
}

#[cfg(target_os = "macos")]
pub(crate) fn verify_apple_vz_runner_entitlement(path: &Path) -> Result<()> {
    let mut command = Command::new("codesign");
    command.args(["-d", "--entitlements", ":-"]).arg(path);
    let output = run_bounded_command_output(
        command,
        CODESIGN_PREFLIGHT_TIMEOUT,
        CODESIGN_PREFLIGHT_OUTPUT_LIMIT,
    )
    .with_context(|| {
        format!(
            "failed to inspect AppleVzRunner entitlements: {}",
            path.display()
        )
    })?;
    if !output.status.success() {
        anyhow::bail!(
            "AppleVzRunner entitlement preflight failed for {}: {}",
            path.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    if !entitlement_plist_has_true(&stdout, "com.apple.security.virtualization") {
        anyhow::bail!(
            "AppleVzRunner is missing com.apple.security.virtualization entitlement: {}",
            path.display()
        );
    }
    Ok(())
}

pub(crate) fn run_bounded_command_output(
    mut command: Command,
    timeout: Duration,
    output_limit: usize,
) -> std::io::Result<Output> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let stdout = match child.stdout.take() {
        Some(stdout) => stdout,
        None => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(std::io::Error::other("failed to capture command stdout"));
        }
    };
    let stderr = match child.stderr.take() {
        Some(stderr) => stderr,
        None => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(std::io::Error::other("failed to capture command stderr"));
        }
    };
    let stdout_thread = thread::spawn(move || drain_command_output(stdout, output_limit));
    let stderr_thread = thread::spawn(move || drain_command_output(stderr, output_limit));

    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < timeout => thread::sleep(Duration::from_millis(10)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_thread.join();
                let _ = stderr_thread.join();
                return Err(std::io::Error::new(
                    ErrorKind::TimedOut,
                    format!(
                        "command exceeded {}-millisecond timeout",
                        timeout.as_millis()
                    ),
                ));
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_thread.join();
                let _ = stderr_thread.join();
                return Err(error);
            }
        }
    };

    let (stdout, stdout_exceeded) = join_command_output(stdout_thread, "stdout")?;
    let (stderr, stderr_exceeded) = join_command_output(stderr_thread, "stderr")?;
    if stdout_exceeded || stderr_exceeded {
        return Err(std::io::Error::new(
            ErrorKind::InvalidData,
            format!("command output exceeded {output_limit}-byte per-stream limit"),
        ));
    }
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

pub(crate) fn drain_command_output<R: Read>(
    mut stream: R,
    limit: usize,
) -> std::io::Result<(Vec<u8>, bool)> {
    let mut retained = Vec::with_capacity(limit.min(8192));
    let mut exceeded = false;
    let mut buffer = [0_u8; 8192];
    loop {
        let read = stream.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        let remaining = limit.saturating_sub(retained.len());
        let keep = remaining.min(read);
        retained.extend_from_slice(&buffer[..keep]);
        exceeded |= keep < read;
    }
    Ok((retained, exceeded))
}

pub(crate) fn join_command_output(
    handle: thread::JoinHandle<std::io::Result<(Vec<u8>, bool)>>,
    name: &str,
) -> std::io::Result<(Vec<u8>, bool)> {
    handle
        .join()
        .map_err(|_| std::io::Error::other(format!("command {name} drain thread panicked")))?
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn verify_apple_vz_runner_entitlement(_path: &Path) -> Result<()> {
    Ok(())
}

pub(crate) fn entitlement_plist_has_true(plist: &str, key: &str) -> bool {
    let key_tag = format!("<key>{key}</key>");
    let Some(after_key) = plist.split_once(&key_tag).map(|(_, after)| after) else {
        return false;
    };
    let value = after_key.trim_start();
    value.starts_with("<true/>") || value.starts_with("<true />")
}
