//! Split out of main.rs to keep files under 800 lines.

use crate::*;
use anyhow::Result;
use std::collections::BTreeMap;
use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command as ProcessCommand;
use std::process::Stdio;
use std::thread;
use std::time::Duration;
use std::time::Instant;

pub(crate) struct ClipboardWriter {
    pub(crate) mode: ClipboardWriterMode,
}

pub(crate) enum ClipboardWriterMode {
    Simulated,
    Command { program: PathBuf, args: Vec<String> },
}

impl ClipboardWriter {
    pub(crate) fn simulated() -> Self {
        Self {
            mode: ClipboardWriterMode::Simulated,
        }
    }

    /// Explicit `--clipboard-command <path>`: run the given program with no
    /// extra arguments, exactly as before.
    pub(crate) fn command(program: PathBuf) -> Self {
        Self::command_with_args(program, Vec::new())
    }

    /// Auto-detected clipboard tools (e.g. `xclip -selection clipboard`) carry
    /// their own arguments ahead of the piped clipboard text.
    pub(crate) fn command_with_args(program: PathBuf, args: Vec<String>) -> Self {
        Self {
            mode: ClipboardWriterMode::Command { program, args },
        }
    }

    pub(crate) fn write_text(&mut self, text: &str) -> Result<Option<String>, String> {
        match &self.mode {
            ClipboardWriterMode::Simulated => Ok(None),
            ClipboardWriterMode::Command { program, args } => {
                run_clipboard_command(program, args, text)?;
                Ok(Some("clipboard updated".to_string()))
            }
        }
    }

    /// Test-only view of the resolved mode: `None` when simulated, otherwise the
    /// resolved program path plus its arguments.
    #[cfg(test)]
    pub(crate) fn command_for_test(&self) -> Option<(&Path, &[String])> {
        match &self.mode {
            ClipboardWriterMode::Simulated => None,
            ClipboardWriterMode::Command { program, args } => Some((program, args)),
        }
    }
}

pub(crate) fn run_clipboard_command(
    program: &Path,
    args: &[String],
    text: &str,
) -> Result<(), String> {
    // stdout/stderr -> null: a command (e.g. `xclip`) that daemonizes to serve
    // the X selection would otherwise inherit + hold these pipes, hanging the
    // agent forever in the wait. Pinned PATH guards an auto-detected bare name.
    let mut child = ProcessCommand::new(program)
        .args(args)
        .env("PATH", EFFECT_COMMAND_PATH)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| {
            format!(
                "failed to execute clipboard command {}: {error}",
                program.display()
            )
        })?;

    // Feed the clipboard text on a separate thread so a command that doesn't
    // drain stdin can't deadlock the agent on a large payload.
    let mut stdin = child.stdin.take().ok_or_else(|| {
        format!(
            "failed to open stdin for clipboard command {}",
            program.display()
        )
    })?;
    let payload = text.as_bytes().to_vec();
    let writer = thread::spawn(move || {
        let _ = stdin.write_all(&payload);
        // dropping stdin here closes it (EOF for the child)
    });

    let label = format!("clipboard command {}", program.display());
    let status = wait_bounded(&mut child, &label);
    let _ = writer.join();
    match status {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => Err(format!("{label} failed: exit status {status}")),
        Err(error) => Err(error),
    }
}

/// Reads the guest OS clipboard, mirroring `ClipboardWriter`. The real mode
/// runs a configured/auto-detected reader (`wl-paste`/`xclip -o`) and captures
/// its stdout; the simulated mode returns a fixed value (used in tests / when
/// no reader is available).
pub(crate) struct ClipboardReader {
    pub(crate) mode: ClipboardReaderMode,
}

pub(crate) enum ClipboardReaderMode {
    /// No real reader; `read_text` returns the optional canned value.
    Simulated {
        value: Option<String>,
    },
    Command {
        program: PathBuf,
        args: Vec<String>,
    },
}

impl ClipboardReader {
    /// Simulated reader that always yields `None` (no clipboard content).
    pub(crate) fn simulated() -> Self {
        Self {
            mode: ClipboardReaderMode::Simulated { value: None },
        }
    }

