use anyhow::{Context, Result};
use bridgevm_agent_protocol::{
    AgentAuth, AgentCapability, AgentEnvelope, AgentMessage, GuestIpAddress,
};
use bridgevm_agentd::{read_envelope_line, write_envelope_line};
use clap::Parser;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, OpenOptions},
    io::{BufReader, Read, Write},
    net::IpAddr,
    os::unix::net::UnixStream,
    path::{Component, Path, PathBuf},
    process::{Command as ProcessCommand, Stdio},
    thread,
    time::{Duration, Instant},
};

/// A trusted PATH pinned for effect commands (clipboard/resize) so an
/// auto-detected bare program name (`xclip`/`xrandr`/`wl-copy`) can only resolve
/// from system dirs, not a guest-writable directory earlier in the inherited
/// PATH (the agent runs as root). Absolute-path commands are unaffected.
const EFFECT_COMMAND_PATH: &str = "/usr/bin:/bin:/usr/sbin:/sbin";
/// Hard cap so a configured effect command that hangs (e.g. a daemonizing child)
/// can't wedge the single-threaded agent forever.
const EFFECT_COMMAND_TIMEOUT: Duration = Duration::from_secs(5);

/// Wait for `child` up to `EFFECT_COMMAND_TIMEOUT`, killing + reaping it on
/// timeout. Returns the exit status, or an error string on timeout/wait failure.
fn wait_bounded(child: &mut std::process::Child, label: &str) -> Result<std::process::ExitStatus, String> {
    let deadline = Instant::now() + EFFECT_COMMAND_TIMEOUT;
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
            Err(error) => return Err(format!("failed to wait for {label}: {error}")),
        }
    }
}

