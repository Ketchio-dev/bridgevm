//! Split out of main.rs to keep files under 800 lines.

use crate::*;
use anyhow::Context;
use anyhow::Result;
use bridgevm_agent_protocol::AgentCapability;
use bridgevm_agent_protocol::WindowInputEvent;
use clap::Parser;
use std::fs;
use std::fs::OpenOptions;
use std::io::Read;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;
use std::time::Instant;

pub(crate) const EFFECT_COMMAND_PATH: &str = "/usr/bin:/bin:/usr/sbin:/sbin";
/// Hard cap so a configured effect command that hangs (e.g. a daemonizing child)
/// can't wedge the single-threaded agent forever.
pub(crate) const EFFECT_COMMAND_TIMEOUT: Duration = Duration::from_secs(5);
pub(crate) const MAX_DESKTOP_COMMAND_OUTPUT_BYTES: usize = 4 * 1024 * 1024;
pub(crate) const MAX_CLIPBOARD_OUTPUT_BYTES: usize = 512 * 1024;
pub(crate) const OUTPUT_DRAIN_BUFFER_BYTES: usize = 64 * 1024;
pub(crate) const MAX_DESKTOP_FILE_BYTES: usize = 1024 * 1024;
pub(crate) const MAX_TOKEN_FILE_BYTES: usize = 64 * 1024;
pub(crate) const MAX_PROC_TEXT_BYTES: usize = 1024 * 1024;
pub(crate) const DEFAULT_FSFREEZE_TIMEOUT: Duration = Duration::from_secs(30);
pub(crate) const MAX_FSFREEZE_OUTPUT_BYTES: usize = 64 * 1024;

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

pub(crate) const AGENT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Parser)]
#[command(
    name = "bridgevm-tools-linux",
    about = "BridgeVM Linux guest tools scaffold"
)]
pub(crate) struct Args {
    #[arg(long, value_name = "PATH")]
    pub(crate) socket: Option<PathBuf>,
    #[arg(long, value_name = "PATH")]
    pub(crate) device: Option<PathBuf>,
    #[arg(long, value_name = "TOKEN")]
    pub(crate) token: Option<String>,
    #[arg(long, value_name = "PATH")]
    pub(crate) token_file: Option<PathBuf>,
    #[arg(long, default_value = "linux")]
    pub(crate) guest_os: String,
    #[arg(long)]
    pub(crate) serve_once: bool,
    #[arg(long = "capability", value_name = "NAME[:VERSION]")]
    pub(crate) capabilities: Vec<String>,
    #[arg(long = "guest-ip", value_name = "ADDR[@IFACE]")]
    pub(crate) guest_ips: Vec<String>,
    #[arg(long)]
    pub(crate) no_guest_ip: bool,
    #[arg(long, default_value_t = 1)]
    pub(crate) metrics_cpu_percent: u8,
    #[arg(long, default_value_t = 256)]
    pub(crate) metrics_memory_used_mib: u64,
    #[arg(long)]
    pub(crate) no_metrics: bool,
    #[arg(long, value_name = "TEXT")]
    pub(crate) clipboard_text: Option<String>,
    #[arg(long, value_name = "PATH")]
    pub(crate) clipboard_command: Option<PathBuf>,
    /// Poll the real guest OS clipboard every <MS> milliseconds and emit a
    /// guest-origin `ClipboardChanged` frame whenever its text changes. Default
    /// 0 disables the watcher (preserving the prior synthetic-only behavior);
    /// the watcher only runs when this is > 0, the clipboard capability is
    /// enabled, and a real clipboard reader (wl-paste/xclip) is detected.
    #[arg(long, value_name = "MS", default_value_t = 0)]
    pub(crate) clipboard_watch_interval_ms: u64,
    /// Explicit clipboard reader program for `--clipboard-watch-interval-ms`
    /// (runs with no extra args, its stdout is the clipboard text). When unset
    /// the watcher auto-detects wl-paste/xclip.
    #[arg(long, value_name = "PATH")]
    pub(crate) clipboard_read_command: Option<PathBuf>,
    #[arg(long, value_name = "PATH")]
    pub(crate) display_resize_command: Option<PathBuf>,
    #[arg(long, value_name = "DIR")]
    pub(crate) file_drop_dir: Option<PathBuf>,
    #[arg(long)]
    pub(crate) real_fsfreeze: bool,
    #[arg(long = "fsfreeze-mount", value_name = "MOUNT")]
    pub(crate) fsfreeze_mounts: Vec<PathBuf>,
    /// Do NOT apply host TimeSync commands to the real guest clock; only
    /// acknowledge them. By default a booted guest applies the host epoch to
    /// its real clock via settimeofday(2) (the agent runs as root under
    /// cloud-init).
    #[arg(long)]
    pub(crate) no_real_time_sync: bool,
    /// Do NOT read real /proc metrics for the startup GuestMetrics frame; use
    /// the synthetic --metrics-* values instead. By default the agent reports
    /// real guest memory + CPU/load read from /proc.
    #[arg(long)]
    pub(crate) no_real_metrics: bool,
}

