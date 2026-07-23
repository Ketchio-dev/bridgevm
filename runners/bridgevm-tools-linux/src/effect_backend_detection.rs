//! Capability-gated resolution and auto-detection of the clipboard, resize, clock and desktop backends.

use crate::*;
use anyhow::Result;
use bridgevm_agent_protocol::AgentCapability;
use std::path::PathBuf;
use std::time::Duration;

pub(crate) fn resolve_filesystem_freezer(
    real_fsfreeze: bool,
    mounts: Vec<PathBuf>,
) -> Result<FilesystemFreezer> {
    if !real_fsfreeze {
        if !mounts.is_empty() {
            anyhow::bail!("--fsfreeze-mount requires --real-fsfreeze");
        }
        return Ok(FilesystemFreezer::simulated());
    }
    if mounts.is_empty() {
        anyhow::bail!("--real-fsfreeze requires at least one --fsfreeze-mount");
    }

    Ok(FilesystemFreezer::real(
        normalize_fsfreeze_mounts(mounts)?,
        Box::new(CommandFilesystemFreezeBackend),
    ))
}

pub(crate) fn resolve_clipboard_writer(
    capabilities: &[AgentCapability],
    command: Option<PathBuf>,
) -> Result<ClipboardWriter> {
    match command {
        Some(_) if !supports_capability(capabilities, "clipboard") => {
            anyhow::bail!("--clipboard-command requires the clipboard capability")
        }
        // Explicit path: run exactly that program with no extra arguments.
        Some(command) => Ok(ClipboardWriter::command(command)),
        // No explicit command: auto-detect a real clipboard tool when the
        // capability is enabled, otherwise stay simulated.
        None if supports_capability(capabilities, "clipboard") => {
            Ok(detect_clipboard_writer(&SystemDesktopEnv))
        }
        None => Ok(ClipboardWriter::simulated()),
    }
}

/// Resolve the opt-in guest->host clipboard watcher. Returns `None` (watcher
/// disabled, default behavior unchanged) unless `--clipboard-watch-interval-ms`
/// is greater than zero AND the clipboard capability is enabled AND a real
/// reader is available (explicit `--clipboard-read-command` or an auto-detected
/// wl-paste/xclip). A configured interval with no usable reader is a hard error
/// so the operator is not silently left without live sync.
pub(crate) fn resolve_clipboard_watcher(
    capabilities: &[AgentCapability],
    interval_ms: u64,
    command: Option<PathBuf>,
) -> Result<Option<ClipboardWatcher>> {
    if interval_ms == 0 {
        if command.is_some() {
            anyhow::bail!(
                "--clipboard-read-command requires --clipboard-watch-interval-ms greater than zero"
            );
        }
        return Ok(None);
    }
    if !supports_capability(capabilities, "clipboard") {
        anyhow::bail!("--clipboard-watch-interval-ms requires the clipboard capability");
    }

    let reader = match command {
        // Explicit path: read exactly that program's stdout, no extra args.
        Some(command) => ClipboardReader::command(command),
        // No explicit command: auto-detect a real clipboard reader.
        None => detect_clipboard_reader(&SystemDesktopEnv),
    };
    if reader.command_path().is_none() {
        anyhow::bail!(
            "--clipboard-watch-interval-ms could not find a clipboard reader (install wl-paste or xclip, or pass --clipboard-read-command)"
        );
    }

    Ok(Some(ClipboardWatcher {
        interval: Duration::from_millis(interval_ms),
        reader,
    }))
}

pub(crate) fn resolve_display_resizer(
    capabilities: &[AgentCapability],
    command: Option<PathBuf>,
) -> Result<DisplayResizer> {
    match command {
        Some(_) if !supports_capability(capabilities, "display-resize") => {
            anyhow::bail!("--display-resize-command requires the display-resize capability")
        }
        // Explicit path: run exactly that program (it receives WIDTH HEIGHT SCALE).
        Some(command) => Ok(DisplayResizer::command(command)),
        // No explicit command: auto-detect a real resize tool when the
        // capability is enabled, otherwise stay simulated.
        None if supports_capability(capabilities, "display-resize") => {
            Ok(detect_display_resizer(&SystemDesktopEnv))
        }
        None => Ok(DisplayResizer::simulated()),
    }
}

/// Reports facts about the guest desktop session used to auto-detect the right
/// clipboard/resize tool. Injectable so unit tests can supply a fake without
/// touching the real environment, PATH, or running xclip/xrandr.
pub(crate) trait DesktopEnv {
    /// Whether an environment variable (e.g. `WAYLAND_DISPLAY`/`DISPLAY`) is set.
    fn has_env(&self, name: &str) -> bool;
    /// Whether an executable is resolvable on `PATH`.
    fn has_program(&self, program: &str) -> bool;
    /// Resolved executable path for programs that will be launched later.
    fn program_path(&self, program: &str) -> Option<PathBuf>;
}

