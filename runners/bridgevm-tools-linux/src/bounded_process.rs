//! Bounded child-process execution with a pinned PATH, timeout kill and capped output, plus bounded file reads.

use anyhow::Result;
use std::fs;
use std::io::Read;
use std::path::Path;
use std::process::Command as ProcessCommand;
use std::process::Stdio;
use std::thread;
use std::time::Duration;
use std::time::Instant;

pub(crate) const EFFECT_COMMAND_PATH: &str = "/usr/bin:/bin:/usr/sbin:/sbin";

/// Hard cap so a configured effect command that hangs (e.g. a daemonizing child)
/// can't wedge the single-threaded agent forever.
pub(crate) const EFFECT_COMMAND_TIMEOUT: Duration = Duration::from_secs(5);

pub(crate) const MAX_DESKTOP_COMMAND_OUTPUT_BYTES: usize = 4 * 1024 * 1024;

pub(crate) const OUTPUT_DRAIN_BUFFER_BYTES: usize = 64 * 1024;

pub(crate) fn read_utf8_file_bounded(path: &Path, max_bytes: usize) -> std::io::Result<String> {
    let file = fs::File::open(path)?;
    let max_u64 = u64::try_from(max_bytes)
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "limit too large"))?;
    let size = file.metadata()?.len();
    if size > max_u64 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("file is {size} bytes, larger than the {max_bytes} byte limit"),
        ));
    }
    let read_limit = max_u64
        .checked_add(1)
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "limit too large"))?;
    let capacity = usize::try_from(size).map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "file size exceeds host address space",
        )
    })?;
    let mut bytes = Vec::with_capacity(capacity);
    file.take(read_limit).read_to_end(&mut bytes)?;
    if bytes.len() > max_bytes {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("file grew beyond the {max_bytes} byte limit while being read"),
        ));
    }
    String::from_utf8(bytes).map_err(|error| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("file is not valid UTF-8: {error}"),
        )
    })
}

pub(crate) fn drain_output_bounded(
    reader: &mut impl Read,
    max_bytes: usize,
) -> std::io::Result<(Vec<u8>, bool)> {
    let mut captured = Vec::new();
    let mut chunk = [0_u8; OUTPUT_DRAIN_BUFFER_BYTES];
    let mut exceeded = false;
    loop {
        let read = reader.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        let remaining = max_bytes.saturating_sub(captured.len());
        let keep = read.min(remaining);
        captured.extend_from_slice(&chunk[..keep]);
        exceeded |= keep < read;
    }
    Ok((captured, exceeded))
}

/// Wait for `child` up to `EFFECT_COMMAND_TIMEOUT`, killing + reaping it on
/// timeout. Returns the exit status, or an error string on timeout/wait failure.
pub(crate) fn wait_bounded(
    child: &mut std::process::Child,
    label: &str,
) -> Result<std::process::ExitStatus, String> {
    wait_bounded_for(child, label, EFFECT_COMMAND_TIMEOUT)
}

pub(crate) fn wait_bounded_for(
    child: &mut std::process::Child,
    label: &str,
    timeout: Duration,
) -> Result<std::process::ExitStatus, String> {
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!("{label} timed out"));
                }
                thread::sleep(Duration::from_millis(20));
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("failed to wait for {label}: {error}"));
            }
        }
    }
}

pub(crate) fn run_command_status(program: &Path, args: &[&str]) -> Result<(), String> {
    let mut child = ProcessCommand::new(program)
        .args(args)
        .env("PATH", EFFECT_COMMAND_PATH)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("failed to execute {}: {error}", program.display()))?;
    let label = format!("desktop command {}", program.display());
    match wait_bounded(&mut child, &label)? {
        status if status.success() => Ok(()),
        status => Err(format!("{label} failed: exit status {status}")),
    }
}

pub(crate) fn run_command_output(program: &Path, args: &[&str]) -> Result<String, String> {
    let mut child = ProcessCommand::new(program)
        .args(args)
        .env("PATH", EFFECT_COMMAND_PATH)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("failed to execute {}: {error}", program.display()))?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| format!("failed to open stdout for {}", program.display()))?;
    let drain =
        thread::spawn(move || drain_output_bounded(&mut stdout, MAX_DESKTOP_COMMAND_OUTPUT_BYTES));
    let label = format!("desktop command {}", program.display());
    let status = wait_bounded(&mut child, &label);
    let drained = drain
        .join()
        .map_err(|_| format!("{label} stdout drain panicked"))?
        .map_err(|error| format!("{label} stdout read failed: {error}"))?;
    match status {
        Ok(status) if status.success() && drained.1 => Err(format!(
            "{label} output exceeded {MAX_DESKTOP_COMMAND_OUTPUT_BYTES} bytes"
        )),
        Ok(status) if status.success() => Ok(String::from_utf8_lossy(&drained.0).to_string()),
        Ok(status) => Err(format!("{label} failed: exit status {status}")),
        Err(error) => Err(error),
    }
}
