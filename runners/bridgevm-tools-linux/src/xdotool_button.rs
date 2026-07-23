//! Split out of main.rs to keep files under 800 lines.

use crate::*;
use anyhow::Result;
use bridgevm_agent_protocol::AgentCapability;
use bridgevm_agent_protocol::AgentEnvelope;
use bridgevm_agent_protocol::AgentMessage;
use bridgevm_agent_protocol::WindowInputEvent;
use bridgevm_agentd::read_envelope_line;
use bridgevm_agentd::write_envelope_line;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fs;
use std::io::BufReader;
use std::io::Read;
use std::io::Write;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command as ProcessCommand;
use std::process::Stdio;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

pub(crate) fn xdotool_button(button: Option<&str>) -> Result<&'static str, String> {
    match button {
        Some("left") => Ok("1"),
        Some("middle") => Ok("2"),
        Some("right") => Ok("3"),
        Some(button) => Err(format!("unsupported pointer button {button}")),
        None => Err("pointer button is required".to_string()),
    }
}

pub(crate) fn window_input_payload(event: &WindowInputEvent, source: &str) -> serde_json::Value {
    match event {
        WindowInputEvent::Pointer {
            x,
            y,
            action,
            button,
        } => serde_json::json!({
            "kind": "pointer",
            "x": x,
            "y": y,
            "action": action,
            "button": button,
            "source": source
        }),
        WindowInputEvent::Key { key, action } => serde_json::json!({
            "kind": "key",
            "key": key,
            "action": action,
            "source": source
        }),
    }
}

pub(crate) fn window_input_label(event: &WindowInputEvent) -> &'static str {
    match event {
        WindowInputEvent::Pointer { .. } => "pointer",
        WindowInputEvent::Key { .. } => "key",
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DesktopApplication {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DesktopWindow {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) desktop: Option<i64>,
    pub(crate) pid: Option<u32>,
    pub(crate) bounds: Option<DesktopWindowBounds>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DesktopWindowBounds {
    pub(crate) x: i64,
    pub(crate) y: i64,
    pub(crate) width: u64,
    pub(crate) height: u64,
}

pub(crate) fn window_bounds_payload(x: i64, y: i64, width: u64, height: u64) -> serde_json::Value {
    serde_json::json!({
        "x": x,
        "y": y,
        "width": width,
        "height": height
    })
}

pub(crate) fn desktop_window_payload(window: &DesktopWindow) -> serde_json::Value {
    let mut payload = serde_json::json!({
        "id": window.id,
        "title": window.title,
        "source": "wmctrl"
    });
    if let Some(desktop) = window.desktop {
        payload["desktop"] = serde_json::json!(desktop);
    }
    if let Some(pid) = window.pid {
        payload["pid"] = serde_json::json!(pid);
    }
    if let Some(bounds) = &window.bounds {
        payload["bounds"] = window_bounds_payload(bounds.x, bounds.y, bounds.width, bounds.height);
    }
    payload
}

pub(crate) fn read_desktop_applications() -> Result<Vec<DesktopApplication>, String> {
    let mut dirs = vec![
        PathBuf::from("/usr/local/share/applications"),
        PathBuf::from("/usr/share/applications"),
    ];
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join(".local/share/applications"));
    }

    let mut apps = BTreeMap::<String, DesktopApplication>::new();
    for dir in dirs {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("desktop") {
                continue;
            }
            let Some(app) = parse_desktop_application(&path) else {
                continue;
            };
            apps.entry(app.id.clone()).or_insert(app);
        }
    }

    if apps.is_empty() {
        return Err("no visible .desktop applications were found".to_string());
    }
    Ok(apps.into_values().collect())
}

pub(crate) fn parse_desktop_application(path: &Path) -> Option<DesktopApplication> {
    let contents = read_utf8_file_bounded(path, MAX_DESKTOP_FILE_BYTES).ok()?;
    let mut name = None;
    let mut app_type = None;
    let mut no_display = false;
    let mut hidden = false;
    for line in contents.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key {
            "Name" => name = Some(value.trim().to_string()),
            "Type" => app_type = Some(value.trim().to_string()),
            "NoDisplay" => no_display = value.trim().eq_ignore_ascii_case("true"),
            "Hidden" => hidden = value.trim().eq_ignore_ascii_case("true"),
            _ => {}
        }
    }
    if app_type.as_deref() != Some("Application") || no_display || hidden {
        return None;
    }
    let id = path.file_name()?.to_string_lossy().to_string();
    let name = name.filter(|value| !value.is_empty())?;
    Some(DesktopApplication {
        id,
        name,
        path: path.to_path_buf(),
    })
}