const AGENT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Parser)]
#[command(
    name = "bridgevm-tools-linux",
    about = "BridgeVM Linux guest tools scaffold"
)]
struct Args {
    #[arg(long, value_name = "PATH")]
    socket: Option<PathBuf>,
    #[arg(long, value_name = "PATH")]
    device: Option<PathBuf>,
    #[arg(long, value_name = "TOKEN")]
    token: Option<String>,
    #[arg(long, value_name = "PATH")]
    token_file: Option<PathBuf>,
    #[arg(long, default_value = "linux")]
    guest_os: String,
    #[arg(long)]
    serve_once: bool,
    #[arg(long = "capability", value_name = "NAME[:VERSION]")]
    capabilities: Vec<String>,
    #[arg(long = "guest-ip", value_name = "ADDR[@IFACE]")]
    guest_ips: Vec<String>,
    #[arg(long)]
    no_guest_ip: bool,
    #[arg(long, default_value_t = 1)]
    metrics_cpu_percent: u8,
    #[arg(long, default_value_t = 256)]
    metrics_memory_used_mib: u64,
    #[arg(long)]
    no_metrics: bool,
    #[arg(long, value_name = "TEXT")]
    clipboard_text: Option<String>,
    #[arg(long, value_name = "PATH")]
    clipboard_command: Option<PathBuf>,
    #[arg(long, value_name = "PATH")]
    display_resize_command: Option<PathBuf>,
    #[arg(long, value_name = "DIR")]
    file_drop_dir: Option<PathBuf>,
    #[arg(long)]
    real_fsfreeze: bool,
    #[arg(long = "fsfreeze-mount", value_name = "MOUNT")]
    fsfreeze_mounts: Vec<PathBuf>,
    /// Do NOT apply host TimeSync commands to the real guest clock; only
    /// acknowledge them. By default a booted guest applies the host epoch to
    /// its real clock via settimeofday(2) (the agent runs as root under
    /// cloud-init).
    #[arg(long)]
    no_real_time_sync: bool,
    /// Do NOT read real /proc metrics for the startup GuestMetrics frame; use
    /// the synthetic --metrics-* values instead. By default the agent reports
    /// real guest memory + CPU/load read from /proc.
    #[arg(long)]
    no_real_metrics: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let Some(transport) = resolve_transport(args.socket, args.device)? else {
        println!("bridgevm-tools-linux ready");
        return Ok(());
    };
    let token = resolve_token(args.token, args.token_file)?;
    let capabilities = resolve_capabilities(&args.capabilities)?;
    let filesystem_freezer = resolve_filesystem_freezer(args.real_fsfreeze, args.fsfreeze_mounts)?;
    let clipboard_writer = resolve_clipboard_writer(&capabilities, args.clipboard_command)?;
    let display_resizer = resolve_display_resizer(&capabilities, args.display_resize_command)?;
    let clock_setter = resolve_clock_setter(&capabilities, args.no_real_time_sync);
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
            let mut writer = stream
                .try_clone()
                .context("failed to clone guest-tools socket")?;
            run_tools_session(
                stream,
                &mut writer,
                &token,
                &args.guest_os,
                capabilities,
                telemetry,
                args.file_drop_dir,
                filesystem_freezer,
                clipboard_writer,
                display_resizer,
                clock_setter,
                args.serve_once,
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
            let mut writer = file.try_clone().with_context(|| {
                format!("failed to clone guest-tools device {}", device.display())
            })?;
            run_tools_session(
                file,
                &mut writer,
                &token,
                &args.guest_os,
                capabilities,
                telemetry,
                args.file_drop_dir,
                filesystem_freezer,
                clipboard_writer,
                display_resizer,
                clock_setter,
                args.serve_once,
            )
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum GuestToolsTransport {
    Socket(PathBuf),
    Device(PathBuf),
}

fn resolve_transport(
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

fn resolve_filesystem_freezer(
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

fn resolve_clipboard_writer(
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

fn resolve_display_resizer(
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
trait DesktopEnv {
    /// Whether an environment variable (e.g. `WAYLAND_DISPLAY`/`DISPLAY`) is set.
    fn has_env(&self, name: &str) -> bool;
    /// Whether an executable is resolvable on `PATH`.
    fn has_program(&self, program: &str) -> bool;
}

/// Real implementation backed by the process environment and `PATH`.
struct SystemDesktopEnv;

impl DesktopEnv for SystemDesktopEnv {
    fn has_env(&self, name: &str) -> bool {
        std::env::var_os(name).is_some_and(|value| !value.is_empty())
    }

    fn has_program(&self, program: &str) -> bool {
        let Some(path) = std::env::var_os("PATH") else {
            return false;
        };
        std::env::split_paths(&path).any(|dir| {
            !dir.as_os_str().is_empty() && dir.join(program).is_file()
        })
    }
}

/// Clipboard auto-detection: prefer Wayland's `wl-copy` (no args), fall back to
/// X11's `xclip -selection clipboard`, otherwise simulated.
fn detect_clipboard_writer(env: &impl DesktopEnv) -> ClipboardWriter {
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

/// Display-resize auto-detection: use X11's `xrandr` (it receives WIDTH HEIGHT
/// SCALE as arguments, like an explicit command), otherwise simulated.
fn detect_display_resizer(env: &impl DesktopEnv) -> DisplayResizer {
    if env.has_env("DISPLAY") && env.has_program("xrandr") {
        DisplayResizer::command(PathBuf::from("xrandr"))
    } else {
        DisplayResizer::simulated()
    }
}

fn resolve_clock_setter(capabilities: &[AgentCapability], no_real_time_sync: bool) -> ClockSetter {
    // Real clock sync is only meaningful when time-sync is negotiated; if the
    // capability is absent the handler rejects the command before we ever try
    // to set the clock, so a simulated setter is the honest default there.
    if no_real_time_sync || !supports_capability(capabilities, "time-sync") {
        ClockSetter::simulated()
    } else {
        ClockSetter::real(Box::new(SettimeofdayClockBackend))
    }
}

fn normalize_fsfreeze_mounts(mounts: Vec<PathBuf>) -> Result<Vec<PathBuf>> {
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

fn normalize_absolute_path(path: &Path) -> Result<PathBuf> {
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

fn run_tools_session(
    reader: impl Read,
    writer: &mut impl Write,
    token: &str,
    guest_os: &str,
    capabilities: Vec<AgentCapability>,
    telemetry: TelemetryConfig,
    file_drop_dir: Option<PathBuf>,
    filesystem_freezer: FilesystemFreezer,
    clipboard_writer: ClipboardWriter,
    display_resizer: DisplayResizer,
    clock_setter: ClockSetter,
    serve_once: bool,
) -> Result<()> {
    let mut state = GuestToolsState::new(&capabilities)
        .with_file_drop_dir(file_drop_dir)
        .with_filesystem_freezer(filesystem_freezer)
        .with_clipboard_writer(clipboard_writer)
        .with_display_resizer(display_resizer)
        .with_clock_setter(clock_setter);
    let hello = guest_hello(token, guest_os, capabilities);
    write_envelope_line(writer, &hello).map_err(|error| anyhow::anyhow!("{error:?}"))?;
    for envelope in initial_status_envelopes(&telemetry) {
        write_envelope_line(writer, &envelope).map_err(|error| anyhow::anyhow!("{error:?}"))?;
    }

    let mut reader = BufReader::new(reader);
    let mut handled_commands = 0usize;
    while let Some(command) =
        read_envelope_line(&mut reader).map_err(|error| anyhow::anyhow!("{error:?}"))?
    {
        if let Some(result) = state.handle_command(&command) {
            write_envelope_line(writer, &result).map_err(|error| anyhow::anyhow!("{error:?}"))?;
        }
        handled_commands += 1;
        if serve_once && handled_commands >= 1 {
            break;
        }
    }

    Ok(())
}

struct GuestToolsState {
    shared_folders_supported: bool,
    drag_drop_supported: bool,
    applications_supported: bool,
    windows_supported: bool,
    clipboard_supported: bool,
    display_resize_supported: bool,
    fs_freeze_supported: bool,
    fs_thaw_supported: bool,
    time_sync_supported: bool,
    shared_folders: BTreeMap<String, SharedFolderMount>,
    file_drops: BTreeMap<String, FileDropTransfer>,
    applications: BTreeMap<String, ApplicationEntry>,
    windows: BTreeMap<String, WindowEntry>,
    file_drop_dir: Option<PathBuf>,
    filesystem_frozen: bool,
    filesystem_freezer: FilesystemFreezer,
    clipboard_writer: ClipboardWriter,
    display_resizer: DisplayResizer,
    clock_setter: ClockSetter,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SharedFolderMount {
    host_path_token: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FileDropTransfer {
    file_name: String,
    size_bytes: u64,
    bytes: Vec<u8>,
    chunks_seen: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ApplicationEntry {
    name: String,
    launched: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct WindowEntry {
    title: String,
    focused: bool,
    closed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CommandOutcome {
    ok: bool,
    error_code: Option<String>,
    message: Option<String>,
    result: Option<serde_json::Value>,
    metadata: Option<serde_json::Value>,
}

impl CommandOutcome {
    fn ok(message: impl Into<Option<String>>) -> Self {
        Self {
            ok: true,
            error_code: None,
            message: message.into(),
            result: None,
            metadata: None,
        }
    }

    fn ok_with_result(
        message: impl Into<Option<String>>,
        result: impl Into<serde_json::Value>,
    ) -> Self {
        Self {
            ok: true,
            error_code: None,
            message: message.into(),
            result: Some(result.into()),
            metadata: None,
        }
    }

    fn error(error_code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            ok: false,
            error_code: Some(error_code.into()),
            message: Some(message.into()),
            result: None,
            metadata: None,
        }
    }
}

impl GuestToolsState {
    fn new(capabilities: &[AgentCapability]) -> Self {
        Self {
            shared_folders_supported: supports_capability(capabilities, "shared-folders"),
            drag_drop_supported: supports_capability(capabilities, "drag-drop"),
            applications_supported: supports_capability(capabilities, "applications"),
            windows_supported: supports_capability(capabilities, "windows"),
            clipboard_supported: supports_capability(capabilities, "clipboard"),
            display_resize_supported: supports_capability(capabilities, "display-resize"),
            fs_freeze_supported: supports_capability(capabilities, "fs-freeze"),
            fs_thaw_supported: supports_capability(capabilities, "fs-thaw"),
            time_sync_supported: supports_capability(capabilities, "time-sync"),
            shared_folders: BTreeMap::new(),
            file_drops: BTreeMap::new(),
            applications: default_applications(),
            windows: default_windows(),
            file_drop_dir: None,
            filesystem_frozen: false,
            filesystem_freezer: FilesystemFreezer::simulated(),
            clipboard_writer: ClipboardWriter::simulated(),
            display_resizer: DisplayResizer::simulated(),
            clock_setter: ClockSetter::simulated(),
        }
    }

    fn with_file_drop_dir(mut self, file_drop_dir: Option<PathBuf>) -> Self {
        self.file_drop_dir = file_drop_dir;
        self
    }

    fn with_filesystem_freezer(mut self, filesystem_freezer: FilesystemFreezer) -> Self {
        self.filesystem_freezer = filesystem_freezer;
        self
    }

    fn with_clipboard_writer(mut self, clipboard_writer: ClipboardWriter) -> Self {
        self.clipboard_writer = clipboard_writer;
        self
    }

    fn with_display_resizer(mut self, display_resizer: DisplayResizer) -> Self {
        self.display_resizer = display_resizer;
        self
    }

    fn with_clock_setter(mut self, clock_setter: ClockSetter) -> Self {
        self.clock_setter = clock_setter;
        self
    }

    fn handle_command(&mut self, command: &AgentEnvelope) -> Option<AgentEnvelope> {
        let outcome = self.apply_command(&command.message);
        let request_id = command.request_id.as_ref()?;

        Some(AgentEnvelope::new(AgentMessage::CommandResult {
            request_id: request_id.clone(),
            ok: outcome.ok,
            error_code: outcome.error_code,
            message: outcome.message,
            result: outcome.result,
            metadata: outcome.metadata,
        }))
    }

    fn apply_command(&mut self, message: &AgentMessage) -> CommandOutcome {
        match message {
            AgentMessage::TimeSync { unix_epoch_millis } => self.sync_time(*unix_epoch_millis),
            AgentMessage::ResizeDisplay {
                width,
                height,
                scale,
            } => self.resize_display(*width, *height, *scale),
            AgentMessage::SetClipboard { text } => self.set_clipboard(text),
            AgentMessage::MountShare {
                name,
                host_path_token,
            } => self.mount_share(name, host_path_token),
            AgentMessage::UnmountShare { name } => self.unmount_share(name),
            AgentMessage::FileDropStart {
                transfer_id,
                file_name,
                size_bytes,
            } => self.start_file_drop(transfer_id, file_name, *size_bytes),
            AgentMessage::FileDropChunk {
                transfer_id,
                chunk_index,
                data_base64,
            } => self.record_file_drop_chunk(transfer_id, *chunk_index, data_base64),
            AgentMessage::FileDropComplete { transfer_id } => self.complete_file_drop(transfer_id),
            AgentMessage::ListApplications => self.list_applications(),
            AgentMessage::LaunchApplication { id } => self.launch_application(id),
            AgentMessage::ListWindows => self.list_windows(),
            AgentMessage::FocusWindow { id } => self.focus_window(id),
            AgentMessage::CloseWindow { id } => self.close_window(id),
            AgentMessage::FreezeFilesystem { timeout_millis } => {
                self.freeze_filesystem(*timeout_millis)
            }
            AgentMessage::ThawFilesystem => self.thaw_filesystem(),
            _ => CommandOutcome::error(
                "unsupported-command",
                "command is not implemented by the Linux tools scaffold",
            ),
        }
    }

    fn sync_time(&mut self, unix_epoch_millis: u64) -> CommandOutcome {
        if !self.time_sync_supported {
            return CommandOutcome::error(
                "capability-not-enabled",
                "time-sync capability is not enabled",
            );
        }

        match self.clock_setter.set_epoch_millis(unix_epoch_millis) {
            Ok(message) => CommandOutcome {
                ok: true,
                error_code: None,
                message,
                result: Some(serde_json::json!({
                    "applied_unix_epoch_millis": unix_epoch_millis,
                })),
                metadata: None,
            },
            Err(message) => CommandOutcome::error("time-sync-failed", message),
        }
    }

    fn mount_share(&mut self, name: &str, host_path_token: &str) -> CommandOutcome {
        if !self.shared_folders_supported {
            return CommandOutcome::error(
                "capability-not-enabled",
                "shared folders capability is not enabled",
            );
        }

        let existed = self.shared_folders.insert(
            name.to_string(),
            SharedFolderMount {
                host_path_token: host_path_token.to_string(),
            },
        );
        if existed.is_some() {
            CommandOutcome::ok(Some(format!("accepted share update for {name}")))
        } else {
            CommandOutcome::ok(Some(format!("accepted mount request for share {name}")))
        }
    }

    fn unmount_share(&mut self, name: &str) -> CommandOutcome {
        if !self.shared_folders_supported {
            return CommandOutcome::error(
                "capability-not-enabled",
                "shared folders capability is not enabled",
            );
        }

        if self.shared_folders.remove(name).is_some() {
            CommandOutcome::ok(Some(format!("accepted unmount request for share {name}")))
        } else {
            CommandOutcome::error("share-not-mounted", format!("share {name} is not mounted"))
        }
    }

    fn start_file_drop(
        &mut self,
        transfer_id: &str,
        file_name: &str,
        size_bytes: u64,
    ) -> CommandOutcome {
        if !self.drag_drop_supported {
            return CommandOutcome::error(
                "capability-not-enabled",
                "drag-and-drop capability is not enabled",
            );
        }
        if self.file_drops.contains_key(transfer_id) {
            return CommandOutcome::error(
                "transfer-already-started",
                format!("file drop {transfer_id} is already active"),
            );
        }

        self.file_drops.insert(
            transfer_id.to_string(),
            FileDropTransfer {
                file_name: file_name.to_string(),
                size_bytes,
                bytes: Vec::new(),
                chunks_seen: 0,
            },
        );
        CommandOutcome::ok(Some(format!("started file drop {transfer_id}")))
    }

    fn record_file_drop_chunk(
        &mut self,
        transfer_id: &str,
        chunk_index: u32,
        data_base64: &str,
    ) -> CommandOutcome {
        if !self.drag_drop_supported {
            return CommandOutcome::error(
                "capability-not-enabled",
                "drag-and-drop capability is not enabled",
            );
        }
        let Some(transfer) = self.file_drops.get_mut(transfer_id) else {
            return CommandOutcome::error(
                "transfer-not-started",
                format!("file drop {transfer_id} has not started"),
            );
        };
        let chunk = match decode_base64(data_base64) {
            Ok(chunk) => chunk,
            Err(message) => return CommandOutcome::error("invalid-file-drop-chunk", message),
        };

        transfer.bytes.extend_from_slice(&chunk);
        transfer.chunks_seen = transfer.chunks_seen.max(chunk_index.saturating_add(1));
        CommandOutcome::ok(Some(format!(
            "accepted file drop {transfer_id} chunk {chunk_index}"
        )))
    }

    fn complete_file_drop(&mut self, transfer_id: &str) -> CommandOutcome {
        if !self.drag_drop_supported {
            return CommandOutcome::error(
                "capability-not-enabled",
                "drag-and-drop capability is not enabled",
            );
        }
        let Some(transfer) = self.file_drops.get(transfer_id) else {
            return CommandOutcome::error(
                "transfer-not-started",
                format!("file drop {transfer_id} has not started"),
            );
        };
        if transfer.bytes.len() as u64 != transfer.size_bytes {
            return CommandOutcome::error(
                "transfer-size-mismatch",
                format!(
                    "file drop {} expected {} bytes but received {}",
                    transfer.file_name,
                    transfer.size_bytes,
                    transfer.bytes.len()
                ),
            );
        }
        if let Some(file_drop_dir) = &self.file_drop_dir {
            let Some(destination) = safe_file_drop_destination(file_drop_dir, &transfer.file_name)
            else {
                return CommandOutcome::error(
                    "unsafe-file-name",
                    format!("file drop file name is not safe: {}", transfer.file_name),
                );
            };
            if let Err(error) = fs::create_dir_all(file_drop_dir) {
                return CommandOutcome::error(
                    "file-drop-write-failed",
                    format!(
                        "failed to create file drop directory {}: {error}",
                        file_drop_dir.display()
                    ),
                );
            }
            if let Err(error) = fs::write(&destination, &transfer.bytes) {
                return CommandOutcome::error(
                    "file-drop-write-failed",
                    format!(
                        "failed to write file drop {}: {error}",
                        destination.display()
                    ),
                );
            }
        }
        let transfer = self
            .file_drops
            .remove(transfer_id)
            .expect("transfer was checked above");

        let mut message = format!(
            "completed file drop {} ({} bytes across {} chunks)",
            transfer.file_name, transfer.size_bytes, transfer.chunks_seen
        );
        if let Some(file_drop_dir) = &self.file_drop_dir {
            if let Some(destination) =
                safe_file_drop_destination(file_drop_dir, &transfer.file_name)
            {
                message.push_str(&format!(" at {}", destination.display()));
            }
        }
        CommandOutcome::ok(Some(message))
    }

    fn list_applications(&self) -> CommandOutcome {
        if !self.applications_supported {
            return CommandOutcome::error(
                "capability-not-enabled",
                "applications capability is not enabled",
            );
        }

        let names = self
            .applications
            .iter()
            .map(|(id, app)| format!("{id}:{}", app.name))
            .collect::<Vec<_>>()
            .join(",");
        let applications = self
            .applications
            .iter()
            .map(|(id, app)| {
                serde_json::json!({
                    "id": id,
                    "name": app.name,
                    "launched": app.launched
                })
            })
            .collect::<Vec<_>>();
        CommandOutcome::ok_with_result(
            Some(format!("applications: {names}")),
            serde_json::json!({ "applications": applications }),
        )
    }

    fn launch_application(&mut self, id: &str) -> CommandOutcome {
        if !self.applications_supported {
            return CommandOutcome::error(
                "capability-not-enabled",
                "applications capability is not enabled",
            );
        }
        let Some(app) = self.applications.get_mut(id) else {
            return CommandOutcome::error(
                "application-not-found",
                format!("application {id} was not found"),
            );
        };

        app.launched = true;
        CommandOutcome::ok_with_result(
            Some(format!(
                "accepted launch request for application {}",
                app.name
            )),
            serde_json::json!({
                "application": {
                    "id": id,
                    "name": app.name,
                    "launched": app.launched
                }
            }),
        )
    }

    fn list_windows(&self) -> CommandOutcome {
        if !self.windows_supported {
            return CommandOutcome::error(
                "capability-not-enabled",
                "windows capability is not enabled",
            );
        }

        let windows = self
            .windows
            .iter()
            .filter(|(_, window)| !window.closed)
            .map(|(id, window)| format!("{id}:{}", window.title))
            .collect::<Vec<_>>()
            .join(",");
        let window_payload = self
            .windows
            .iter()
            .filter(|(_, window)| !window.closed)
            .map(|(id, window)| {
                serde_json::json!({
                    "id": id,
                    "title": window.title,
                    "focused": window.focused
                })
            })
            .collect::<Vec<_>>();
        CommandOutcome::ok_with_result(
            Some(format!("windows: {windows}")),
            serde_json::json!({ "windows": window_payload }),
        )
    }

    fn focus_window(&mut self, id: &str) -> CommandOutcome {
        if !self.windows_supported {
            return CommandOutcome::error(
                "capability-not-enabled",
                "windows capability is not enabled",
            );
        }
        if !self.windows.get(id).is_some_and(|window| !window.closed) {
            return CommandOutcome::error("window-not-found", format!("window {id} was not found"));
        }

        for window in self.windows.values_mut() {
            window.focused = false;
        }
        let window = self.windows.get_mut(id).expect("window checked above");
        window.focused = true;
        CommandOutcome::ok_with_result(
            Some(format!(
                "accepted focus request for window {}",
                window.title
            )),
            serde_json::json!({
                "window": {
                    "id": id,
                    "title": window.title,
                    "focused": window.focused
                }
            }),
        )
    }

    fn close_window(&mut self, id: &str) -> CommandOutcome {
        if !self.windows_supported {
            return CommandOutcome::error(
                "capability-not-enabled",
                "windows capability is not enabled",
            );
        }
        let Some(window) = self.windows.get_mut(id) else {
            return CommandOutcome::error("window-not-found", format!("window {id} was not found"));
        };
        if window.closed {
            return CommandOutcome::error("window-not-found", format!("window {id} was not found"));
        }

        window.closed = true;
        window.focused = false;
        CommandOutcome::ok_with_result(
            Some(format!("closed window {}", window.title)),
            serde_json::json!({
                "window": {
                    "id": id,
                    "title": window.title,
                    "closed": window.closed
                }
            }),
        )
    }

    fn freeze_filesystem(&mut self, timeout_millis: Option<u64>) -> CommandOutcome {
        if !self.fs_freeze_supported {
            return CommandOutcome::error(
                "capability-not-enabled",
                "filesystem freeze capability is not enabled",
            );
        }
        if self.filesystem_frozen {
            return CommandOutcome::error(
                "filesystem-already-frozen",
                "filesystem freeze scaffold boundary is already active",
            );
        }

        match self.filesystem_freezer.freeze(timeout_millis) {
            Ok(message) => {
                self.filesystem_frozen = true;
                CommandOutcome::ok(Some(message))
            }
            Err(message) => CommandOutcome::error("filesystem-freeze-failed", message),
        }
    }

    fn thaw_filesystem(&mut self) -> CommandOutcome {
        if !self.fs_thaw_supported {
            return CommandOutcome::error(
                "capability-not-enabled",
                "filesystem thaw capability is not enabled",
            );
        }
        if !self.filesystem_frozen {
            return CommandOutcome::error(
                "filesystem-not-frozen",
                "filesystem thaw scaffold boundary is not active",
            );
        }

        match self.filesystem_freezer.thaw() {
            Ok(message) => {
                self.filesystem_frozen = false;
                CommandOutcome::ok(Some(message))
            }
            Err(message) => CommandOutcome::error("filesystem-thaw-failed", message),
        }
    }

    fn set_clipboard(&mut self, text: &str) -> CommandOutcome {
        if !self.clipboard_supported {
            return CommandOutcome::error(
                "capability-not-enabled",
                "clipboard capability is not enabled",
            );
        }

        match self.clipboard_writer.write_text(text) {
            Ok(message) => CommandOutcome::ok(message),
            Err(message) => CommandOutcome::error("clipboard-write-failed", message),
        }
    }

    fn resize_display(&mut self, width: u32, height: u32, scale: u16) -> CommandOutcome {
        if !self.display_resize_supported {
            return CommandOutcome::error(
                "capability-not-enabled",
                "display resize capability is not enabled",
            );
        }

        match self.display_resizer.resize(width, height, scale) {
            Ok(message) => CommandOutcome::ok(message),
            Err(message) => CommandOutcome::error("display-resize-failed", message),
        }
    }
}

struct ClipboardWriter {
    mode: ClipboardWriterMode,
}

enum ClipboardWriterMode {
    Simulated,
    Command { program: PathBuf, args: Vec<String> },
}

impl ClipboardWriter {
    fn simulated() -> Self {
        Self {
            mode: ClipboardWriterMode::Simulated,
        }
    }

    /// Explicit `--clipboard-command <path>`: run the given program with no
    /// extra arguments, exactly as before.
    fn command(program: PathBuf) -> Self {
        Self::command_with_args(program, Vec::new())
    }

    /// Auto-detected clipboard tools (e.g. `xclip -selection clipboard`) carry
    /// their own arguments ahead of the piped clipboard text.
    fn command_with_args(program: PathBuf, args: Vec<String>) -> Self {
        Self {
            mode: ClipboardWriterMode::Command { program, args },
        }
    }

    fn write_text(&mut self, text: &str) -> Result<Option<String>, String> {
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
    fn command_for_test(&self) -> Option<(&Path, &[String])> {
        match &self.mode {
            ClipboardWriterMode::Simulated => None,
            ClipboardWriterMode::Command { program, args } => Some((program, args)),
        }
    }
}

fn run_clipboard_command(program: &Path, args: &[String], text: &str) -> Result<(), String> {
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

struct DisplayResizer {
    mode: DisplayResizerMode,
}

enum DisplayResizerMode {
    Simulated,
    Command { command: PathBuf },
}

impl DisplayResizer {
    fn simulated() -> Self {
        Self {
            mode: DisplayResizerMode::Simulated,
        }
    }

    fn command(command: PathBuf) -> Self {
        Self {
            mode: DisplayResizerMode::Command { command },
        }
    }

    fn resize(&mut self, width: u32, height: u32, scale: u16) -> Result<Option<String>, String> {
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
    fn command_for_test(&self) -> Option<&Path> {
        match &self.mode {
            DisplayResizerMode::Simulated => None,
            DisplayResizerMode::Command { command } => Some(command),
        }
    }
}

fn run_display_resize_command(
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
struct ClockSetter {
    mode: ClockSetterMode,
}

enum ClockSetterMode {
    /// Acknowledge the host epoch without touching the real clock (used on
    /// non-Linux builds, when --no-real-time-sync is passed, or in tests).
    Simulated,
    /// Apply the host epoch to the real guest clock through the backend.
    Real { backend: Box<dyn ClockBackend> },
}

impl ClockSetter {
    fn simulated() -> Self {
        Self {
            mode: ClockSetterMode::Simulated,
        }
    }

    fn real(backend: Box<dyn ClockBackend>) -> Self {
        Self {
            mode: ClockSetterMode::Real { backend },
        }
    }

    /// Returns an optional human-readable message on success.
    fn set_epoch_millis(&mut self, unix_epoch_millis: u64) -> Result<Option<String>, String> {
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

trait ClockBackend {
    fn set_epoch_millis(&mut self, unix_epoch_millis: u64) -> Result<(), String>;
}

/// Real Linux backend: set the wall clock with settimeofday(2). The agent runs
/// as root under cloud-init, so CAP_SYS_TIME is available.
struct SettimeofdayClockBackend;

impl ClockBackend for SettimeofdayClockBackend {
    fn set_epoch_millis(&mut self, unix_epoch_millis: u64) -> Result<(), String> {
        set_system_clock_millis(unix_epoch_millis)
    }
}

#[cfg(target_os = "linux")]
fn set_system_clock_millis(unix_epoch_millis: u64) -> Result<(), String> {
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
fn set_system_clock_millis(_unix_epoch_millis: u64) -> Result<(), String> {
    Err("real clock sync is only supported on Linux guests".to_string())
}

struct FilesystemFreezer {
    mode: FilesystemFreezerMode,
}

enum FilesystemFreezerMode {
    Simulated,
    Real {
        mounts: Vec<PathBuf>,
        frozen_mounts: Vec<PathBuf>,
        backend: Box<dyn FilesystemFreezeBackend>,
    },
}

impl FilesystemFreezer {
    fn simulated() -> Self {
        Self {
            mode: FilesystemFreezerMode::Simulated,
        }
    }

    fn real(mounts: Vec<PathBuf>, backend: Box<dyn FilesystemFreezeBackend>) -> Self {
        Self {
            mode: FilesystemFreezerMode::Real {
                mounts,
                frozen_mounts: Vec::new(),
                backend,
            },
        }
    }

    fn freeze(&mut self, timeout_millis: Option<u64>) -> Result<String, String> {
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

    fn thaw(&mut self) -> Result<String, String> {
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

trait FilesystemFreezeBackend {
    fn freeze(&mut self, mount: &Path, timeout_millis: Option<u64>) -> Result<(), String>;
    fn thaw(&mut self, mount: &Path) -> Result<(), String>;
}

struct CommandFilesystemFreezeBackend;

impl FilesystemFreezeBackend for CommandFilesystemFreezeBackend {
    fn freeze(&mut self, mount: &Path, _timeout_millis: Option<u64>) -> Result<(), String> {
        run_fsfreeze_command("-f", mount)
    }

    fn thaw(&mut self, mount: &Path) -> Result<(), String> {
        run_fsfreeze_command("-u", mount)
    }
}

fn run_fsfreeze_command(flag: &str, mount: &Path) -> Result<(), String> {
    let output = ProcessCommand::new("fsfreeze")
        .arg(flag)
        .arg(mount)
        .output()
        .map_err(|error| format!("failed to execute fsfreeze: {error}"))?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let detail = if !stderr.is_empty() {
        stderr
    } else if !stdout.is_empty() {
        stdout
    } else {
        format!("exit status {}", output.status)
    };
    Err(detail)
}

fn thaw_mounts_best_effort(
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

fn display_mounts(mounts: &[PathBuf]) -> String {
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

fn freeze_thaw_message(action: &str, timeout_millis: Option<u64>, state: &str) -> String {
    let timeout = timeout_millis.map_or_else(
        || "without a timeout".to_string(),
        |timeout_millis| format!("with timeout {timeout_millis} ms"),
    );
    format!(
        "{state} simulated filesystem {action} scaffold boundary {timeout}; no OS fsfreeze was executed and application consistency is not guaranteed"
    )
}

fn default_applications() -> BTreeMap<String, ApplicationEntry> {
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

fn default_windows() -> BTreeMap<String, WindowEntry> {
    [(
        "window-1",
        WindowEntry {
            title: "BridgeVM Linux Desktop".to_string(),
            focused: true,
            closed: false,
        },
    )]
    .into_iter()
    .map(|(id, entry)| (id.to_string(), entry))
    .collect()
}

fn safe_file_drop_destination(root: &Path, file_name: &str) -> Option<PathBuf> {
    let mut components = Path::new(file_name).components();
    let Some(Component::Normal(name)) = components.next() else {
        return None;
    };
    if components.next().is_some() {
        return None;
    }
    Some(root.join(name))
}

fn decode_base64(input: &str) -> Result<Vec<u8>, String> {
    let bytes = input.as_bytes();
    if bytes.len() % 4 != 0 {
        return Err("base64 payload length must be a multiple of 4".to_string());
    }

    let mut output = Vec::with_capacity(bytes.len() / 4 * 3);
    let mut index = 0usize;
    while index < bytes.len() {
        let chunk = &bytes[index..index + 4];
        let mut values = [0_u8; 4];
        let mut padding = 0usize;
        for (offset, byte) in chunk.iter().enumerate() {
            if *byte == b'=' {
                padding += 1;
                values[offset] = 0;
                continue;
            }
            if padding > 0 {
                return Err("base64 padding must be at the end of the payload".to_string());
            }
            values[offset] = decode_base64_value(*byte)
                .ok_or_else(|| format!("base64 payload contains invalid byte 0x{byte:02x}"))?;
        }
        if padding > 2 {
            return Err("base64 payload has too much padding".to_string());
        }
        if padding > 0 && index + 4 != bytes.len() {
            return Err("base64 padding is only allowed in the final chunk".to_string());
        }

        output.push((values[0] << 2) | (values[1] >> 4));
        if padding < 2 {
            output.push((values[1] << 4) | (values[2] >> 2));
        }
        if padding == 0 {
            output.push((values[2] << 6) | values[3]);
        }
        index += 4;
    }

    Ok(output)
}

fn decode_base64_value(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

fn resolve_token(token: Option<String>, token_file: Option<PathBuf>) -> Result<String> {
    match (token, token_file) {
        (Some(_), Some(_)) => anyhow::bail!("use either --token or --token-file, not both"),
        (Some(token), None) => validate_token(&token),
        (None, Some(path)) => {
            let contents = std::fs::read_to_string(&path)
                .with_context(|| format!("failed to read token file {}", path.display()))?;
            parse_token_file(&contents)
        }
        (None, None) => {
            anyhow::bail!("--token or --token-file is required when a transport is provided")
        }
    }
}

fn parse_token_file(contents: &str) -> Result<String> {
    let trimmed = contents.trim();
    if trimmed.starts_with('{') {
        let value: serde_json::Value =
            serde_json::from_str(trimmed).context("invalid guest tools token JSON")?;
        let token = value
            .get("token")
            .and_then(|token| token.as_str())
            .context("guest tools token JSON is missing string field 'token'")?;
        return validate_token(token);
    }

    validate_token(trimmed)
}

fn validate_token(token: &str) -> Result<String> {
    let token = token.trim();
    if token.is_empty() {
        anyhow::bail!("guest tools token cannot be empty");
    }

    Ok(token.to_string())
}

fn guest_hello(token: &str, guest_os: &str, capabilities: Vec<AgentCapability>) -> AgentEnvelope {
    AgentEnvelope::new(AgentMessage::GuestHello {
        version: bridgevm_agent_protocol::PROTOCOL_VERSION,
        guest_os: guest_os.to_string(),
        agent_version: Some(AGENT_VERSION.to_string()),
        capabilities,
        auth: Some(AgentAuth::ToolsToken {
            token: token.to_string(),
        }),
    })
}

fn resolve_capabilities(values: &[String]) -> Result<Vec<AgentCapability>> {
    if values.is_empty() {
        return Ok(default_capabilities());
    }

    let mut seen = BTreeSet::new();
    values
        .iter()
        .map(|value| parse_capability(value, &mut seen))
        .collect()
}

fn parse_capability(value: &str, seen: &mut BTreeSet<String>) -> Result<AgentCapability> {
    let (name, version) = value
        .split_once(':')
        .map_or((value, "1"), |(name, version)| (name, version));
    let name = name.trim();
    if name.is_empty() {
        anyhow::bail!("capability name cannot be empty");
    }
    if !seen.insert(name.to_string()) {
        anyhow::bail!("duplicate capability '{name}'");
    }
    let version = version
        .trim()
        .parse::<u16>()
        .with_context(|| format!("invalid version for capability '{name}'"))?;
    if version == 0 {
        anyhow::bail!("capability '{name}' version must be greater than zero");
    }

    Ok(AgentCapability {
        name: name.to_string(),
        version,
    })
}

fn default_capabilities() -> Vec<AgentCapability> {
    [
        "heartbeat",
        "time-sync",
        "guest-ip",
        "clipboard",
        "display-resize",
        "shared-folders",
        "drag-drop",
        "applications",
        "windows",
        "fs-freeze",
        "fs-thaw",
        "guest-metrics",
        "agent-update",
    ]
    .into_iter()
    .map(|name| AgentCapability {
        name: name.to_string(),
        version: 1,
    })
    .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TelemetryConfig {
    guest_ips: Vec<GuestIpAddress>,
    metrics: Option<GuestMetricsConfig>,
    clipboard_text: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GuestMetricsConfig {
    cpu_percent: u8,
    memory_used_mib: u64,
}

impl TelemetryConfig {
    #[allow(clippy::too_many_arguments)]
    fn from_args(
        capabilities: &[AgentCapability],
        guest_ips: &[String],
        no_guest_ip: bool,
        metrics_cpu_percent: u8,
        metrics_memory_used_mib: u64,
        no_metrics: bool,
        no_real_metrics: bool,
        clipboard_text: Option<String>,
    ) -> Result<Self> {
        Self::from_args_with_reader(
            capabilities,
            guest_ips,
            no_guest_ip,
            metrics_cpu_percent,
            metrics_memory_used_mib,
            no_metrics,
            no_real_metrics,
            clipboard_text,
            read_proc_metrics,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn from_args_with_reader(
        capabilities: &[AgentCapability],
        guest_ips: &[String],
        no_guest_ip: bool,
        metrics_cpu_percent: u8,
        metrics_memory_used_mib: u64,
        no_metrics: bool,
        no_real_metrics: bool,
        clipboard_text: Option<String>,
        metrics_reader: impl Fn() -> Option<GuestMetricsConfig>,
    ) -> Result<Self> {
        if no_guest_ip && !guest_ips.is_empty() {
            anyhow::bail!("use either --guest-ip or --no-guest-ip, not both");
        }
        if no_metrics && (metrics_cpu_percent != 1 || metrics_memory_used_mib != 256) {
            anyhow::bail!("metrics values cannot be set with --no-metrics");
        }
        if no_metrics && no_real_metrics {
            anyhow::bail!("use either --no-metrics or --no-real-metrics, not both");
        }
        if metrics_cpu_percent > 100 {
            anyhow::bail!("--metrics-cpu-percent must be between 0 and 100");
        }

        let supports_guest_ip = supports_capability(capabilities, "guest-ip");
        let supports_guest_metrics = supports_capability(capabilities, "guest-metrics");
        let supports_clipboard = supports_capability(capabilities, "clipboard");
        let guest_ips = if no_guest_ip || !supports_guest_ip {
            Vec::new()
        } else if guest_ips.is_empty() {
            vec![parse_guest_ip("10.0.2.15@eth0")?]
        } else {
            guest_ips
                .iter()
                .map(|value| parse_guest_ip(value))
                .collect::<Result<Vec<_>>>()?
        };
        let configured_metrics = GuestMetricsConfig {
            cpu_percent: metrics_cpu_percent,
            memory_used_mib: metrics_memory_used_mib,
        };
        let metrics = if no_metrics || !supports_guest_metrics {
            None
        } else if no_real_metrics {
            // Honor the synthetic --metrics-* values verbatim.
            Some(configured_metrics)
        } else {
            // Prefer real /proc-derived metrics; fall back to the configured
            // synthetic values if /proc is unavailable (e.g. non-Linux build).
            Some(metrics_reader().unwrap_or(configured_metrics))
        };
        let clipboard_text = match clipboard_text {
            Some(_) if !supports_clipboard => {
                anyhow::bail!("--clipboard-text requires the clipboard capability")
            }
            Some(text) => Some(normalize_clipboard_text(&text)?),
            None => None,
        };

        Ok(Self {
            guest_ips,
            metrics,
            clipboard_text,
        })
    }
}

/// Read real guest metrics from /proc. Returns None if the files cannot be
/// read or parsed (e.g. when running off-Linux for unit tests), so the caller
/// can fall back to the configured synthetic values.
fn read_proc_metrics() -> Option<GuestMetricsConfig> {
    let meminfo = fs::read_to_string("/proc/meminfo").ok()?;
    let memory_used_mib = parse_memory_used_mib(&meminfo)?;
    // CPU load is approximated from the 1-minute load average over the online
    // CPU count; clamped to 0..=100 to satisfy the protocol invariant.
    let loadavg = fs::read_to_string("/proc/loadavg").ok();
    let cpu_percent = loadavg
        .as_deref()
        .and_then(parse_loadavg_one_minute)
        .map(|load| load_to_cpu_percent(load, online_cpu_count()))
        .unwrap_or(0);
    Some(GuestMetricsConfig {
        cpu_percent,
        memory_used_mib,
    })
}

/// Used = MemTotal - MemAvailable (kB in /proc/meminfo), reported in MiB.
fn parse_memory_used_mib(meminfo: &str) -> Option<u64> {
    let mut total_kib = None;
    let mut available_kib = None;
    for line in meminfo.lines() {
        if let Some(value) = parse_meminfo_kib(line, "MemTotal:") {
            total_kib = Some(value);
        } else if let Some(value) = parse_meminfo_kib(line, "MemAvailable:") {
            available_kib = Some(value);
        }
    }
    let total = total_kib?;
    let available = available_kib?;
    let used_kib = total.saturating_sub(available);
    Some(used_kib / 1024)
}

fn parse_meminfo_kib(line: &str, key: &str) -> Option<u64> {
    let rest = line.strip_prefix(key)?;
    rest.split_whitespace().next()?.parse::<u64>().ok()
}

fn parse_loadavg_one_minute(loadavg: &str) -> Option<f64> {
    loadavg.split_whitespace().next()?.parse::<f64>().ok()
}

fn load_to_cpu_percent(load: f64, cpu_count: u64) -> u8 {
    let cpu_count = cpu_count.max(1) as f64;
    let percent = (load / cpu_count * 100.0).round();
    percent.clamp(0.0, 100.0) as u8
}

fn online_cpu_count() -> u64 {
    std::thread::available_parallelism()
        .map(|count| count.get() as u64)
        .unwrap_or(1)
}

fn normalize_clipboard_text(text: &str) -> Result<String> {
    let text = text.trim_end_matches(['\r', '\n']).to_string();
    if text.is_empty() {
        anyhow::bail!("clipboard text cannot be empty");
    }
    Ok(text)
}

fn supports_capability(capabilities: &[AgentCapability], name: &str) -> bool {
    capabilities
        .iter()
        .any(|capability| capability.name == name)
}

fn parse_guest_ip(value: &str) -> Result<GuestIpAddress> {
    let (address, interface) = value
        .split_once('@')
        .map_or((value, None), |(address, interface)| {
            (address, Some(interface))
        });
    let address = address
        .trim()
        .parse::<IpAddr>()
        .with_context(|| format!("invalid guest IP address '{address}'"))?;
    if address.is_unspecified() {
        anyhow::bail!("guest IP address cannot be unspecified");
    }
    let interface = interface
        .map(str::trim)
        .filter(|interface| !interface.is_empty())
        .map(ToString::to_string);

    Ok(GuestIpAddress { address, interface })
}

fn initial_status_envelopes(telemetry: &TelemetryConfig) -> Vec<AgentEnvelope> {
    let mut envelopes = vec![AgentEnvelope::new(AgentMessage::Heartbeat)];
    if !telemetry.guest_ips.is_empty() {
        envelopes.push(AgentEnvelope::new(AgentMessage::GuestIpChanged {
            addresses: telemetry.guest_ips.clone(),
        }));
    }
    if let Some(metrics) = telemetry.metrics {
        envelopes.push(AgentEnvelope::new(AgentMessage::GuestMetrics {
            cpu_percent: metrics.cpu_percent,
            memory_used_mib: metrics.memory_used_mib,
        }));
    }
    if let Some(text) = &telemetry.clipboard_text {
        envelopes.push(AgentEnvelope::new(AgentMessage::ClipboardChanged {
            text: text.clone(),
        }));
    }
    envelopes
}

#[cfg(test)]
mod tests {
    use super::*;
    use bridgevm_agentd::{decode_envelope_line, encode_envelope_line};
    use std::{
        io::{BufRead, Cursor},
        os::unix::{fs::PermissionsExt, net::UnixListener},
        thread,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn hello_advertises_core_linux_capabilities() {
        let envelope = guest_hello("token-1", "linux", default_capabilities());
        envelope.validate().unwrap();

        let AgentMessage::GuestHello {
            guest_os,
            capabilities,
            auth,
            ..
        } = envelope.message
        else {
            panic!("expected GuestHello");
        };

        assert_eq!(guest_os, "linux");
        assert_eq!(
            auth,
            Some(AgentAuth::ToolsToken {
                token: "token-1".to_string()
            })
        );
        assert!(capabilities
            .iter()
            .any(|capability| capability.name == "clipboard"));
        assert!(capabilities
            .iter()
            .any(|capability| capability.name == "heartbeat"));
        assert!(capabilities
            .iter()
            .any(|capability| capability.name == "display-resize"));
        assert!(capabilities
            .iter()
            .any(|capability| capability.name == "drag-drop"));
        assert!(capabilities
            .iter()
            .any(|capability| capability.name == "applications"));
        assert!(capabilities
            .iter()
            .any(|capability| capability.name == "windows"));
        assert!(capabilities
            .iter()
            .any(|capability| capability.name == "fs-freeze"));
        assert!(capabilities
            .iter()
            .any(|capability| capability.name == "fs-thaw"));
        assert!(capabilities
            .iter()
            .any(|capability| capability.name == "agent-update"));
    }

    #[test]
    fn request_id_commands_get_command_results() {
        let command = AgentEnvelope::with_request_id(
            AgentMessage::SetClipboard {
                text: "hello".to_string(),
            },
            "clipboard-1",
        );

        let mut state = GuestToolsState::new(&default_capabilities());
        let result = state.handle_command(&command).unwrap();
        assert_eq!(
            result.message,
            AgentMessage::CommandResult {
                request_id: "clipboard-1".to_string(),
                ok: true,
                error_code: None,
                message: None,
                result: None,
                metadata: None,
            }
        );
    }

    #[test]
    fn clipboard_command_backend_writes_text_and_reports_success() {
        let root = unique_temp_dir("bridgevm-tools-clipboard");
        fs::create_dir_all(&root).unwrap();
        let command_path = root.join("write-clipboard.sh");
        let output_path = root.join("clipboard.txt");
        fs::write(
            &command_path,
            format!("#!/bin/sh\ncat > '{}'\n", output_path.display()),
        )
        .unwrap();
        let mut permissions = fs::metadata(&command_path).unwrap().permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(&command_path, permissions).unwrap();

        let command = AgentEnvelope::with_request_id(
            AgentMessage::SetClipboard {
                text: "hello from host".to_string(),
            },
            "clipboard-1",
        );
        let mut state = GuestToolsState::new(&default_capabilities())
            .with_clipboard_writer(ClipboardWriter::command(command_path));

        assert_eq!(
            state.handle_command(&command).unwrap().message,
            AgentMessage::CommandResult {
                request_id: "clipboard-1".to_string(),
                ok: true,
                error_code: None,
                message: Some("clipboard updated".to_string()),
                result: None,
                metadata: None,
            }
        );
        assert_eq!(fs::read_to_string(output_path).unwrap(), "hello from host");
    }

    #[test]
    fn clipboard_command_backend_reports_failures() {
        let command = AgentEnvelope::with_request_id(
            AgentMessage::SetClipboard {
                text: "hello".to_string(),
            },
            "clipboard-1",
        );
        let mut state = GuestToolsState::new(&default_capabilities()).with_clipboard_writer(
            ClipboardWriter::command(PathBuf::from("/tmp/bridgevm-missing-clipboard-command")),
        );

        let result = state.handle_command(&command).unwrap();
        let AgentMessage::CommandResult {
            request_id,
            ok,
            error_code,
            message,
            ..
        } = result.message
        else {
            panic!("expected CommandResult");
        };
        assert_eq!(request_id, "clipboard-1");
        assert!(!ok);
        assert_eq!(error_code.as_deref(), Some("clipboard-write-failed"));
        assert!(message
            .as_deref()
            .unwrap()
            .contains("failed to execute clipboard command"));
    }

    #[test]
    fn clipboard_commands_require_capability() {
        let command = AgentEnvelope::with_request_id(
            AgentMessage::SetClipboard {
                text: "hello".to_string(),
            },
            "clipboard-1",
        );
        let mut state = GuestToolsState::new(&[AgentCapability {
            name: "heartbeat".to_string(),
            version: 1,
        }]);

        assert_eq!(
            state.handle_command(&command).unwrap().message,
            AgentMessage::CommandResult {
                request_id: "clipboard-1".to_string(),
                ok: false,
                error_code: Some("capability-not-enabled".to_string()),
                message: Some("clipboard capability is not enabled".to_string()),
                result: None,
                metadata: None,
            }
        );
    }

    #[test]
    fn display_resize_command_backend_passes_dimensions_and_reports_success() {
        let root = unique_temp_dir("bridgevm-tools-display-resize");
        fs::create_dir_all(&root).unwrap();
        let command_path = root.join("resize-display.sh");
        let output_path = root.join("resize.txt");
        fs::write(
            &command_path,
            format!(
                "#!/bin/sh\nprintf '%s %s %s' \"$1\" \"$2\" \"$3\" > '{}'\n",
                output_path.display()
            ),
        )
        .unwrap();
        let mut permissions = fs::metadata(&command_path).unwrap().permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(&command_path, permissions).unwrap();

        let command = AgentEnvelope::with_request_id(
            AgentMessage::ResizeDisplay {
                width: 1440,
                height: 900,
                scale: 2,
            },
            "resize-1",
        );
        let mut state = GuestToolsState::new(&default_capabilities())
            .with_display_resizer(DisplayResizer::command(command_path));

        assert_eq!(
            state.handle_command(&command).unwrap().message,
            AgentMessage::CommandResult {
                request_id: "resize-1".to_string(),
                ok: true,
                error_code: None,
                message: Some("display resized to 1440x900 scale 2".to_string()),
                result: None,
                metadata: None,
            }
        );
        assert_eq!(fs::read_to_string(output_path).unwrap(), "1440 900 2");
    }

    #[test]
    fn display_resize_command_backend_reports_failures() {
        let command = AgentEnvelope::with_request_id(
            AgentMessage::ResizeDisplay {
                width: 1440,
                height: 900,
                scale: 2,
            },
            "resize-1",
        );
        let mut state = GuestToolsState::new(&default_capabilities()).with_display_resizer(
            DisplayResizer::command(PathBuf::from("/tmp/bridgevm-missing-display-command")),
        );

        let result = state.handle_command(&command).unwrap();
        let AgentMessage::CommandResult {
            request_id,
            ok,
            error_code,
            message,
            ..
        } = result.message
        else {
            panic!("expected CommandResult");
        };
        assert_eq!(request_id, "resize-1");
        assert!(!ok);
        assert_eq!(error_code.as_deref(), Some("display-resize-failed"));
        assert!(message
            .as_deref()
            .unwrap()
            .contains("failed to execute display resize command"));
    }

    #[test]
    fn display_resize_commands_require_capability() {
        let command = AgentEnvelope::with_request_id(
            AgentMessage::ResizeDisplay {
                width: 1440,
                height: 900,
                scale: 2,
            },
            "resize-1",
        );
        let mut state = GuestToolsState::new(&[AgentCapability {
            name: "heartbeat".to_string(),
            version: 1,
        }]);

        assert_eq!(
            state.handle_command(&command).unwrap().message,
            AgentMessage::CommandResult {
                request_id: "resize-1".to_string(),
                ok: false,
                error_code: Some("capability-not-enabled".to_string()),
                message: Some("display resize capability is not enabled".to_string()),
                result: None,
                metadata: None,
            }
        );
    }

    /// Fake desktop environment for auto-detection tests: records which env vars
    /// are "set" and which programs are "on PATH" without touching the real
    /// process environment or running xclip/xrandr.
    struct FakeDesktopEnv {
        envs: &'static [&'static str],
        programs: &'static [&'static str],
    }

    impl DesktopEnv for FakeDesktopEnv {
        fn has_env(&self, name: &str) -> bool {
            self.envs.contains(&name)
        }

        fn has_program(&self, program: &str) -> bool {
            self.programs.contains(&program)
        }
    }

    #[test]
    fn clipboard_detection_prefers_wayland_wl_copy() {
        // Wayland session with both tools available -> wl-copy, no args, even
        // though xclip is also present.
        let env = FakeDesktopEnv {
            envs: &["WAYLAND_DISPLAY", "DISPLAY"],
            programs: &["wl-copy", "xclip"],
        };
        let writer = detect_clipboard_writer(&env);
        let (program, args) = writer.command_for_test().expect("expected a command");
        assert_eq!(program, Path::new("wl-copy"));
        assert!(args.is_empty());
    }

    #[test]
    fn clipboard_detection_falls_back_to_x11_xclip() {
        // X11 session (no WAYLAND_DISPLAY) -> xclip with selection args.
        let env = FakeDesktopEnv {
            envs: &["DISPLAY"],
            programs: &["xclip"],
        };
        let writer = detect_clipboard_writer(&env);
        let (program, args) = writer.command_for_test().expect("expected a command");
        assert_eq!(program, Path::new("xclip"));
        assert_eq!(args, &["-selection".to_string(), "clipboard".to_string()]);
    }

    #[test]
    fn clipboard_detection_falls_back_to_simulated_without_tools() {
        // Wayland var set but wl-copy missing, and no DISPLAY -> simulated.
        let env = FakeDesktopEnv {
            envs: &["WAYLAND_DISPLAY"],
            programs: &[],
        };
        assert!(detect_clipboard_writer(&env).command_for_test().is_none());

        // No session at all -> simulated.
        let env = FakeDesktopEnv {
            envs: &[],
            programs: &["wl-copy", "xclip"],
        };
        assert!(detect_clipboard_writer(&env).command_for_test().is_none());
    }

    #[test]
    fn display_resize_detection_uses_xrandr_on_x11() {
        let env = FakeDesktopEnv {
            envs: &["DISPLAY"],
            programs: &["xrandr"],
        };
        let resizer = detect_display_resizer(&env);
        assert_eq!(
            resizer.command_for_test().expect("expected a command"),
            Path::new("xrandr")
        );

        // Missing DISPLAY or missing xrandr -> simulated.
        let no_display = FakeDesktopEnv {
            envs: &[],
            programs: &["xrandr"],
        };
        assert!(detect_display_resizer(&no_display)
            .command_for_test()
            .is_none());
        let no_tool = FakeDesktopEnv {
            envs: &["DISPLAY"],
            programs: &[],
        };
        assert!(detect_display_resizer(&no_tool).command_for_test().is_none());
    }

    #[test]
    fn explicit_clipboard_command_is_unchanged_by_detection() {
        // An explicit --clipboard-command path runs that exact program with no
        // extra args, regardless of capability auto-detection.
        let writer = resolve_clipboard_writer(
            &default_capabilities(),
            Some(PathBuf::from("/usr/local/bin/my-clipboard")),
        )
        .unwrap();
        let (program, args) = writer.command_for_test().expect("expected a command");
        assert_eq!(program, Path::new("/usr/local/bin/my-clipboard"));
        assert!(args.is_empty());

        // Likewise for the explicit display-resize command.
        let resizer = resolve_display_resizer(
            &default_capabilities(),
            Some(PathBuf::from("/usr/local/bin/my-resize")),
        )
        .unwrap();
        assert_eq!(
            resizer.command_for_test().expect("expected a command"),
            Path::new("/usr/local/bin/my-resize")
        );
    }

    #[test]
    fn detection_only_runs_when_capability_enabled() {
        // No explicit command and no clipboard/display-resize capability ->
        // simulated, detection is never consulted.
        let heartbeat_only = [AgentCapability {
            name: "heartbeat".to_string(),
            version: 1,
        }];
        assert!(resolve_clipboard_writer(&heartbeat_only, None)
            .unwrap()
            .command_for_test()
            .is_none());
        assert!(resolve_display_resizer(&heartbeat_only, None)
            .unwrap()
            .command_for_test()
            .is_none());
    }

    #[test]
    fn fire_and_forget_commands_do_not_emit_results() {
        let command = AgentEnvelope::new(AgentMessage::TimeSync {
            unix_epoch_millis: 1,
        });

        let mut state = GuestToolsState::new(&default_capabilities());
        assert_eq!(state.handle_command(&command), None);
    }

    #[test]
    fn shared_folder_commands_update_guest_tools_state() {
        let mut state = GuestToolsState::new(&default_capabilities());
        let mount = AgentEnvelope::with_request_id(
            AgentMessage::MountShare {
                name: "workspace".to_string(),
                host_path_token: "host-token-1".to_string(),
            },
            "mount-1",
        );
        let update = AgentEnvelope::with_request_id(
            AgentMessage::MountShare {
                name: "workspace".to_string(),
                host_path_token: "host-token-2".to_string(),
            },
            "mount-2",
        );
        let unmount = AgentEnvelope::with_request_id(
            AgentMessage::UnmountShare {
                name: "workspace".to_string(),
            },
            "unmount-1",
        );

        assert_eq!(
            state.handle_command(&mount).unwrap().message,
            AgentMessage::CommandResult {
                request_id: "mount-1".to_string(),
                ok: true,
                error_code: None,
                message: Some("accepted mount request for share workspace".to_string()),
                result: None,
                metadata: None,
            }
        );
        assert_eq!(
            state
                .shared_folders
                .get("workspace")
                .map(|mount| mount.host_path_token.as_str()),
            Some("host-token-1")
        );
        assert_eq!(
            state.handle_command(&update).unwrap().message,
            AgentMessage::CommandResult {
                request_id: "mount-2".to_string(),
                ok: true,
                error_code: None,
                message: Some("accepted share update for workspace".to_string()),
                result: None,
                metadata: None,
            }
        );
        assert_eq!(
            state
                .shared_folders
                .get("workspace")
                .map(|mount| mount.host_path_token.as_str()),
            Some("host-token-2")
        );
        assert_eq!(
            state.handle_command(&unmount).unwrap().message,
            AgentMessage::CommandResult {
                request_id: "unmount-1".to_string(),
                ok: true,
                error_code: None,
                message: Some("accepted unmount request for share workspace".to_string()),
                result: None,
                metadata: None,
            }
        );
        assert!(!state.shared_folders.contains_key("workspace"));
    }

    #[test]
    fn unmounting_missing_share_returns_error() {
        let mut state = GuestToolsState::new(&default_capabilities());
        let command = AgentEnvelope::with_request_id(
            AgentMessage::UnmountShare {
                name: "missing".to_string(),
            },
            "unmount-1",
        );

        assert_eq!(
            state.handle_command(&command).unwrap().message,
            AgentMessage::CommandResult {
                request_id: "unmount-1".to_string(),
                ok: false,
                error_code: Some("share-not-mounted".to_string()),
                message: Some("share missing is not mounted".to_string()),
                result: None,
                metadata: None,
            }
        );
    }

    #[test]
    fn shared_folder_commands_require_capability() {
        let mut state = GuestToolsState::new(&[AgentCapability {
            name: "heartbeat".to_string(),
            version: 1,
        }]);
        let command = AgentEnvelope::with_request_id(
            AgentMessage::MountShare {
                name: "workspace".to_string(),
                host_path_token: "host-token-1".to_string(),
            },
            "mount-1",
        );

        assert_eq!(
            state.handle_command(&command).unwrap().message,
            AgentMessage::CommandResult {
                request_id: "mount-1".to_string(),
                ok: false,
                error_code: Some("capability-not-enabled".to_string()),
                message: Some("shared folders capability is not enabled".to_string()),
                result: None,
                metadata: None,
            }
        );
        assert!(state.shared_folders.is_empty());
    }

    #[test]
    fn file_drop_commands_track_alpha_transfer_state() {
        let mut state = GuestToolsState::new(&default_capabilities());
        let start = AgentEnvelope::with_request_id(
            AgentMessage::FileDropStart {
                transfer_id: "drop-1".to_string(),
                file_name: "notes.txt".to_string(),
                size_bytes: 11,
            },
            "drop-start-1",
        );
        let chunk = AgentEnvelope::with_request_id(
            AgentMessage::FileDropChunk {
                transfer_id: "drop-1".to_string(),
                chunk_index: 0,
                data_base64: "aGVsbG8gd29ybGQ=".to_string(),
            },
            "drop-chunk-1",
        );
        let complete = AgentEnvelope::with_request_id(
            AgentMessage::FileDropComplete {
                transfer_id: "drop-1".to_string(),
            },
            "drop-complete-1",
        );

        assert_eq!(
            state.handle_command(&start).unwrap().message,
            AgentMessage::CommandResult {
                request_id: "drop-start-1".to_string(),
                ok: true,
                error_code: None,
                message: Some("started file drop drop-1".to_string()),
                result: None,
                metadata: None,
            }
        );
        assert_eq!(
            state.handle_command(&chunk).unwrap().message,
            AgentMessage::CommandResult {
                request_id: "drop-chunk-1".to_string(),
                ok: true,
                error_code: None,
                message: Some("accepted file drop drop-1 chunk 0".to_string()),
                result: None,
                metadata: None,
            }
        );
        assert_eq!(
            state.handle_command(&complete).unwrap().message,
            AgentMessage::CommandResult {
                request_id: "drop-complete-1".to_string(),
                ok: true,
                error_code: None,
                message: Some(
                    "completed file drop notes.txt (11 bytes across 1 chunks)".to_string()
                ),
                result: None,
                metadata: None,
            }
        );
        assert!(state.file_drops.is_empty());
    }

    #[test]
    fn file_drop_commands_can_write_payload_to_configured_directory() {
        let root = unique_temp_dir("bridgevm-tools-file-drop");
        let mut state =
            GuestToolsState::new(&default_capabilities()).with_file_drop_dir(Some(root.clone()));
        let start = AgentEnvelope::with_request_id(
            AgentMessage::FileDropStart {
                transfer_id: "drop-1".to_string(),
                file_name: "notes.txt".to_string(),
                size_bytes: 11,
            },
            "drop-start-1",
        );
        let chunk = AgentEnvelope::with_request_id(
            AgentMessage::FileDropChunk {
                transfer_id: "drop-1".to_string(),
                chunk_index: 0,
                data_base64: "aGVsbG8gd29ybGQ=".to_string(),
            },
            "drop-chunk-1",
        );
        let complete = AgentEnvelope::with_request_id(
            AgentMessage::FileDropComplete {
                transfer_id: "drop-1".to_string(),
            },
            "drop-complete-1",
        );

        assert!(matches!(
            state.handle_command(&start).unwrap().message,
            AgentMessage::CommandResult { ok: true, .. }
        ));
        assert!(matches!(
            state.handle_command(&chunk).unwrap().message,
            AgentMessage::CommandResult { ok: true, .. }
        ));
        let result = state.handle_command(&complete).unwrap();

        assert_eq!(
            result.message,
            AgentMessage::CommandResult {
                request_id: "drop-complete-1".to_string(),
                ok: true,
                error_code: None,
                message: Some(format!(
                    "completed file drop notes.txt (11 bytes across 1 chunks) at {}",
                    root.join("notes.txt").display()
                )),
                result: None,
                metadata: None,
            }
        );
        assert_eq!(
            std::fs::read_to_string(root.join("notes.txt")).unwrap(),
            "hello world"
        );
        assert!(state.file_drops.is_empty());
    }

    #[test]
    fn file_drop_write_rejects_unsafe_names_and_size_mismatches() {
        let root = unique_temp_dir("bridgevm-tools-file-drop-errors");
        let mut state =
            GuestToolsState::new(&default_capabilities()).with_file_drop_dir(Some(root.clone()));
        let start = AgentEnvelope::with_request_id(
            AgentMessage::FileDropStart {
                transfer_id: "drop-1".to_string(),
                file_name: "../notes.txt".to_string(),
                size_bytes: 11,
            },
            "drop-start-1",
        );
        let chunk = AgentEnvelope::with_request_id(
            AgentMessage::FileDropChunk {
                transfer_id: "drop-1".to_string(),
                chunk_index: 0,
                data_base64: "aGVsbG8gd29ybGQ=".to_string(),
            },
            "drop-chunk-1",
        );
        let complete = AgentEnvelope::with_request_id(
            AgentMessage::FileDropComplete {
                transfer_id: "drop-1".to_string(),
            },
            "drop-complete-1",
        );

        state.handle_command(&start).unwrap();
        state.handle_command(&chunk).unwrap();
        assert_eq!(
            state.handle_command(&complete).unwrap().message,
            AgentMessage::CommandResult {
                request_id: "drop-complete-1".to_string(),
                ok: false,
                error_code: Some("unsafe-file-name".to_string()),
                message: Some("file drop file name is not safe: ../notes.txt".to_string()),
                result: None,
                metadata: None,
            }
        );
        assert!(!root.join("notes.txt").exists());

        let mut state =
            GuestToolsState::new(&default_capabilities()).with_file_drop_dir(Some(root));
        let start = AgentEnvelope::with_request_id(
            AgentMessage::FileDropStart {
                transfer_id: "drop-2".to_string(),
                file_name: "short.txt".to_string(),
                size_bytes: 12,
            },
            "drop-start-2",
        );
        let chunk = AgentEnvelope::with_request_id(
            AgentMessage::FileDropChunk {
                transfer_id: "drop-2".to_string(),
                chunk_index: 0,
                data_base64: "aGVsbG8gd29ybGQ=".to_string(),
            },
            "drop-chunk-2",
        );
        let complete = AgentEnvelope::with_request_id(
            AgentMessage::FileDropComplete {
                transfer_id: "drop-2".to_string(),
            },
            "drop-complete-2",
        );

        state.handle_command(&start).unwrap();
        state.handle_command(&chunk).unwrap();
        assert_eq!(
            state.handle_command(&complete).unwrap().message,
            AgentMessage::CommandResult {
                request_id: "drop-complete-2".to_string(),
                ok: false,
                error_code: Some("transfer-size-mismatch".to_string()),
                message: Some("file drop short.txt expected 12 bytes but received 11".to_string()),
                result: None,
                metadata: None,
            }
        );
    }

    #[test]
    fn file_drop_commands_require_capability_and_start_order() {
        let mut state = GuestToolsState::new(&[AgentCapability {
            name: "heartbeat".to_string(),
            version: 1,
        }]);
        let start = AgentEnvelope::with_request_id(
            AgentMessage::FileDropStart {
                transfer_id: "drop-1".to_string(),
                file_name: "notes.txt".to_string(),
                size_bytes: 11,
            },
            "drop-start-1",
        );

        assert_eq!(
            state.handle_command(&start).unwrap().message,
            AgentMessage::CommandResult {
                request_id: "drop-start-1".to_string(),
                ok: false,
                error_code: Some("capability-not-enabled".to_string()),
                message: Some("drag-and-drop capability is not enabled".to_string()),
                result: None,
                metadata: None,
            }
        );

        let mut state = GuestToolsState::new(&default_capabilities());
        let complete = AgentEnvelope::with_request_id(
            AgentMessage::FileDropComplete {
                transfer_id: "missing".to_string(),
            },
            "drop-complete-1",
        );

        assert_eq!(
            state.handle_command(&complete).unwrap().message,
            AgentMessage::CommandResult {
                request_id: "drop-complete-1".to_string(),
                ok: false,
                error_code: Some("transfer-not-started".to_string()),
                message: Some("file drop missing has not started".to_string()),
                result: None,
                metadata: None,
            }
        );
    }

    #[test]
    fn application_commands_track_alpha_state() {
        let mut state = GuestToolsState::new(&default_capabilities());

        let list = AgentEnvelope::with_request_id(AgentMessage::ListApplications, "apps-1");
        assert_eq!(
            state.handle_command(&list).unwrap().message,
            AgentMessage::CommandResult {
                request_id: "apps-1".to_string(),
                ok: true,
                error_code: None,
                message: Some(
                    "applications: org.bridgevm.files:Files,org.bridgevm.terminal:Terminal"
                        .to_string()
                ),
                result: Some(serde_json::json!({
                    "applications": [
                        {
                            "id": "org.bridgevm.files",
                            "name": "Files",
                            "launched": false
                        },
                        {
                            "id": "org.bridgevm.terminal",
                            "name": "Terminal",
                            "launched": false
                        }
                    ]
                })),
                metadata: None,
            }
        );

        let launch = AgentEnvelope::with_request_id(
            AgentMessage::LaunchApplication {
                id: "org.bridgevm.terminal".to_string(),
            },
            "launch-1",
        );
        assert_eq!(
            state.handle_command(&launch).unwrap().message,
            AgentMessage::CommandResult {
                request_id: "launch-1".to_string(),
                ok: true,
                error_code: None,
                message: Some("accepted launch request for application Terminal".to_string()),
                result: Some(serde_json::json!({
                    "application": {
                        "id": "org.bridgevm.terminal",
                        "name": "Terminal",
                        "launched": true
                    }
                })),
                metadata: None,
            }
        );
        assert!(state
            .applications
            .get("org.bridgevm.terminal")
            .is_some_and(|app| app.launched));
    }

    #[test]
    fn window_commands_track_alpha_state() {
        let mut state = GuestToolsState::new(&default_capabilities());

        let list = AgentEnvelope::with_request_id(AgentMessage::ListWindows, "windows-1");
        assert_eq!(
            state.handle_command(&list).unwrap().message,
            AgentMessage::CommandResult {
                request_id: "windows-1".to_string(),
                ok: true,
                error_code: None,
                message: Some("windows: window-1:BridgeVM Linux Desktop".to_string()),
                result: Some(serde_json::json!({
                    "windows": [
                        {
                            "id": "window-1",
                            "title": "BridgeVM Linux Desktop",
                            "focused": true
                        }
                    ]
                })),
                metadata: None,
            }
        );

        let focus = AgentEnvelope::with_request_id(
            AgentMessage::FocusWindow {
                id: "window-1".to_string(),
            },
            "focus-1",
        );
        assert_eq!(
            state.handle_command(&focus).unwrap().message,
            AgentMessage::CommandResult {
                request_id: "focus-1".to_string(),
                ok: true,
                error_code: None,
                message: Some(
                    "accepted focus request for window BridgeVM Linux Desktop".to_string()
                ),
                result: Some(serde_json::json!({
                    "window": {
                        "id": "window-1",
                        "title": "BridgeVM Linux Desktop",
                        "focused": true
                    }
                })),
                metadata: None,
            }
        );

        let close = AgentEnvelope::with_request_id(
            AgentMessage::CloseWindow {
                id: "window-1".to_string(),
            },
            "close-1",
        );
        assert_eq!(
            state.handle_command(&close).unwrap().message,
            AgentMessage::CommandResult {
                request_id: "close-1".to_string(),
                ok: true,
                error_code: None,
                message: Some("closed window BridgeVM Linux Desktop".to_string()),
                result: Some(serde_json::json!({
                    "window": {
                        "id": "window-1",
                        "title": "BridgeVM Linux Desktop",
                        "closed": true
                    }
                })),
                metadata: None,
            }
        );
        assert!(state
            .windows
            .get("window-1")
            .is_some_and(|window| window.closed && !window.focused));
    }

    #[test]
    fn application_and_window_commands_require_capabilities() {
        let mut state = GuestToolsState::new(&[AgentCapability {
            name: "heartbeat".to_string(),
            version: 1,
        }]);

        let launch = AgentEnvelope::with_request_id(
            AgentMessage::LaunchApplication {
                id: "org.bridgevm.terminal".to_string(),
            },
            "launch-1",
        );
        assert_eq!(
            state.handle_command(&launch).unwrap().message,
            AgentMessage::CommandResult {
                request_id: "launch-1".to_string(),
                ok: false,
                error_code: Some("capability-not-enabled".to_string()),
                message: Some("applications capability is not enabled".to_string()),
                result: None,
                metadata: None,
            }
        );

        let focus = AgentEnvelope::with_request_id(
            AgentMessage::FocusWindow {
                id: "window-1".to_string(),
            },
            "focus-1",
        );
        assert_eq!(
            state.handle_command(&focus).unwrap().message,
            AgentMessage::CommandResult {
                request_id: "focus-1".to_string(),
                ok: false,
                error_code: Some("capability-not-enabled".to_string()),
                message: Some("windows capability is not enabled".to_string()),
                result: None,
                metadata: None,
            }
        );
    }

    #[test]
    fn filesystem_freeze_thaw_tracks_scaffold_boundary() {
        let mut state = GuestToolsState::new(&default_capabilities());
        let freeze = AgentEnvelope::with_request_id(
            AgentMessage::FreezeFilesystem {
                timeout_millis: Some(30_000),
            },
            "freeze-1",
        );
        let duplicate_freeze = AgentEnvelope::with_request_id(
            AgentMessage::FreezeFilesystem {
                timeout_millis: None,
            },
            "freeze-2",
        );
        let thaw = AgentEnvelope::with_request_id(AgentMessage::ThawFilesystem, "thaw-1");
        let duplicate_thaw = AgentEnvelope::with_request_id(AgentMessage::ThawFilesystem, "thaw-2");

        assert_eq!(
            state.handle_command(&freeze).unwrap().message,
            AgentMessage::CommandResult {
                request_id: "freeze-1".to_string(),
                ok: true,
                error_code: None,
                message: Some(
                    "entered simulated filesystem freeze scaffold boundary with timeout 30000 ms; no OS fsfreeze was executed and application consistency is not guaranteed"
                        .to_string()
                ),
                result: None,
                metadata: None,
            }
        );
        assert!(state.filesystem_frozen);
        assert_eq!(
            state.handle_command(&duplicate_freeze).unwrap().message,
            AgentMessage::CommandResult {
                request_id: "freeze-2".to_string(),
                ok: false,
                error_code: Some("filesystem-already-frozen".to_string()),
                message: Some("filesystem freeze scaffold boundary is already active".to_string()),
                result: None,
                metadata: None,
            }
        );
        assert_eq!(
            state.handle_command(&thaw).unwrap().message,
            AgentMessage::CommandResult {
                request_id: "thaw-1".to_string(),
                ok: true,
                error_code: None,
                message: Some(
                    "left simulated filesystem thaw scaffold boundary; no OS fsfreeze was executed and application consistency is not guaranteed"
                        .to_string()
                ),
                result: None,
                metadata: None,
            }
        );
        assert!(!state.filesystem_frozen);
        assert_eq!(
            state.handle_command(&duplicate_thaw).unwrap().message,
            AgentMessage::CommandResult {
                request_id: "thaw-2".to_string(),
                ok: false,
                error_code: Some("filesystem-not-frozen".to_string()),
                message: Some("filesystem thaw scaffold boundary is not active".to_string()),
                result: None,
                metadata: None,
            }
        );
    }

    #[test]
    fn filesystem_freeze_thaw_require_capabilities() {
        let mut state = GuestToolsState::new(&[AgentCapability {
            name: "heartbeat".to_string(),
            version: 1,
        }]);
        let freeze = AgentEnvelope::with_request_id(
            AgentMessage::FreezeFilesystem {
                timeout_millis: None,
            },
            "freeze-1",
        );
        assert_eq!(
            state.handle_command(&freeze).unwrap().message,
            AgentMessage::CommandResult {
                request_id: "freeze-1".to_string(),
                ok: false,
                error_code: Some("capability-not-enabled".to_string()),
                message: Some("filesystem freeze capability is not enabled".to_string()),
                result: None,
                metadata: None,
            }
        );

        let mut state = GuestToolsState::new(&[AgentCapability {
            name: "fs-freeze".to_string(),
            version: 1,
        }]);
        let thaw = AgentEnvelope::with_request_id(AgentMessage::ThawFilesystem, "thaw-1");
        assert_eq!(
            state.handle_command(&thaw).unwrap().message,
            AgentMessage::CommandResult {
                request_id: "thaw-1".to_string(),
                ok: false,
                error_code: Some("capability-not-enabled".to_string()),
                message: Some("filesystem thaw capability is not enabled".to_string()),
                result: None,
                metadata: None,
            }
        );
    }

    #[test]
    fn real_filesystem_freezer_uses_allowlisted_mounts_and_reverse_thaw() {
        let calls = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let backend = RecordingFreezeBackend {
            calls: calls.clone(),
            fail_freeze: None,
        };
        let mut state = GuestToolsState::new(&default_capabilities()).with_filesystem_freezer(
            FilesystemFreezer::real(
                vec![PathBuf::from("/"), PathBuf::from("/var")],
                Box::new(backend),
            ),
        );

        let freeze = AgentEnvelope::with_request_id(
            AgentMessage::FreezeFilesystem {
                timeout_millis: Some(10_000),
            },
            "freeze-1",
        );
        let thaw = AgentEnvelope::with_request_id(AgentMessage::ThawFilesystem, "thaw-1");

        let result = state.handle_command(&freeze).unwrap();
        assert_eq!(
            result.message,
            AgentMessage::CommandResult {
                request_id: "freeze-1".to_string(),
                ok: true,
                error_code: None,
                message: Some(
                    "entered real fsfreeze boundary for /, /var; application consistency still depends on guest applications flushing state"
                        .to_string()
                ),
                result: None,
                metadata: None,
            }
        );
        assert_eq!(calls.borrow().as_slice(), ["freeze:/", "freeze:/var"]);

        let result = state.handle_command(&thaw).unwrap();
        assert_eq!(
            result.message,
            AgentMessage::CommandResult {
                request_id: "thaw-1".to_string(),
                ok: true,
                error_code: None,
                message: Some(
                    "left real fsfreeze boundary for /, /var; application consistency still depends on guest applications flushing state"
                        .to_string()
                ),
                result: None,
                metadata: None,
            }
        );
        assert_eq!(
            calls.borrow().as_slice(),
            ["freeze:/", "freeze:/var", "thaw:/var", "thaw:/"]
        );
    }

    #[test]
    fn real_filesystem_freezer_rolls_back_on_partial_freeze_failure() {
        let calls = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let backend = RecordingFreezeBackend {
            calls: calls.clone(),
            fail_freeze: Some(PathBuf::from("/var")),
        };
        let mut state = GuestToolsState::new(&default_capabilities()).with_filesystem_freezer(
            FilesystemFreezer::real(
                vec![PathBuf::from("/"), PathBuf::from("/var")],
                Box::new(backend),
            ),
        );
        let freeze = AgentEnvelope::with_request_id(
            AgentMessage::FreezeFilesystem {
                timeout_millis: Some(10_000),
            },
            "freeze-1",
        );

        let result = state.handle_command(&freeze).unwrap();
        assert_eq!(
            result.message,
            AgentMessage::CommandResult {
                request_id: "freeze-1".to_string(),
                ok: false,
                error_code: Some("filesystem-freeze-failed".to_string()),
                message: Some(
                    "failed to freeze /var: injected freeze failure; rollback thaw succeeded"
                        .to_string()
                ),
                result: None,
                metadata: None,
            }
        );
        assert!(!state.filesystem_frozen);
        assert_eq!(
            calls.borrow().as_slice(),
            ["freeze:/", "freeze:/var", "thaw:/"]
        );
    }

    #[test]
    fn real_filesystem_freezer_args_require_explicit_absolute_mounts() {
        assert!(resolve_filesystem_freezer(false, vec![PathBuf::from("/")]).is_err());
        assert!(resolve_filesystem_freezer(true, Vec::new()).is_err());
        assert!(resolve_filesystem_freezer(true, vec![PathBuf::from("relative")]).is_err());
        assert!(resolve_filesystem_freezer(
            true,
            vec![PathBuf::from("/var"), PathBuf::from("/var/."),]
        )
        .is_err());
        assert_eq!(
            normalize_fsfreeze_mounts(vec![PathBuf::from("/var/../var/lib")]).unwrap(),
            vec![PathBuf::from("/var/lib")]
        );
    }

    #[test]
    fn real_fsfreeze_cli_parses_opt_in_mount_flags() {
        let args = Args::try_parse_from([
            "bridgevm-tools-linux",
            "--socket",
            "tools.sock",
            "--token",
            "token-1",
            "--real-fsfreeze",
            "--fsfreeze-mount",
            "/",
            "--fsfreeze-mount",
            "/var",
        ])
        .unwrap();

        assert!(args.real_fsfreeze);
        assert_eq!(
            args.fsfreeze_mounts,
            vec![PathBuf::from("/"), PathBuf::from("/var")]
        );
    }

    #[test]
    fn fire_and_forget_share_commands_update_state_without_result() {
        let mut state = GuestToolsState::new(&default_capabilities());
        let mount = AgentEnvelope::new(AgentMessage::MountShare {
            name: "workspace".to_string(),
            host_path_token: "host-token-1".to_string(),
        });

        assert_eq!(state.handle_command(&mount), None);
        assert!(state.shared_folders.contains_key("workspace"));
    }

    #[test]
    fn time_sync_with_real_backend_applies_epoch_and_replies_with_result() {
        let applied = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let backend = RecordingClockBackend {
            applied: applied.clone(),
            fail: false,
        };
        let mut state = GuestToolsState::new(&default_capabilities())
            .with_clock_setter(ClockSetter::real(Box::new(backend)));

        let command = AgentEnvelope::with_request_id(
            AgentMessage::TimeSync {
                unix_epoch_millis: 1_781_470_123_456,
            },
            "time-1",
        );

        assert_eq!(
            state.handle_command(&command).unwrap().message,
            AgentMessage::CommandResult {
                request_id: "time-1".to_string(),
                ok: true,
                error_code: None,
                message: Some("set guest clock to 1781470123456 ms since epoch".to_string()),
                result: Some(serde_json::json!({
                    "applied_unix_epoch_millis": 1_781_470_123_456u64,
                })),
                metadata: None,
            }
        );
        assert_eq!(applied.borrow().as_slice(), [1_781_470_123_456]);
    }

    #[test]
    fn time_sync_simulated_backend_acknowledges_without_setting_clock() {
        let mut state = GuestToolsState::new(&default_capabilities())
            .with_clock_setter(ClockSetter::simulated());
        let command = AgentEnvelope::with_request_id(
            AgentMessage::TimeSync {
                unix_epoch_millis: 1_781_470_000_000,
            },
            "time-1",
        );

        let AgentMessage::CommandResult {
            ok,
            error_code,
            message,
            result,
            ..
        } = state.handle_command(&command).unwrap().message
        else {
            panic!("expected CommandResult");
        };
        assert!(ok);
        assert_eq!(error_code, None);
        assert!(message.unwrap().contains("guest clock was not changed"));
        assert_eq!(
            result,
            Some(serde_json::json!({
                "applied_unix_epoch_millis": 1_781_470_000_000u64,
            }))
        );
    }

    #[test]
    fn time_sync_real_backend_failure_reports_error() {
        let applied = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let backend = RecordingClockBackend {
            applied,
            fail: true,
        };
        let mut state = GuestToolsState::new(&default_capabilities())
            .with_clock_setter(ClockSetter::real(Box::new(backend)));
        let command = AgentEnvelope::with_request_id(
            AgentMessage::TimeSync {
                unix_epoch_millis: 1_781_470_000_000,
            },
            "time-1",
        );

        let AgentMessage::CommandResult {
            ok, error_code, ..
        } = state.handle_command(&command).unwrap().message
        else {
            panic!("expected CommandResult");
        };
        assert!(!ok);
        assert_eq!(error_code.as_deref(), Some("time-sync-failed"));
    }

    #[test]
    fn time_sync_requires_capability() {
        let mut state = GuestToolsState::new(&[AgentCapability {
            name: "heartbeat".to_string(),
            version: 1,
        }])
        .with_clock_setter(ClockSetter::real(Box::new(RecordingClockBackend {
            applied: std::rc::Rc::new(std::cell::RefCell::new(Vec::new())),
            fail: false,
        })));
        let command = AgentEnvelope::with_request_id(
            AgentMessage::TimeSync {
                unix_epoch_millis: 1_781_470_000_000,
            },
            "time-1",
        );

        assert_eq!(
            state.handle_command(&command).unwrap().message,
            AgentMessage::CommandResult {
                request_id: "time-1".to_string(),
                ok: false,
                error_code: Some("capability-not-enabled".to_string()),
                message: Some("time-sync capability is not enabled".to_string()),
                result: None,
                metadata: None,
            }
        );
    }

    #[test]
    fn resolve_clock_setter_defaults_to_real_when_capable() {
        // Real when time-sync is advertised and not opted out.
        assert!(matches!(
            resolve_clock_setter(&default_capabilities(), false).mode,
            ClockSetterMode::Real { .. }
        ));
        // Simulated when opted out, or when time-sync is not negotiated.
        assert!(matches!(
            resolve_clock_setter(&default_capabilities(), true).mode,
            ClockSetterMode::Simulated
        ));
        assert!(matches!(
            resolve_clock_setter(
                &[AgentCapability {
                    name: "heartbeat".to_string(),
                    version: 1,
                }],
                false,
            )
            .mode,
            ClockSetterMode::Simulated
        ));
    }

    #[test]
    fn proc_meminfo_parses_used_memory_in_mib() {
        let meminfo = "MemTotal:        4194304 kB\nMemFree:          524288 kB\nMemAvailable:    2097152 kB\nBuffers:           10240 kB\n";
        // used_kib = 4194304 - 2097152 = 2097152 kB = 2048 MiB
        assert_eq!(parse_memory_used_mib(meminfo), Some(2048));

        // Missing MemAvailable -> None (cannot compute used reliably).
        assert_eq!(parse_memory_used_mib("MemTotal: 4194304 kB\n"), None);
    }

    #[test]
    fn loadavg_parses_and_maps_to_cpu_percent() {
        assert_eq!(parse_loadavg_one_minute("0.50 0.25 0.10 1/234 5678"), Some(0.50));
        assert_eq!(parse_loadavg_one_minute("garbage"), None);
        // 1.0 load over 4 CPUs -> 25%.
        assert_eq!(load_to_cpu_percent(1.0, 4), 25);
        // Saturation: 8.0 load over 4 CPUs clamps to 100%.
        assert_eq!(load_to_cpu_percent(8.0, 4), 100);
        // Zero CPUs treated as one.
        assert_eq!(load_to_cpu_percent(0.5, 0), 50);
    }

    #[test]
    fn real_metrics_reader_feeds_telemetry_and_falls_back_when_unavailable() {
        // A reader that returns real values is used verbatim.
        let telemetry = TelemetryConfig::from_args_with_reader(
            &default_capabilities(),
            &[],
            false,
            1,
            256,
            false,
            false,
            None,
            || {
                Some(GuestMetricsConfig {
                    cpu_percent: 42,
                    memory_used_mib: 777,
                })
            },
        )
        .unwrap();
        assert_eq!(
            telemetry.metrics,
            Some(GuestMetricsConfig {
                cpu_percent: 42,
                memory_used_mib: 777,
            })
        );

        // When the reader yields None, fall back to the configured synthetic
        // values (here the 256 MiB / 1% defaults).
        let telemetry = TelemetryConfig::from_args_with_reader(
            &default_capabilities(),
            &[],
            false,
            1,
            256,
            false,
            false,
            None,
            || None,
        )
        .unwrap();
        assert_eq!(
            telemetry.metrics,
            Some(GuestMetricsConfig {
                cpu_percent: 1,
                memory_used_mib: 256,
            })
        );
    }

    struct RecordingClockBackend {
        applied: std::rc::Rc<std::cell::RefCell<Vec<u64>>>,
        fail: bool,
    }

    impl ClockBackend for RecordingClockBackend {
        fn set_epoch_millis(&mut self, unix_epoch_millis: u64) -> Result<(), String> {
            if self.fail {
                return Err("injected settimeofday failure".to_string());
            }
            self.applied.borrow_mut().push(unix_epoch_millis);
            Ok(())
        }
    }

    struct RecordingFreezeBackend {
        calls: std::rc::Rc<std::cell::RefCell<Vec<String>>>,
        fail_freeze: Option<PathBuf>,
    }

    impl FilesystemFreezeBackend for RecordingFreezeBackend {
        fn freeze(&mut self, mount: &Path, _timeout_millis: Option<u64>) -> Result<(), String> {
            self.calls
                .borrow_mut()
                .push(format!("freeze:{}", mount.display()));
            if self.fail_freeze.as_deref() == Some(mount) {
                Err("injected freeze failure".to_string())
            } else {
                Ok(())
            }
        }

        fn thaw(&mut self, mount: &Path) -> Result<(), String> {
            self.calls
                .borrow_mut()
                .push(format!("thaw:{}", mount.display()));
            Ok(())
        }
    }

    #[test]
    fn token_resolution_requires_one_source() {
        assert_eq!(
            resolve_token(Some("token-1".to_string()), None).unwrap(),
            "token-1"
        );
        assert!(resolve_token(None, None).is_err());
        assert!(resolve_token(Some("   ".to_string()), None).is_err());
        assert!(resolve_token(Some("token-1".to_string()), Some("token".into())).is_err());
    }

    #[test]
    fn transport_resolution_accepts_socket_or_device_only() {
        assert_eq!(
            resolve_transport(Some("tools.sock".into()), None).unwrap(),
            Some(GuestToolsTransport::Socket("tools.sock".into()))
        );
        assert_eq!(
            resolve_transport(
                None,
                Some("/dev/virtio-ports/org.bridgevm.guest-tools.0".into())
            )
            .unwrap(),
            Some(GuestToolsTransport::Device(
                "/dev/virtio-ports/org.bridgevm.guest-tools.0".into()
            ))
        );
        assert_eq!(resolve_transport(None, None).unwrap(), None);
        assert!(resolve_transport(Some("tools.sock".into()), Some("device".into())).is_err());
    }

    #[test]
    fn token_file_parser_accepts_metadata_json_and_raw_tokens() {
        assert_eq!(
            parse_token_file(r#"{"token":"token-1","created_at_unix":1}"#).unwrap(),
            "token-1"
        );
        assert_eq!(parse_token_file("token-2\n").unwrap(), "token-2");
        assert!(parse_token_file(r#"{"created_at_unix":1}"#).is_err());
        assert!(parse_token_file(r#"{"token":"   ","created_at_unix":1}"#).is_err());
        assert!(parse_token_file("  \n").is_err());
    }

    #[test]
    fn capability_overrides_parse_names_versions_and_reject_duplicates() {
        let capabilities =
            resolve_capabilities(&["heartbeat".to_string(), "clipboard:2".to_string()]).unwrap();

        assert_eq!(
            capabilities,
            vec![
                AgentCapability {
                    name: "heartbeat".to_string(),
                    version: 1
                },
                AgentCapability {
                    name: "clipboard".to_string(),
                    version: 2
                }
            ]
        );
        assert_eq!(resolve_capabilities(&[]).unwrap(), default_capabilities());
        assert!(resolve_capabilities(&["".to_string()]).is_err());
        assert!(resolve_capabilities(&["clipboard:0".to_string()]).is_err());
        assert!(resolve_capabilities(&["clipboard".to_string(), "clipboard".to_string()]).is_err());
    }

    #[test]
    fn initial_status_frames_validate() {
        let telemetry = default_telemetry();
        let envelopes = initial_status_envelopes(&telemetry);
        assert_eq!(envelopes.len(), 3);
        for envelope in envelopes {
            envelope.validate().unwrap();
        }
    }

    #[test]
    fn telemetry_config_parses_overrides_and_honors_capabilities() {
        // Force synthetic metrics (--no-real-metrics) so the exact 17/1024
        // assertion is deterministic regardless of the host the test runs on.
        let telemetry = TelemetryConfig::from_args(
            &default_capabilities(),
            &["192.168.64.10@enp0s1".to_string()],
            false,
            17,
            1024,
            false,
            true,
            Some("guest copied text\n".to_string()),
        )
        .unwrap();
        assert_eq!(telemetry.guest_ips.len(), 1);
        assert_eq!(telemetry.guest_ips[0].address.to_string(), "192.168.64.10");
        assert_eq!(telemetry.guest_ips[0].interface.as_deref(), Some("enp0s1"));
        assert_eq!(
            telemetry.metrics,
            Some(GuestMetricsConfig {
                cpu_percent: 17,
                memory_used_mib: 1024
            })
        );
        assert_eq!(
            telemetry.clipboard_text.as_deref(),
            Some("guest copied text")
        );

        let capabilities = resolve_capabilities(&["heartbeat".to_string()]).unwrap();
        let telemetry =
            TelemetryConfig::from_args(&capabilities, &[], false, 1, 256, false, false, None)
                .unwrap();
        assert!(telemetry.guest_ips.is_empty());
        assert_eq!(telemetry.metrics, None);
        assert_eq!(telemetry.clipboard_text, None);

        assert!(TelemetryConfig::from_args(
            &default_capabilities(),
            &[],
            false,
            101,
            256,
            false,
            false,
            None
        )
        .is_err());
        assert!(TelemetryConfig::from_args(
            &default_capabilities(),
            &["0.0.0.0".to_string()],
            false,
            1,
            256,
            false,
            false,
            None
        )
        .is_err());
        assert!(TelemetryConfig::from_args(
            &capabilities,
            &[],
            false,
            1,
            256,
            false,
            false,
            Some("copy".to_string())
        )
        .is_err());
        assert!(TelemetryConfig::from_args(
            &default_capabilities(),
            &[],
            false,
            1,
            256,
            false,
            false,
            Some("\n".to_string())
        )
        .is_err());
        // --no-metrics and --no-real-metrics are mutually exclusive.
        assert!(TelemetryConfig::from_args(
            &default_capabilities(),
            &[],
            false,
            1,
            256,
            true,
            true,
            None
        )
        .is_err());
    }

    #[test]
    fn clipboard_telemetry_emits_clipboard_changed_frame() {
        let telemetry = TelemetryConfig::from_args(
            &default_capabilities(),
            &[],
            false,
            1,
            256,
            false,
            false,
            Some("hello from guest".to_string()),
        )
        .unwrap();

        let envelopes = initial_status_envelopes(&telemetry);

        assert_eq!(
            envelopes.last().map(|envelope| &envelope.message),
            Some(&AgentMessage::ClipboardChanged {
                text: "hello from guest".to_string()
            })
        );
        for envelope in envelopes {
            envelope.validate().unwrap();
        }
    }

    #[test]
    fn serve_once_writes_hello_and_command_result() {
        let command = encode_envelope_line(&AgentEnvelope::with_request_id(
            AgentMessage::ResizeDisplay {
                width: 1440,
                height: 900,
                scale: 2,
            },
            "resize-1",
        ))
        .unwrap();
        let mut output = Vec::new();

        run_tools_session(
            Cursor::new(command.as_bytes()),
            &mut output,
            "token-1",
            "linux",
            default_capabilities(),
            default_telemetry(),
            None,
            FilesystemFreezer::simulated(),
            ClipboardWriter::simulated(),
            DisplayResizer::simulated(),
            ClockSetter::simulated(),
            true,
        )
        .unwrap();

        let output = String::from_utf8(output).unwrap();
        let mut lines = output.lines().map(|line| format!("{line}\n"));
        let hello = decode_envelope_line(&lines.next().unwrap()).unwrap();
        let heartbeat = decode_envelope_line(&lines.next().unwrap()).unwrap();
        let guest_ip = decode_envelope_line(&lines.next().unwrap()).unwrap();
        let metrics = decode_envelope_line(&lines.next().unwrap()).unwrap();
        let result = decode_envelope_line(&lines.next().unwrap()).unwrap();

        assert!(matches!(hello.message, AgentMessage::GuestHello { .. }));
        assert_eq!(heartbeat.message, AgentMessage::Heartbeat);
        assert!(matches!(
            guest_ip.message,
            AgentMessage::GuestIpChanged { .. }
        ));
        assert!(matches!(metrics.message, AgentMessage::GuestMetrics { .. }));
        assert_eq!(
            result.message,
            AgentMessage::CommandResult {
                request_id: "resize-1".to_string(),
                ok: true,
                error_code: None,
                message: None,
                result: None,
                metadata: None,
            }
        );
        assert_eq!(lines.next(), None);
    }

    #[test]
    fn default_session_handles_commands_until_eof() {
        let commands = [
            AgentEnvelope::with_request_id(
                AgentMessage::TimeSync {
                    unix_epoch_millis: 1,
                },
                "time-1",
            ),
            AgentEnvelope::with_request_id(
                AgentMessage::SetClipboard {
                    text: "hello".to_string(),
                },
                "clipboard-1",
            ),
        ]
        .into_iter()
        .map(|command| encode_envelope_line(&command).unwrap())
        .collect::<String>();
        let mut output = Vec::new();

        run_tools_session(
            Cursor::new(commands.as_bytes()),
            &mut output,
            "token-1",
            "linux",
            default_capabilities(),
            default_telemetry(),
            None,
            FilesystemFreezer::simulated(),
            ClipboardWriter::simulated(),
            DisplayResizer::simulated(),
            ClockSetter::simulated(),
            false,
        )
        .unwrap();

        let output = String::from_utf8(output).unwrap();
        let frames = output
            .lines()
            .map(|line| decode_envelope_line(&format!("{line}\n")).unwrap())
            .collect::<Vec<_>>();

        assert_eq!(frames.len(), 6);
        assert!(matches!(frames[0].message, AgentMessage::GuestHello { .. }));
        assert_eq!(frames[1].message, AgentMessage::Heartbeat);
        assert_eq!(
            frames[4].message,
            AgentMessage::CommandResult {
                request_id: "time-1".to_string(),
                ok: true,
                error_code: None,
                message: Some(
                    "acknowledged time-sync to 1 ms since epoch; guest clock was not changed (simulated)"
                        .to_string()
                ),
                result: Some(serde_json::json!({ "applied_unix_epoch_millis": 1u64 })),
                metadata: None,
            }
        );
        assert_eq!(
            frames[5].message,
            AgentMessage::CommandResult {
                request_id: "clipboard-1".to_string(),
                ok: true,
                error_code: None,
                message: None,
                result: None,
                metadata: None,
            }
        );
    }

    #[test]
    fn unix_socket_session_round_trips_host_command() {
        let socket_path = temp_socket_path();
        let listener = UnixListener::bind(&socket_path).unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());

            let hello = read_frame(&mut reader);
            assert!(matches!(hello.message, AgentMessage::GuestHello { .. }));
            assert_eq!(read_frame(&mut reader).message, AgentMessage::Heartbeat);
            assert!(matches!(
                read_frame(&mut reader).message,
                AgentMessage::GuestIpChanged { .. }
            ));
            assert!(matches!(
                read_frame(&mut reader).message,
                AgentMessage::GuestMetrics { .. }
            ));

            let command = AgentEnvelope::with_request_id(
                AgentMessage::SetClipboard {
                    text: "hello from host".to_string(),
                },
                "clipboard-1",
            );
            write_envelope_line(&mut stream, &command).unwrap();

            let result = read_frame(&mut reader);
            assert_eq!(
                result.message,
                AgentMessage::CommandResult {
                    request_id: "clipboard-1".to_string(),
                    ok: true,
                    error_code: None,
                    message: None,
                    result: None,
                    metadata: None,
                }
            );
        });

        let stream = UnixStream::connect(&socket_path).unwrap();
        let mut writer = stream.try_clone().unwrap();
        run_tools_session(
            stream,
            &mut writer,
            "token-1",
            "linux",
            default_capabilities(),
            default_telemetry(),
            None,
            FilesystemFreezer::simulated(),
            ClipboardWriter::simulated(),
            DisplayResizer::simulated(),
            ClockSetter::simulated(),
            true,
        )
        .unwrap();

        server.join().unwrap();
        let _ = std::fs::remove_file(socket_path);
    }

    fn read_frame(reader: &mut impl BufRead) -> AgentEnvelope {
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        decode_envelope_line(&line).unwrap()
    }

    fn temp_socket_path() -> PathBuf {
        let micros = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros();
        std::env::temp_dir().join(format!("bvmt-{}-{micros}.sock", std::process::id()))
    }

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let micros = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros();
        std::env::temp_dir().join(format!("{prefix}-{}-{micros}", std::process::id()))
    }

    fn default_telemetry() -> TelemetryConfig {
        // Force synthetic metrics so the frame count is deterministic on any
        // host (real /proc reads would vary the values, not the count, but we
        // keep tests host-independent).
        TelemetryConfig::from_args(&default_capabilities(), &[], false, 1, 256, false, true, None)
            .unwrap()
    }
}