/// Real implementation backed by the process environment and `PATH`.
pub(crate) struct SystemDesktopEnv;

/// Clipboard auto-detection: prefer Wayland's `wl-copy` (no args), fall back to
/// X11's `xclip -selection clipboard`, otherwise simulated.
pub(crate) fn detect_clipboard_writer(env: &impl DesktopEnv) -> ClipboardWriter {
    if env.has_env("WAYLAND_DISPLAY") && env.has_program("wl-copy") {
        ClipboardWriter::command(PathBuf::from("wl-copy"))
    } else if env.has_env("DISPLAY") && env.has_program("xclip") {
        ClipboardWriter::command_with_args(
            PathBuf::from("xclip"),
            vec!["-selection".to_string(), "clipboard".to_string()],
        )
    } else {
        ClipboardWriter::simulated()
    }
}

/// Clipboard-reader auto-detection (mirrors `detect_clipboard_writer`): prefer
/// Wayland's `wl-paste --no-newline`, fall back to X11's
/// `xclip -selection clipboard -o`, otherwise simulated (no reader).
pub(crate) fn detect_clipboard_reader(env: &impl DesktopEnv) -> ClipboardReader {
    if env.has_env("WAYLAND_DISPLAY") && env.has_program("wl-paste") {
        ClipboardReader::command_with_args(
            PathBuf::from("wl-paste"),
            vec!["--no-newline".to_string()],
        )
    } else if env.has_env("DISPLAY") && env.has_program("xclip") {
        ClipboardReader::command_with_args(
            PathBuf::from("xclip"),
            vec![
                "-selection".to_string(),
                "clipboard".to_string(),
                "-o".to_string(),
            ],
        )
    } else {
        ClipboardReader::simulated()
    }
}

/// Display-resize auto-detection: use X11's `xrandr` (it receives WIDTH HEIGHT
/// SCALE as arguments, like an explicit command), otherwise simulated.
pub(crate) fn detect_display_resizer(env: &impl DesktopEnv) -> DisplayResizer {
    if env.has_env("DISPLAY") && env.has_program("xrandr") {
        DisplayResizer::command(PathBuf::from("xrandr"))
    } else {
        DisplayResizer::simulated()
    }
}

pub(crate) fn resolve_desktop_controller(capabilities: &[AgentCapability]) -> DesktopController {
    let applications = supports_capability(capabilities, "applications");
    let windows = supports_capability(capabilities, "windows");
    if !applications && !windows {
        return DesktopController::simulated();
    }

    detect_desktop_controller(&SystemDesktopEnv, applications, windows)
}

pub(crate) fn detect_desktop_controller(
    env: &impl DesktopEnv,
    applications: bool,
    windows: bool,
) -> DesktopController {
    let app_launcher = if applications {
        if let Some(program) = env.program_path("gio") {
            Some(AppLauncher::Gio(program))
        } else {
            env.program_path("gtk-launch").map(AppLauncher::GtkLaunch)
        }
    } else {
        None
    };
    let window_tool = if windows && env.has_env("DISPLAY") {
        env.program_path("wmctrl")
    } else {
        None
    };
    let input_tool = if windows && env.has_env("DISPLAY") {
        env.program_path("xdotool")
    } else {
        None
    };

    if app_launcher.is_some() || window_tool.is_some() || input_tool.is_some() {
        DesktopController::real(app_launcher, window_tool, input_tool)
    } else {
        DesktopController::simulated()
    }
}

pub(crate) fn resolve_clock_setter(
    capabilities: &[AgentCapability],
    no_real_time_sync: bool,
) -> ClockSetter {
    // Real clock sync is only meaningful when time-sync is negotiated; if the
    // capability is absent the handler rejects the command before we ever try
    // to set the clock, so a simulated setter is the honest default there.
    if no_real_time_sync || !supports_capability(capabilities, "time-sync") {
        ClockSetter::simulated()
    } else {
        ClockSetter::real(Box::new(SettimeofdayClockBackend))
    }
}

impl DesktopEnv for SystemDesktopEnv {
    fn has_env(&self, name: &str) -> bool {
        std::env::var_os(name).is_some_and(|value| !value.is_empty())
    }

    fn has_program(&self, program: &str) -> bool {
        self.program_path(program).is_some()
    }

    fn program_path(&self, program: &str) -> Option<PathBuf> {
        let path = std::env::var_os("PATH")?;
        std::env::split_paths(&path)
            .map(|dir| dir.join(program))
            .find(|path| path.is_file())
    }
}