pub(crate) fn run_application_launcher(
    launcher: &AppLauncher,
    app: &DesktopApplication,
) -> Result<(), String> {
    match launcher {
        AppLauncher::Gio(program) => {
            let path = app.path.to_string_lossy().to_string();
            run_command_status(program, &["launch", &path])
        }
        AppLauncher::GtkLaunch(program) => run_command_status(program, &[&app.id]),
    }
}

pub(crate) fn read_wmctrl_windows(program: &Path) -> Result<Vec<DesktopWindow>, String> {
    match run_command_output(program, &["-l", "-p", "-G"]) {
        Ok(output) => parse_wmctrl_windows(&output, true).or_else(|_| {
            let fallback = run_command_output(program, &["-l"])?;
            parse_wmctrl_windows(&fallback, false)
        }),
        Err(enhanced_error) => {
            let fallback = run_command_output(program, &["-l"]).map_err(|fallback_error| {
                format!("{enhanced_error}; fallback -l also failed: {fallback_error}")
            })?;
            parse_wmctrl_windows(&fallback, false)
        }
    }
}

pub(crate) fn parse_wmctrl_windows(
    output: &str,
    enhanced: bool,
) -> Result<Vec<DesktopWindow>, String> {
    let windows = output
        .lines()
        .filter_map(|line| {
            if enhanced {
                parse_wmctrl_window_enhanced(line)
            } else {
                parse_wmctrl_window_basic(line)
            }
        })
        .collect::<Vec<_>>();
    if windows.is_empty() {
        return Err("wmctrl reported no desktop windows".to_string());
    }
    Ok(windows)
}

pub(crate) fn parse_wmctrl_window_enhanced(line: &str) -> Option<DesktopWindow> {
    let mut parts = line.split_whitespace();
    let id = parts.next()?.to_string();
    let desktop = parts.next()?.parse::<i64>().ok()?;
    let pid = parts.next()?.parse::<u32>().ok()?;
    let x = parts.next()?.parse::<i64>().ok()?;
    let y = parts.next()?.parse::<i64>().ok()?;
    let width = parts.next()?.parse::<u64>().ok()?;
    let height = parts.next()?.parse::<u64>().ok()?;
    let _host = parts.next()?;
    let title = parts.collect::<Vec<_>>().join(" ");
    if id.is_empty() || title.is_empty() {
        return None;
    }
    Some(DesktopWindow {
        id,
        title,
        desktop: Some(desktop),
        pid: Some(pid),
        bounds: Some(DesktopWindowBounds {
            x,
            y,
            width,
            height,
        }),
    })
}

