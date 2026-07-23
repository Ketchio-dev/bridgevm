//! Clipboard write and read backends, their bounded runners, and the watcher change-detection state.

use crate::*;
use anyhow::Result;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command as ProcessCommand;
use std::process::Stdio;
use std::thread;
use std::time::Duration;

pub(crate) struct ClipboardWriter {
    pub(crate) mode: ClipboardWriterMode,
}

pub(crate) enum ClipboardWriterMode {
    Simulated,
    Command { program: PathBuf, args: Vec<String> },
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

pub(crate) const MAX_CLIPBOARD_OUTPUT_BYTES: usize = 512 * 1024;

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
