//! The argument surface, transport selection, and the startup wiring that resolves every effect backend.

use crate::*;
use anyhow::Context;
use anyhow::Result;
use clap::Parser;
use std::fs::OpenOptions;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;

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