pub(crate) fn parse_wmctrl_window_basic(line: &str) -> Option<DesktopWindow> {
    let mut parts = line.split_whitespace();
    let id = parts.next()?.to_string();
    let desktop = parts.next()?.parse::<i64>().ok();
    let _host = parts.next()?;
    let title = parts.collect::<Vec<_>>().join(" ");
    if id.is_empty() || title.is_empty() {
        return None;
    }
    Some(DesktopWindow {
        id,
        title,
        desktop,
        pid: None,
        bounds: None,
    })
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

pub(crate) fn normalize_fsfreeze_mounts(mounts: Vec<PathBuf>) -> Result<Vec<PathBuf>> {
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

pub(crate) fn normalize_absolute_path(path: &Path) -> Result<PathBuf> {
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

pub(crate) struct ToolsSessionConfig<'a> {
    pub(crate) token: &'a str,
    pub(crate) guest_os: &'a str,
    pub(crate) capabilities: Vec<AgentCapability>,
    pub(crate) telemetry: TelemetryConfig,
    pub(crate) file_drop_dir: Option<PathBuf>,
    pub(crate) filesystem_freezer: FilesystemFreezer,
    pub(crate) clipboard_writer: ClipboardWriter,
    pub(crate) display_resizer: DisplayResizer,
    pub(crate) clock_setter: ClockSetter,
    pub(crate) desktop_controller: DesktopController,
    pub(crate) serve_once: bool,
}

pub(crate) fn run_tools_session(
    reader: impl Read,
    writer: &mut impl Write,
    config: ToolsSessionConfig<'_>,
) -> Result<()> {
    let ToolsSessionConfig {
        token,
        guest_os,
        capabilities,
        telemetry,
        file_drop_dir,
        filesystem_freezer,
        clipboard_writer,
        display_resizer,
        clock_setter,
        desktop_controller,
        serve_once,
    } = config;
    let mut state = GuestToolsState::new(&capabilities)
        .with_file_drop_dir(file_drop_dir)
        .with_filesystem_freezer(filesystem_freezer)
        .with_clipboard_writer(clipboard_writer)
        .with_display_resizer(display_resizer)
        .with_clock_setter(clock_setter)
        .with_desktop_controller(desktop_controller);
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

/// Session runner with the opt-in continuous clipboard watcher layered on top
/// of the proven `run_tools_session` loop. When `watcher` is `None` (the
/// default, interval 0) this is byte-for-byte the same as `run_tools_session`:
/// no extra thread is spawned and the writer is used directly. When a watcher
/// is supplied, the writer is shared behind a `Mutex` so the watcher thread and
/// the command loop never interleave frames, the watcher polls the real reader
/// on its interval and emits `ClipboardChanged` through the same writer, and the
/// watcher is signalled to stop and joined before this function returns.
pub(crate) fn run_tools_session_watched<R, W>(
    reader: R,
    mut writer: W,
    config: ToolsSessionConfig<'_>,
    clipboard_watcher: Option<ClipboardWatcher>,
) -> Result<()>
where
    R: Read,
    W: Write + Send + 'static,
{
    let Some(watcher) = clipboard_watcher else {
        // Disabled watcher: identical to the historical single-threaded path.
        return run_tools_session(reader, &mut writer, config);
    };

    // Share the writer so the watcher thread and the command loop serialize
    // their frames. write_envelope_line flushes per frame, so a frame written
    // under the lock is complete before the lock is released.
    let shared_writer = Arc::new(Mutex::new(writer));
    let stop = Arc::new(AtomicBool::new(false));
    let watcher_handle =
        spawn_clipboard_watcher(watcher, Arc::clone(&shared_writer), Arc::clone(&stop));

    let result = run_tools_session_shared(reader, &shared_writer, config);

    // Signal the watcher to stop and reap it so no thread leaks and no frame is
    // written after the session loop has returned.
    stop.store(true, Ordering::SeqCst);
    let _ = watcher_handle.join();
    result
}

/// Spawn the watcher thread. It polls the reader every `interval`, feeds reads
/// through the pure `ClipboardWatchState`, and writes a `ClipboardChanged`
/// frame under the shared writer lock on each detected change. It exits when
/// `stop` is set or when writing to the shared writer fails (session gone).
pub(crate) fn spawn_clipboard_watcher<W>(
    watcher: ClipboardWatcher,
    shared_writer: Arc<Mutex<W>>,
    stop: Arc<AtomicBool>,
) -> thread::JoinHandle<()>
where
    W: Write + Send + 'static,
{
    thread::spawn(move || {
        let ClipboardWatcher { interval, reader } = watcher;
        let mut state = ClipboardWatchState::new();
        // Poll in short slices so a long interval still stops promptly.
        let slice = interval
            .min(Duration::from_millis(100))
            .max(Duration::from_millis(1));
        let mut waited = Duration::ZERO;
        loop {
            if stop.load(Ordering::SeqCst) {
                return;
            }
            if waited < interval {
                thread::sleep(slice);
                waited += slice;
                continue;
            }
            waited = Duration::ZERO;

            // A reader error is non-fatal: skip this tick and try again. A
            // misbehaving reader is already bounded by run_clipboard_read_command.
            let latest = reader.read_text().unwrap_or(None);
            if let Some(text) = state.observe(latest) {
                let envelope = AgentEnvelope::new(AgentMessage::ClipboardChanged { text });
                let Ok(mut guard) = shared_writer.lock() else {
                    return;
                };
                if write_envelope_line(&mut *guard, &envelope).is_err() {
                    // Stream closed / session ended: stop watching.
                    return;
                }
            }
        }
    })
}

/// The command loop body, parameterized over a shared writer so the watcher can
/// safely interleave `ClipboardChanged` frames. Mirrors `run_tools_session`.
pub(crate) fn run_tools_session_shared<R, W>(
    reader: R,
    shared_writer: &Arc<Mutex<W>>,
    config: ToolsSessionConfig<'_>,
) -> Result<()>
where
    R: Read,
    W: Write,
{
    let ToolsSessionConfig {
        token,
        guest_os,
        capabilities,
        telemetry,
        file_drop_dir,
        filesystem_freezer,
        clipboard_writer,
        display_resizer,
        clock_setter,
        desktop_controller,
        serve_once,
    } = config;
    let mut state = GuestToolsState::new(&capabilities)
        .with_file_drop_dir(file_drop_dir)
        .with_filesystem_freezer(filesystem_freezer)
        .with_clipboard_writer(clipboard_writer)
        .with_display_resizer(display_resizer)
        .with_clock_setter(clock_setter)
        .with_desktop_controller(desktop_controller);
    let hello = guest_hello(token, guest_os, capabilities);
    {
        let mut guard = shared_writer.lock().expect("writer mutex poisoned");
        write_envelope_line(&mut *guard, &hello).map_err(|error| anyhow::anyhow!("{error:?}"))?;
        for envelope in initial_status_envelopes(&telemetry) {
            write_envelope_line(&mut *guard, &envelope)
                .map_err(|error| anyhow::anyhow!("{error:?}"))?;
        }
    }

    let mut reader = BufReader::new(reader);
    let mut handled_commands = 0usize;
    while let Some(command) =
        read_envelope_line(&mut reader).map_err(|error| anyhow::anyhow!("{error:?}"))?
    {
        if let Some(result) = state.handle_command(&command) {
            let mut guard = shared_writer.lock().expect("writer mutex poisoned");
            write_envelope_line(&mut *guard, &result)
                .map_err(|error| anyhow::anyhow!("{error:?}"))?;
        }
        handled_commands += 1;
        if serve_once && handled_commands >= 1 {
            break;
        }
    }

    Ok(())
}

pub(crate) struct GuestToolsState {
    pub(crate) shared_folders_supported: bool,
    pub(crate) drag_drop_supported: bool,
    pub(crate) applications_supported: bool,
    pub(crate) windows_supported: bool,
    pub(crate) clipboard_supported: bool,
    pub(crate) display_resize_supported: bool,
    pub(crate) fs_freeze_supported: bool,
    pub(crate) fs_thaw_supported: bool,
    pub(crate) time_sync_supported: bool,
    pub(crate) benchmark_supported: bool,
    pub(crate) shared_folders: BTreeMap<String, SharedFolderMount>,
    pub(crate) file_drops: BTreeMap<String, FileDropTransfer>,
    pub(crate) applications: BTreeMap<String, ApplicationEntry>,
    pub(crate) windows: BTreeMap<String, WindowEntry>,
    pub(crate) file_drop_dir: Option<PathBuf>,
    pub(crate) filesystem_frozen: bool,
    pub(crate) filesystem_freezer: FilesystemFreezer,
    pub(crate) clipboard_writer: ClipboardWriter,
    pub(crate) display_resizer: DisplayResizer,
    pub(crate) clock_setter: ClockSetter,
    pub(crate) desktop_controller: DesktopController,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SharedFolderMount {
    pub(crate) host_path_token: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FileDropTransfer {
    pub(crate) file_name: String,
    pub(crate) size_bytes: u64,
    pub(crate) bytes: Vec<u8>,
    pub(crate) chunks_seen: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ApplicationEntry {
    pub(crate) name: String,
    pub(crate) launched: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WindowEntry {
    pub(crate) title: String,
    pub(crate) focused: bool,
    pub(crate) closed: bool,
    pub(crate) bounds: Option<DesktopWindowBounds>,
}

pub(crate) fn window_entry_payload(id: &str, window: &WindowEntry) -> serde_json::Value {
    let mut payload = serde_json::json!({
        "id": id,
        "title": window.title,
    });
    if let Some(bounds) = &window.bounds {
        payload["bounds"] = window_bounds_payload(bounds.x, bounds.y, bounds.width, bounds.height);
    }
    payload
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CommandOutcome {
    pub(crate) ok: bool,
    pub(crate) error_code: Option<String>,
    pub(crate) message: Option<String>,
    pub(crate) result: Option<serde_json::Value>,
    pub(crate) metadata: Option<serde_json::Value>,
}

impl CommandOutcome {
    pub(crate) fn ok(message: impl Into<Option<String>>) -> Self {
        Self {
            ok: true,
            error_code: None,
            message: message.into(),
            result: None,
            metadata: None,
        }
    }

    pub(crate) fn ok_with_result(
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

    pub(crate) fn error(error_code: impl Into<String>, message: impl Into<String>) -> Self {
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
    pub(crate) fn new(capabilities: &[AgentCapability]) -> Self {
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
            benchmark_supported: supports_capability(capabilities, "benchmark"),
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
            desktop_controller: DesktopController::simulated(),
        }
    }

    pub(crate) fn with_file_drop_dir(mut self, file_drop_dir: Option<PathBuf>) -> Self {
        self.file_drop_dir = file_drop_dir;
        self
    }

    pub(crate) fn with_filesystem_freezer(mut self, filesystem_freezer: FilesystemFreezer) -> Self {
        self.filesystem_freezer = filesystem_freezer;
        self
    }

    pub(crate) fn with_clipboard_writer(mut self, clipboard_writer: ClipboardWriter) -> Self {
        self.clipboard_writer = clipboard_writer;
        self
    }

    pub(crate) fn with_display_resizer(mut self, display_resizer: DisplayResizer) -> Self {
        self.display_resizer = display_resizer;
        self
    }

    pub(crate) fn with_clock_setter(mut self, clock_setter: ClockSetter) -> Self {
        self.clock_setter = clock_setter;
        self
    }

    pub(crate) fn with_desktop_controller(mut self, desktop_controller: DesktopController) -> Self {
        self.desktop_controller = desktop_controller;
        self
    }

    pub(crate) fn handle_command(&mut self, command: &AgentEnvelope) -> Option<AgentEnvelope> {
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

    pub(crate) fn apply_command(&mut self, message: &AgentMessage) -> CommandOutcome {
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
            AgentMessage::SetWindowBounds {
                id,
                x,
                y,
                width,
                height,
            } => self.set_window_bounds(id, *x, *y, *width, *height),
            AgentMessage::WindowInput { id, event } => self.window_input(id, event),
            AgentMessage::FreezeFilesystem { timeout_millis } => {
                self.freeze_filesystem(*timeout_millis)
            }
            AgentMessage::ThawFilesystem => self.thaw_filesystem(),
            AgentMessage::RunBenchmark { duration_millis } => self.run_benchmark(*duration_millis),
            _ => CommandOutcome::error(
                "unsupported-command",
                "command is not implemented by the Linux tools scaffold",
            ),
        }
    }

    pub(crate) fn sync_time(&mut self, unix_epoch_millis: u64) -> CommandOutcome {
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

    pub(crate) fn mount_share(&mut self, name: &str, host_path_token: &str) -> CommandOutcome {
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

    pub(crate) fn unmount_share(&mut self, name: &str) -> CommandOutcome {
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

    pub(crate) fn start_file_drop(
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

    pub(crate) fn record_file_drop_chunk(
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

        // Reject as soon as the accumulated bytes would exceed the size declared
        // in FileDropStart, so a misbehaving/compromised sender can't grow this
        // buffer without bound (duplicate or oversized chunks) before the final
        // size check in complete_file_drop.
        if transfer.bytes.len() as u64 + chunk.len() as u64 > transfer.size_bytes {
            return CommandOutcome::error(
                "file-drop-overflow",
                format!(
                    "file drop {transfer_id} chunk {chunk_index} exceeds declared size {}",
                    transfer.size_bytes
                ),
            );
        }

        transfer.bytes.extend_from_slice(&chunk);
        transfer.chunks_seen = transfer.chunks_seen.max(chunk_index.saturating_add(1));
        CommandOutcome::ok(Some(format!(
            "accepted file drop {transfer_id} chunk {chunk_index}"
        )))
    }
}
