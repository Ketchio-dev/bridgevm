//! The display-resize backend and its command runner.

use crate::*;
use anyhow::Result;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command as ProcessCommand;
use std::process::Stdio;

pub(crate) struct DisplayResizer {
    pub(crate) mode: DisplayResizerMode,
}

pub(crate) enum DisplayResizerMode {
    Simulated,
    Command { command: PathBuf },
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