pub(crate) fn run() -> Result<()> {
    let args = Args::parse();
    let Some(transport) = resolve_transport(args.socket, args.device)? else {
        println!("bridgevm-tools-linux ready");
        return Ok(());
    };
    let token = resolve_token(args.token, args.token_file)?;
    let capabilities = resolve_capabilities(&args.capabilities)?;
    let filesystem_freezer = resolve_filesystem_freezer(args.real_fsfreeze, args.fsfreeze_mounts)?;
    let clipboard_writer = resolve_clipboard_writer(&capabilities, args.clipboard_command)?;
    let clipboard_watcher = resolve_clipboard_watcher(
        &capabilities,
        args.clipboard_watch_interval_ms,
        args.clipboard_read_command,
    )?;
    let display_resizer = resolve_display_resizer(&capabilities, args.display_resize_command)?;
    let clock_setter = resolve_clock_setter(&capabilities, args.no_real_time_sync);
    let desktop_controller = resolve_desktop_controller(&capabilities);
    let telemetry = TelemetryConfig::from_args(
        &capabilities,
        &args.guest_ips,
        args.no_guest_ip,
        args.metrics_cpu_percent,
        args.metrics_memory_used_mib,
        args.no_metrics,
        args.no_real_metrics,
        args.clipboard_text,
    )?;

    match transport {
        GuestToolsTransport::Socket(socket) => {
            let stream = UnixStream::connect(&socket).with_context(|| {
                format!("failed to connect guest-tools socket {}", socket.display())
            })?;
            let writer = stream
                .try_clone()
                .context("failed to clone guest-tools socket")?;
            run_tools_session_watched(
                stream,
                writer,
                ToolsSessionConfig {
                    token: &token,
                    guest_os: &args.guest_os,
                    capabilities,
                    telemetry,
                    file_drop_dir: args.file_drop_dir,
                    filesystem_freezer,
                    clipboard_writer,
                    display_resizer,
                    clock_setter,
                    desktop_controller,
                    serve_once: args.serve_once,
                },
                clipboard_watcher,
            )
        }
        GuestToolsTransport::Device(device) => {
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .open(&device)
                .with_context(|| {
                    format!("failed to open guest-tools device {}", device.display())
                })?;
            let writer = file.try_clone().with_context(|| {
                format!("failed to clone guest-tools device {}", device.display())
            })?;
            run_tools_session_watched(
                file,
                writer,
                ToolsSessionConfig {
                    token: &token,
                    guest_os: &args.guest_os,
                    capabilities,
                    telemetry,
                    file_drop_dir: args.file_drop_dir,
                    filesystem_freezer,
                    clipboard_writer,
                    display_resizer,
                    clock_setter,
                    desktop_controller,
                    serve_once: args.serve_once,
                },
                clipboard_watcher,
            )
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum GuestToolsTransport {
    Socket(PathBuf),
    Device(PathBuf),
}

pub(crate) fn resolve_transport(
    socket: Option<PathBuf>,
    device: Option<PathBuf>,
) -> Result<Option<GuestToolsTransport>> {
    match (socket, device) {
        (Some(_), Some(_)) => anyhow::bail!("use either --socket or --device, not both"),
        (Some(socket), None) => Ok(Some(GuestToolsTransport::Socket(socket))),
        (None, Some(device)) => Ok(Some(GuestToolsTransport::Device(device))),
        (None, None) => Ok(None),
    }
}

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

pub(crate) struct DesktopController {
    pub(crate) mode: DesktopControllerMode,
}

pub(crate) enum DesktopControllerMode {
    Simulated,
    Real {
        app_launcher: Option<AppLauncher>,
        window_tool: Option<PathBuf>,
        input_tool: Option<PathBuf>,
    },
}

pub(crate) enum AppLauncher {
    Gio(PathBuf),
    GtkLaunch(PathBuf),
}

impl DesktopController {
    pub(crate) fn simulated() -> Self {
        Self {
            mode: DesktopControllerMode::Simulated,
        }
    }

    pub(crate) fn real(
        app_launcher: Option<AppLauncher>,
        window_tool: Option<PathBuf>,
        input_tool: Option<PathBuf>,
    ) -> Self {
        Self {
            mode: DesktopControllerMode::Real {
                app_launcher,
                window_tool,
                input_tool,
            },
        }
    }

    pub(crate) fn list_applications(&self) -> Option<CommandOutcome> {
        let DesktopControllerMode::Real {
            app_launcher: Some(_),
            ..
        } = &self.mode
        else {
            return None;
        };
        Some(match read_desktop_applications() {
            Ok(applications) => {
                let names = applications
                    .iter()
                    .map(|app| format!("{}:{}", app.id, app.name))
                    .collect::<Vec<_>>()
                    .join(",");
                let payload = applications
                    .iter()
                    .map(|app| {
                        serde_json::json!({
                            "id": app.id,
                            "name": app.name,
                            "launched": false,
                            "source": "linux-desktop-file"
                        })
                    })
                    .collect::<Vec<_>>();
                CommandOutcome::ok_with_result(
                    Some(format!("applications: {names}")),
                    serde_json::json!({ "applications": payload }),
                )
            }
            Err(message) if message == "no visible .desktop applications were found" => {
                return None;
            }
            Err(message) => CommandOutcome::error("applications-list-failed", message),
        })
    }

    pub(crate) fn launch_application(&mut self, id: &str) -> Option<CommandOutcome> {
        let DesktopControllerMode::Real {
            app_launcher: Some(app_launcher),
            ..
        } = &self.mode
        else {
            return None;
        };
        let applications = match read_desktop_applications() {
            Ok(applications) => applications,
            Err(message) if message == "no visible .desktop applications were found" => {
                return None;
            }
            Err(message) => {
                return Some(CommandOutcome::error("applications-list-failed", message))
            }
        };
        let Some(app) = applications.into_iter().find(|app| app.id == id) else {
            return Some(CommandOutcome::error(
                "application-not-found",
                format!("application {id} was not found"),
            ));
        };
        Some(match run_application_launcher(app_launcher, &app) {
            Ok(()) => CommandOutcome::ok_with_result(
                Some(format!("launched application {}", app.name)),
                serde_json::json!({
                    "application": {
                        "id": app.id,
                        "name": app.name,
                        "launched": true,
                        "source": "linux-desktop-file"
                    }
                }),
            ),
            Err(message) => CommandOutcome::error("application-launch-failed", message),
        })
    }

    pub(crate) fn list_windows(&self) -> Option<CommandOutcome> {
        let DesktopControllerMode::Real {
            window_tool: Some(window_tool),
            ..
        } = &self.mode
        else {
            return None;
        };
        Some(match read_wmctrl_windows(window_tool) {
            Ok(windows) => {
                let names = windows
                    .iter()
                    .map(|window| format!("{}:{}", window.id, window.title))
                    .collect::<Vec<_>>()
                    .join(",");
                let payload = windows
                    .iter()
                    .map(|window| {
                        let mut window_payload = desktop_window_payload(window);
                        window_payload["focused"] = serde_json::Value::Bool(false);
                        window_payload
                    })
                    .collect::<Vec<_>>();
                CommandOutcome::ok_with_result(
                    Some(format!("windows: {names}")),
                    serde_json::json!({ "windows": payload }),
                )
            }
            Err(message) if message == "wmctrl reported no desktop windows" => {
                return None;
            }
            Err(message) => CommandOutcome::error("windows-list-failed", message),
        })
    }

    pub(crate) fn focus_window(&mut self, id: &str) -> Option<CommandOutcome> {
        self.run_wmctrl_window_action(id, "-ia", "focused", "focus-window-failed")
    }

    pub(crate) fn close_window(&mut self, id: &str) -> Option<CommandOutcome> {
        self.run_wmctrl_window_action(id, "-ic", "closed", "close-window-failed")
    }

    pub(crate) fn set_window_bounds(
        &mut self,
        id: &str,
        x: i64,
        y: i64,
        width: u64,
        height: u64,
    ) -> Option<CommandOutcome> {
        let DesktopControllerMode::Real {
            window_tool: Some(window_tool),
            ..
        } = &self.mode
        else {
            return None;
        };
        let windows = match read_wmctrl_windows(window_tool) {
            Ok(windows) => windows,
            Err(message) if message == "wmctrl reported no desktop windows" => return None,
            Err(message) => return Some(CommandOutcome::error("windows-list-failed", message)),
        };
        let Some(window) = windows.into_iter().find(|window| window.id == id) else {
            return Some(CommandOutcome::error(
                "window-not-found",
                format!("window {id} was not found"),
            ));
        };

        let geometry = format!("0,{x},{y},{width},{height}");
        Some(
            match run_command_status(window_tool, &["-ir", &window.id, "-e", &geometry]) {
                Ok(()) => {
                    let mut window_payload = desktop_window_payload(&window);
                    window_payload["bounds"] = window_bounds_payload(x, y, width, height);
                    window_payload["bounds_changed"] = serde_json::Value::Bool(true);
                    CommandOutcome::ok_with_result(
                        Some(format!("set bounds for window {}", window.title)),
                        serde_json::json!({ "window": window_payload }),
                    )
                }
                Err(message) => CommandOutcome::error("window-bounds-failed", message),
            },
        )
    }

    pub(crate) fn input_window(
        &mut self,
        id: &str,
        event: &WindowInputEvent,
    ) -> Option<CommandOutcome> {
        let DesktopControllerMode::Real {
            window_tool: Some(window_tool),
            input_tool,
            ..
        } = &self.mode
        else {
            return None;
        };
        let windows = match read_wmctrl_windows(window_tool) {
            Ok(windows) => windows,
            Err(message) if message == "wmctrl reported no desktop windows" => return None,
            Err(message) => return Some(CommandOutcome::error("windows-list-failed", message)),
        };
        let Some(window) = windows.into_iter().find(|window| window.id == id) else {
            return Some(CommandOutcome::error(
                "window-not-found",
                format!("window {id} was not found"),
            ));
        };
        let Some(input_tool) = input_tool else {
            return Some(CommandOutcome::error(
                "window-input-unsupported",
                "xdotool is not available for guest window input",
            ));
        };

        if let Err(message) = run_command_status(window_tool, &["-ia", &window.id]) {
            return Some(CommandOutcome::error("window-input-focus-failed", message));
        }

        Some(match run_xdotool_window_input(input_tool, event) {
            Ok(()) => {
                let mut window_payload = desktop_window_payload(&window);
                window_payload["input"] = window_input_payload(event, "xdotool");
                CommandOutcome::ok_with_result(
                    Some(format!(
                        "sent {} input to window {}",
                        window_input_label(event),
                        window.title
                    )),
                    serde_json::json!({ "window": window_payload }),
                )
            }
            Err(message) => CommandOutcome::error("window-input-failed", message),
        })
    }

    pub(crate) fn run_wmctrl_window_action(
        &mut self,
        id: &str,
        flag: &str,
        verb: &str,
        error_code: &str,
    ) -> Option<CommandOutcome> {
        let DesktopControllerMode::Real {
            window_tool: Some(window_tool),
            ..
        } = &self.mode
        else {
            return None;
        };
        let windows = match read_wmctrl_windows(window_tool) {
            Ok(windows) => windows,
            Err(message) if message == "wmctrl reported no desktop windows" => return None,
            Err(message) => return Some(CommandOutcome::error("windows-list-failed", message)),
        };
        let Some(window) = windows.into_iter().find(|window| window.id == id) else {
            return Some(CommandOutcome::error(
                "window-not-found",
                format!("window {id} was not found"),
            ));
        };
        Some(match run_command_status(window_tool, &[flag, &window.id]) {
            Ok(()) => {
                let mut window_payload = desktop_window_payload(&window);
                window_payload[verb] = serde_json::Value::Bool(true);
                CommandOutcome::ok_with_result(
                    Some(format!("{verb} window {}", window.title)),
                    serde_json::json!({ "window": window_payload }),
                )
            }
            Err(message) => CommandOutcome::error(error_code, message),
        })
    }

    #[cfg(test)]
    pub(crate) fn is_real_for_test(&self) -> bool {
        matches!(self.mode, DesktopControllerMode::Real { .. })
    }
}

pub(crate) fn run_xdotool_window_input(
    program: &Path,
    event: &WindowInputEvent,
) -> Result<(), String> {
    match event {
        WindowInputEvent::Pointer {
            x,
            y,
            action,
            button,
        } => {
            let x_arg = x.to_string();
            let y_arg = y.to_string();
            run_command_status(program, &["mousemove", "--sync", &x_arg, &y_arg])?;
            match action.as_str() {
                "move" => Ok(()),
                "press" => {
                    let button = xdotool_button(button.as_deref())?;
                    run_command_status(program, &["mousedown", button])
                }
                "release" => {
                    let button = xdotool_button(button.as_deref())?;
                    run_command_status(program, &["mouseup", button])
                }
                "click" => {
                    let button = xdotool_button(button.as_deref())?;
                    run_command_status(program, &["click", button])
                }
                _ => Err(format!("unsupported pointer action {action}")),
            }
        }
        WindowInputEvent::Key { key, action } => match action.as_str() {
            "press" => run_command_status(program, &["keydown", key]),
            "release" => run_command_status(program, &["keyup", key]),
            "tap" => run_command_status(program, &["key", key]),
            _ => Err(format!("unsupported key action {action}")),
        },
    }
}