    /// Simulated reader that yields a fixed value (test helper).
    #[cfg(test)]
    pub(crate) fn simulated_value(value: Option<String>) -> Self {
        Self {
            mode: ClipboardReaderMode::Simulated { value },
        }
    }

    /// Explicit `--clipboard-read-command <path>`: run that program with no
    /// extra arguments and capture its stdout.
    pub(crate) fn command(program: PathBuf) -> Self {
        Self::command_with_args(program, Vec::new())
    }

    /// Auto-detected readers carry their own arguments (e.g.
    /// `xclip -selection clipboard -o`).
    pub(crate) fn command_with_args(program: PathBuf, args: Vec<String>) -> Self {
        Self {
            mode: ClipboardReaderMode::Command { program, args },
        }
    }

    /// Read the current clipboard text. `Ok(None)` means "nothing usable to
    /// report this tick" (simulated-empty, or a reader that produced no bytes);
    /// the watcher treats that as no-change. `Err` is a real reader failure.
    pub(crate) fn read_text(&self) -> Result<Option<String>, String> {
        match &self.mode {
            ClipboardReaderMode::Simulated { value } => Ok(value.clone()),
            ClipboardReaderMode::Command { program, args } => {
                run_clipboard_read_command(program, args)
            }
        }
    }

    /// Resolved reader program path, or `None` when simulated. Used to decide
    /// whether a real reader was found and (in tests) which tool was selected.
    pub(crate) fn command_path(&self) -> Option<&Path> {
        match &self.mode {
            ClipboardReaderMode::Simulated { .. } => None,
            ClipboardReaderMode::Command { program, .. } => Some(program),
        }
    }

    /// Test-only view of the resolved mode: `None` when simulated, otherwise the
    /// resolved program path plus its arguments.
    #[cfg(test)]
    pub(crate) fn command_for_test(&self) -> Option<(&Path, &[String])> {
        match &self.mode {
            ClipboardReaderMode::Simulated { .. } => None,
            ClipboardReaderMode::Command { program, args } => Some((program, args)),
        }
    }
}

/// Run a clipboard reader and capture stdout as the clipboard text. Mirrors the
/// effect-command hardening used by `run_clipboard_command`: pinned PATH (an
/// auto-detected bare name resolves only from system dirs), null stdin/stderr (a
/// daemonizing child can't read our stdin or hold stderr), and a bounded wait so
/// a hung reader cannot wedge the watcher thread.
pub(crate) fn run_clipboard_read_command(
    program: &Path,
    args: &[String],
) -> Result<Option<String>, String> {
    let mut child = ProcessCommand::new(program)
        .args(args)
        .env("PATH", EFFECT_COMMAND_PATH)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| {
            format!(
                "failed to execute clipboard read command {}: {error}",
                program.display()
            )
        })?;

    // Drain stdout on a separate thread so a reader that emits a large payload
    // can't deadlock the bounded wait by filling the pipe before exiting.
    let mut stdout = child.stdout.take().ok_or_else(|| {
        format!(
            "failed to open stdout for clipboard read command {}",
            program.display()
        )
    })?;
    let drain =
        thread::spawn(move || drain_output_bounded(&mut stdout, MAX_CLIPBOARD_OUTPUT_BYTES));

    let label = format!("clipboard read command {}", program.display());
    let status = wait_bounded(&mut child, &label);
    let drained = drain
        .join()
        .map_err(|_| format!("{label} stdout drain panicked"))?
        .map_err(|error| format!("{label} stdout read failed: {error}"))?;
    match status {
        Ok(status) if status.success() && drained.1 => Err(format!(
            "{label} output exceeded {MAX_CLIPBOARD_OUTPUT_BYTES} bytes"
        )),
        Ok(status) if status.success() => {
            let text = String::from_utf8_lossy(&drained.0)
                .trim_end_matches(['\r', '\n'])
                .to_string();
            if text.is_empty() {
                Ok(None)
            } else {
                Ok(Some(text))
            }
        }
        Ok(status) => Err(format!("{label} failed: exit status {status}")),
        Err(error) => Err(error),
    }
}

/// Opt-in continuous clipboard watcher configuration: a real reader plus the
/// poll interval. Only constructed when the watcher is enabled.
pub(crate) struct ClipboardWatcher {
    pub(crate) interval: Duration,
    pub(crate) reader: ClipboardReader,
}

