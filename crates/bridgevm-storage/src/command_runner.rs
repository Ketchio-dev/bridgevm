//! Subprocess execution with timeout and bounded stdout/stderr drain.

use std::io::Read;
use std::process::Command;
use std::process::Output;
use std::process::Stdio;
use std::thread;
use std::thread::sleep;
use std::time::Duration;
use std::time::Instant;

pub(crate) fn run_command(program: &str, args: &[String]) -> Result<Output, std::io::Error> {
    const COMMAND_TIMEOUT: Duration = Duration::from_secs(6 * 60 * 60);
    const COMMAND_OUTPUT_LIMIT: usize = 1024 * 1024;

    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| std::io::Error::other("failed to capture command stdout"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| std::io::Error::other("failed to capture command stderr"))?;
    let stdout_thread = thread::spawn(move || drain_command_stream(stdout, COMMAND_OUTPUT_LIMIT));
    let stderr_thread = thread::spawn(move || drain_command_stream(stderr, COMMAND_OUTPUT_LIMIT));

    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_thread.join();
                let _ = stderr_thread.join();
                return Err(error);
            }
        }
        if started.elapsed() >= COMMAND_TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdout_thread.join();
            let _ = stderr_thread.join();
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!(
                    "command exceeded {}-second timeout",
                    COMMAND_TIMEOUT.as_secs()
                ),
            ));
        }
        sleep(Duration::from_millis(100));
    };

    let (stdout, stdout_exceeded) = join_command_stream(stdout_thread, "stdout")?;
    let (stderr, stderr_exceeded) = join_command_stream(stderr_thread, "stderr")?;
    if stdout_exceeded || stderr_exceeded {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("command output exceeded {COMMAND_OUTPUT_LIMIT}-byte per-stream limit"),
        ));
    }
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

pub(crate) fn drain_command_stream<R: Read>(
    mut stream: R,
    limit: usize,
) -> Result<(Vec<u8>, bool), std::io::Error> {
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

pub(crate) fn join_command_stream(
    handle: thread::JoinHandle<Result<(Vec<u8>, bool), std::io::Error>>,
    name: &str,
) -> Result<(Vec<u8>, bool), std::io::Error> {
    handle
        .join()
        .map_err(|_| std::io::Error::other(format!("command {name} drain thread panicked")))?
}