/// Pure change-detection core for the clipboard watcher. `observe` returns
/// `Some(text)` only when the observed clipboard content is a non-empty value
/// that differs from the last reported one; identical repeats and empty/None
/// reads are suppressed. Empty/None never clears the remembered value, so a
/// transient empty read between two identical non-empty reads does not cause a
/// spurious re-emit.
#[derive(Debug, Default)]
pub(crate) struct ClipboardWatchState {
    pub(crate) last_reported: Option<String>,
}

impl ClipboardWatchState {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn observe(&mut self, latest: Option<String>) -> Option<String> {
        // Treat None and empty string identically: nothing to report, and the
        // remembered value is left intact (a momentary empty clipboard read is
        // not a "change" worth emitting and must not trigger a later re-emit of
        // the same text).
        let text = latest.filter(|text| !text.is_empty())?;
        if self.last_reported.as_deref() == Some(text.as_str()) {
            return None;
        }
        self.last_reported = Some(text.clone());
        Some(text)
    }
}

pub(crate) struct DisplayResizer {
    pub(crate) mode: DisplayResizerMode,
}

pub(crate) enum DisplayResizerMode {
    Simulated,
    Command { command: PathBuf },
}

impl DisplayResizer {
    pub(crate) fn simulated() -> Self {
        Self {
            mode: DisplayResizerMode::Simulated,
        }
    }

    pub(crate) fn command(command: PathBuf) -> Self {
        Self {
            mode: DisplayResizerMode::Command { command },
        }
    }

    pub(crate) fn resize(
        &mut self,
        width: u32,
        height: u32,
        scale: u16,
    ) -> Result<Option<String>, String> {
        match &self.mode {
            DisplayResizerMode::Simulated => Ok(None),
            DisplayResizerMode::Command { command } => {
                run_display_resize_command(command, width, height, scale)?;
                Ok(Some(format!(
                    "display resized to {width}x{height} scale {scale}"
                )))
            }
        }
    }

    /// Test-only view of the resolved mode: `None` when simulated, otherwise the
    /// resolved program path.
    #[cfg(test)]
    pub(crate) fn command_for_test(&self) -> Option<&Path> {
        match &self.mode {
            DisplayResizerMode::Simulated => None,
            DisplayResizerMode::Command { command } => Some(command),
        }
    }
}

pub(crate) fn run_display_resize_command(
    command: &Path,
    width: u32,
    height: u32,
    scale: u16,
) -> Result<(), String> {
    // Pinned PATH (auto-detected bare name resolves only from system dirs),
    // null fds (a daemonizing child can't hold our pipes), bounded wait.
    let mut child = ProcessCommand::new(command)
        .arg(width.to_string())
        .arg(height.to_string())
        .arg(scale.to_string())
        .env("PATH", EFFECT_COMMAND_PATH)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| {
            format!(
                "failed to execute display resize command {}: {error}",
                command.display()
            )
        })?;
    let label = format!("display resize command {}", command.display());
    match wait_bounded(&mut child, &label)? {
        status if status.success() => Ok(()),
        status => Err(format!("{label} failed: exit status {status}")),
    }
}

/// Applies host TimeSync commands to the guest clock.
pub(crate) struct ClockSetter {
    pub(crate) mode: ClockSetterMode,
}

pub(crate) enum ClockSetterMode {
    /// Acknowledge the host epoch without touching the real clock (used on
    /// non-Linux builds, when --no-real-time-sync is passed, or in tests).
    Simulated,
    /// Apply the host epoch to the real guest clock through the backend.
    Real { backend: Box<dyn ClockBackend> },
}

impl ClockSetter {
    pub(crate) fn simulated() -> Self {
        Self {
            mode: ClockSetterMode::Simulated,
        }
    }

    pub(crate) fn real(backend: Box<dyn ClockBackend>) -> Self {
        Self {
            mode: ClockSetterMode::Real { backend },
        }
    }

    /// Returns an optional human-readable message on success.
    pub(crate) fn set_epoch_millis(
        &mut self,
        unix_epoch_millis: u64,
    ) -> Result<Option<String>, String> {
        match &mut self.mode {
            ClockSetterMode::Simulated => Ok(Some(format!(
                "acknowledged time-sync to {unix_epoch_millis} ms since epoch; guest clock was not changed (simulated)"
            ))),
            ClockSetterMode::Real { backend } => {
                backend.set_epoch_millis(unix_epoch_millis)?;
                Ok(Some(format!(
                    "set guest clock to {unix_epoch_millis} ms since epoch"
                )))
            }
        }
    }
}

pub(crate) trait ClockBackend {
    fn set_epoch_millis(&mut self, unix_epoch_millis: u64) -> Result<(), String>;
}

/// Real Linux backend: set the wall clock with settimeofday(2). The agent runs
/// as root under cloud-init, so CAP_SYS_TIME is available.
pub(crate) struct SettimeofdayClockBackend;

impl ClockBackend for SettimeofdayClockBackend {
    fn set_epoch_millis(&mut self, unix_epoch_millis: u64) -> Result<(), String> {
        set_system_clock_millis(unix_epoch_millis)
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn set_system_clock_millis(unix_epoch_millis: u64) -> Result<(), String> {
    let seconds = (unix_epoch_millis / 1_000) as libc::time_t;
    let micros = ((unix_epoch_millis % 1_000) * 1_000) as libc::suseconds_t;
    let tv = libc::timeval {
        tv_sec: seconds,
        tv_usec: micros,
    };
    // SAFETY: tv is a fully-initialized timeval; settimeofday reads it and does
    // not retain the pointer.
    let rc = unsafe { libc::settimeofday(&tv, std::ptr::null()) };
    if rc == 0 {
        Ok(())
    } else {
        Err(format!(
            "settimeofday failed: {}",
            std::io::Error::last_os_error()
        ))
    }
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn set_system_clock_millis(_unix_epoch_millis: u64) -> Result<(), String> {
    Err("real clock sync is only supported on Linux guests".to_string())
}

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

pub(crate) trait FilesystemFreezeBackend {
    fn freeze(&mut self, mount: &Path, timeout_millis: Option<u64>) -> Result<(), String>;
    fn thaw(&mut self, mount: &Path) -> Result<(), String>;
}

pub(crate) struct CommandFilesystemFreezeBackend;

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

/// Number of compute-kernel iterations folded between each wall-clock deadline
/// check. Small enough that the benchmark stops promptly once the budget is
/// spent, large enough that the `Instant::now()` overhead stays negligible.
pub(crate) const BENCHMARK_KERNEL_CHUNK: u64 = 4_096;
/// Fixed, small payload for the optional disk micro-benchmark. Bounded by
/// construction so the guest never writes unbounded data to its own disk.
pub(crate) const BENCHMARK_DISK_BYTES: usize = 256 * 1024;

/// Pure, deterministic compute kernel: an FNV-1a-style integer hash fold over
/// `iterations` steps starting from `seed`. It performs a fixed amount of work
/// per iteration and returns the same value for the same `(seed, iterations)`
/// input on every platform, so it is unit-testable independently of timing and
/// usable as a CPU-load generator. No allocation, no I/O, no unbounded loops.
pub(crate) fn benchmark_kernel(seed: u64, iterations: u64) -> u64 {
    const FNV_PRIME: u64 = 0x0000_0100_0000_01B3;
    let mut state = seed ^ 0xcbf2_9ce4_8422_2325;
    let mut i = 0u64;
    while i < iterations {
        // Mix the counter in and fold; wrapping ops keep this total and
        // deterministic regardless of overflow.
        state ^= i;
        state = state.wrapping_mul(FNV_PRIME);
        state = state.rotate_left(13) ^ (state >> 7);
        i += 1;
    }
    state
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CpuBenchmarkReport {
    pub(crate) iterations: u64,
    pub(crate) elapsed_millis: u64,
    pub(crate) ops_per_sec: u64,
    pub(crate) checksum: u64,
}

/// Run the pure kernel in fixed-size chunks until the wall-clock `budget` is
/// spent, then report iterations completed, elapsed time, and a derived
/// ops/sec figure. Bounded by `budget` (the caller clamps it to a hard maximum)
/// and by the chunked deadline check; it never loops unbounded and never
/// allocates.
pub(crate) fn run_cpu_benchmark(budget: Duration) -> CpuBenchmarkReport {
    let start = Instant::now();
    let deadline = start + budget;
    let mut iterations: u64 = 0;
    let mut checksum: u64 = 0;
    // Always run at least one chunk so a tiny budget still yields a real figure.
    loop {
        checksum = benchmark_kernel(checksum, BENCHMARK_KERNEL_CHUNK);
        iterations = iterations.saturating_add(BENCHMARK_KERNEL_CHUNK);
        if Instant::now() >= deadline {
            break;
        }
    }
    let elapsed = start.elapsed();
    let elapsed_millis = elapsed.as_millis().min(u128::from(u64::MAX)) as u64;
    let elapsed_secs = elapsed.as_secs_f64();
    let ops_per_sec = if elapsed_secs > 0.0 {
        (iterations as f64 / elapsed_secs)
            .round()
            .min(u64::MAX as f64) as u64
    } else {
        0
    };
    CpuBenchmarkReport {
        iterations,
        elapsed_millis,
        ops_per_sec,
        checksum,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DiskBenchmarkReport {
    pub(crate) bytes_written: usize,
    pub(crate) elapsed_millis: u64,
    pub(crate) mib_per_sec: u64,
}

/// Tiny disk write+fsync micro-benchmark: write a fixed, small buffer to a
/// uniquely-named temp file in `dir`, fsync it, measure, then always remove the
/// file. The payload size is a compile-time constant, so this never writes
/// unbounded data; a write/sync error is surfaced to the caller as `Err`.
pub(crate) fn run_disk_benchmark(dir: &Path) -> Result<DiskBenchmarkReport, String> {
    fs::create_dir_all(dir)
        .map_err(|error| format!("failed to create benchmark scratch dir: {error}"))?;
    let micros = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH_FOR_BENCH)
        .map(|since| since.as_micros())
        .unwrap_or(0);
    let path = dir.join(format!(
        ".bridgevm-bench-{}-{micros}.tmp",
        std::process::id()
    ));
    let payload = vec![0xA5u8; BENCHMARK_DISK_BYTES];

    let start = Instant::now();
    let result = (|| -> std::io::Result<()> {
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&path)?;
        file.write_all(&payload)?;
        file.flush()?;
        file.sync_all()?;
        Ok(())
    })();
    let elapsed = start.elapsed();
    // Always remove the temp file, whether or not the write succeeded.
    let _ = fs::remove_file(&path);
    result.map_err(|error| format!("benchmark disk write failed: {error}"))?;

    let elapsed_millis = elapsed.as_millis().min(u128::from(u64::MAX)) as u64;
    let elapsed_secs = elapsed.as_secs_f64();
    let mib = BENCHMARK_DISK_BYTES as f64 / (1024.0 * 1024.0);
    let mib_per_sec = if elapsed_secs > 0.0 {
        (mib / elapsed_secs).round().min(u64::MAX as f64) as u64
    } else {
        0
    };
    Ok(DiskBenchmarkReport {
        bytes_written: BENCHMARK_DISK_BYTES,
        elapsed_millis,
        mib_per_sec,
    })
}

/// Epoch constant for naming the benchmark scratch file. Aliased so the disk
/// benchmark does not depend on the test-only `UNIX_EPOCH` import.
use std::time::UNIX_EPOCH as UNIX_EPOCH_FOR_BENCH;

pub(crate) fn default_applications() -> BTreeMap<String, ApplicationEntry> {
    [
        (
            "org.bridgevm.terminal",
            ApplicationEntry {
                name: "Terminal".to_string(),
                launched: false,
            },
        ),
        (
            "org.bridgevm.files",
            ApplicationEntry {
                name: "Files".to_string(),
                launched: false,
            },
        ),
    ]
    .into_iter()
    .map(|(id, entry)| (id.to_string(), entry))
    .collect()
}

pub(crate) fn default_windows() -> BTreeMap<String, WindowEntry> {
    [(
        "window-1",
        WindowEntry {
            title: "BridgeVM Linux Desktop".to_string(),
            focused: true,
            closed: false,
            bounds: None,
        },
    )]
    .into_iter()
    .map(|(id, entry)| (id.to_string(), entry))
    .collect()
}
