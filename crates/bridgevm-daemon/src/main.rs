use anyhow::{Context, Result};
use bridgevm_agent_protocol::{
    AgentEnvelope, AgentMessage, DEFAULT_BENCHMARK_DURATION_MILLIS, MAX_BENCHMARK_DURATION_MILLIS,
};
use bridgevm_agentd::{
    accept_guest_hello, authorize_message, decode_envelope_line, read_envelope_line,
    write_envelope_line, AgentCommandTracker, AgentSession, AgentSessionIoError,
};
use bridgevm_api::{
    add_fast_spawn_blocker, apply_power_aware_fast_resources, build_compatibility_resume_command,
    compat_suspend_marker_path, create_performance_sample, fast_spawn_not_implemented_error,
    fast_suspend_state_path, guest_tools_agent_policy, guest_tools_freeze_filesystem_envelope,
    guest_tools_mount_approved_share_envelope, guest_tools_thaw_filesystem_envelope,
    handle_request, inspect_guest_tools_status, launch_readiness_metadata, resume_backend,
    suspend_backend, ApplicationConsistentSnapshotCommandResultRecord,
    ApplicationConsistentSnapshotExecutionRecord, BridgeVmRequest, BridgeVmResponse,
    GuestToolsCommandRecord, PerformanceMeasurementRecord, PerformanceSampleMetadata,
    SnapshotConsistency,
};
use bridgevm_apple_vz::{build_fast_plan, write_launch_spec_artifact};
use bridgevm_config::VmMode;
use bridgevm_qemu::{
    assign_free_vnc_display, build_compatibility_command, qmp_socket_path, query_status,
    quit as qmp_quit, vnc_display_in_command, QemuError, QmpClient, QmpEventDrain,
};
use bridgevm_storage::{
    GuestToolsAgentUpdateMetadata, GuestToolsClipboardMetadata, GuestToolsCommandResultMetadata,
    GuestToolsIpAddressMetadata, GuestToolsMetricsMetadata, GuestToolsRuntimeMetadata,
    GuestToolsSharedFolderMetadata, LaunchReadinessMetadata, QmpSupervisorMetadata, RunnerMetadata,
    SnapshotKind, VmRuntimeState, VmStore,
};
use clap::Parser;
use std::{
    collections::HashMap,
    env, fs,
    io::ErrorKind,
    io::{BufRead, BufReader, Read, Write},
    os::unix::fs::FileTypeExt,
    os::unix::io::AsRawFd,
    os::unix::net::{UnixListener, UnixStream},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const QMP_SUPERVISOR_DRAIN_LIMIT: usize = 16;
const GUEST_TOOLS_DRAIN_LIMIT: usize = 16;
const GUEST_TOOLS_COMMAND_RESULT_TIMEOUT: Duration = Duration::from_secs(5);

/// Set by the SIGTERM/SIGINT handler so the supervisor loop can reap its
/// spawned QEMU/AppleVzRunner children before exiting. Without this, killing
/// `bridgevmd` (the common case: a service restart, or a test harness tearing
/// the daemon down) would leave its VM processes orphaned — still running and
/// still holding their ports (e.g. VNC :0 / TCP 5900) with no supervisor.
static SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);

extern "C" fn handle_shutdown_signal(_signal: libc::c_int) {
    // Async-signal-safe: only flips an atomic. The actual teardown happens in
    // the supervisor loop, which polls this flag.
    SHUTDOWN_REQUESTED.store(true, Ordering::SeqCst);
}

fn install_shutdown_handlers() {
    // SAFETY: `handle_shutdown_signal` does nothing but an atomic store, which
    // is async-signal-safe, so installing it as a C signal handler is sound.
    unsafe {
        let handler = handle_shutdown_signal as *const () as libc::sighandler_t;
        libc::signal(libc::SIGTERM, handler);
        libc::signal(libc::SIGINT, handler);
    }
}

#[derive(Debug, Parser)]
#[command(name = "bridgevmd", about = "BridgeVM core daemon scaffold")]
struct Args {
    #[arg(long, value_name = "PATH")]
    store: Option<PathBuf>,
    #[arg(long, default_value = "bridgevmd.sock", value_name = "SOCKET")]
    socket_name: String,
    #[arg(long)]
    once: bool,
    #[arg(long, default_value_t = 250, value_name = "MILLIS")]
    reconcile_interval_ms: u64,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let store = args
        .store
        .map(VmStore::new)
        .unwrap_or_else(VmStore::default);
    store
        .ensure()
        .context("failed to initialize BridgeVM store")?;

    let socket_path = store.root().join("run").join(args.socket_name);
    println!("bridgevmd store: {}", store.root().display());
    println!("bridgevmd socket: {}", socket_path.display());
    println!("bridgevmd status: metadata service ready");

    if args.once {
        return Ok(());
    }

    serve(
        store,
        &socket_path,
        Duration::from_millis(args.reconcile_interval_ms),
    )
}

fn serve(store: VmStore, socket_path: &Path, reconcile_interval: Duration) -> Result<()> {
    let listener = bind_daemon_listener(socket_path)?;
    listener
        .set_nonblocking(true)
        .context("failed to configure daemon socket")?;
    println!("bridgevmd listening");
    install_shutdown_handlers();
    let mut state = DaemonState::new(store);
    let mut last_reconcile = Instant::now();

    loop {
        if SHUTDOWN_REQUESTED.load(Ordering::SeqCst) {
            println!("bridgevmd received shutdown signal; reaping supervised backends");
            state.shutdown_reap_children();
            println!("bridgevmd shutdown complete");
            return Ok(());
        }

        match listener.accept() {
            Ok(stream) => {
                if let Err(error) = handle_connection(&mut state, stream.0) {
                    eprintln!("bridgevmd request failed: {error:#}");
                }
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock => {}
            Err(error) => eprintln!("bridgevmd accept failed: {error}"),
        }

        if last_reconcile.elapsed() >= reconcile_interval {
            if let Err(error) = state.reconcile_children() {
                eprintln!("bridgevmd supervisor failed: {error:#}");
            }
            last_reconcile = Instant::now();
        }
        thread::sleep(Duration::from_millis(25));
    }
}

fn bind_daemon_listener(socket_path: &Path) -> Result<UnixListener> {
    if let Some(parent) = socket_path.parent() {
        fs::create_dir_all(parent).context("failed to create daemon run directory")?;
    }
    if socket_path.exists() {
        let metadata = match fs::symlink_metadata(socket_path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == ErrorKind::NotFound => {
                return UnixListener::bind(socket_path).context("failed to bind daemon socket");
            }
            Err(error) => {
                return Err(error).context("failed to inspect existing daemon socket path");
            }
        };
        if !metadata.file_type().is_socket() {
            anyhow::bail!(
                "daemon socket path exists and is not a socket: {}",
                socket_path.display()
            );
        }
        match UnixStream::connect(socket_path) {
            Ok(_) => {
                anyhow::bail!("daemon socket already in use: {}", socket_path.display());
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) if error.kind() == ErrorKind::ConnectionRefused => {
                fs::remove_file(socket_path).context("failed to remove stale daemon socket")?;
            }
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "failed to connect to existing daemon socket: {}",
                        socket_path.display()
                    )
                });
            }
        }
    }

    UnixListener::bind(socket_path).context("failed to bind daemon socket")
}

fn handle_connection(state: &mut DaemonState, mut stream: UnixStream) -> Result<()> {
    stream
        .set_nonblocking(false)
        .context("failed to configure daemon client stream")?;
    let mut line = String::new();
    BufReader::new(stream.try_clone()?)
        .read_line(&mut line)
        .context("failed to read daemon request")?;
    let request = serde_json::from_str::<BridgeVmRequest>(&line).context("invalid request JSON")?;
    let response = state.handle_request(request);
    serde_json::to_writer(&mut stream, &response).context("failed to write daemon response")?;
    stream.write_all(b"\n")?;
    Ok(())
}

struct DaemonState {
    store: VmStore,
    children: HashMap<String, SupervisedBackend>,
}

struct SupervisedBackend {
    child: Child,
    qmp: Option<QmpClient>,
    guest_tools: Option<AgentSession>,
    guest_tools_stream: Option<BufReader<UnixStream>>,
    /// A guest-tools socket connection established host-first (right after the
    /// backend is spawned, before the guest agent boots) and HELD open across
    /// reconcile ticks. The guest agent writes its `GuestHello` exactly once,
    /// as the first frame, when it comes up ~a minute into boot. Connecting
    /// fresh on each tick races past that one-shot hello (the daemon would read
    /// a later Heartbeat first -> `ExpectedGuestHello`), so instead we connect
    /// once and keep this reader until the hello arrives or the socket dies.
    guest_tools_pending: Option<UnixStream>,
    guest_tools_commands: AgentCommandTracker,
}

impl SupervisedBackend {
    fn new(child: Child) -> Self {
        Self {
            child,
            qmp: None,
            guest_tools: None,
            guest_tools_stream: None,
            guest_tools_pending: None,
            guest_tools_commands: AgentCommandTracker::new(),
        }
    }
}

struct FastModeSpawnConfig {
    lightvm_runner: PathBuf,
    apple_vz_runner: PathBuf,
    stop_after_seconds: Option<u64>,
    force_stop_grace_seconds: Option<u64>,
    verify_apple_vz_runner_entitlement: bool,
}

impl FastModeSpawnConfig {
    fn from_env() -> Result<Option<Self>> {
        if !env_flag_enabled("BRIDGEVM_APPLE_VZ_ALLOW_REAL_START") {
            return Ok(None);
        }
        let apple_vz_runner = if let Some(path) =
            env::var_os("BRIDGEVM_APPLE_VZ_RUNNER").map(PathBuf::from)
        {
            path
        } else if let Some(path) = bundled_helper_path("AppleVzRunner") {
            path
        } else {
            anyhow::bail!("BRIDGEVM_APPLE_VZ_ALLOW_REAL_START=1 requires BRIDGEVM_APPLE_VZ_RUNNER");
        };
        let lightvm_runner = env::var_os("BRIDGEVM_LIGHTVM_RUNNER")
            .map(PathBuf::from)
            .or_else(|| bundled_helper_path("lightvm-runner"))
            .unwrap_or_else(|| PathBuf::from("lightvm-runner"));

        Ok(Some(Self {
            lightvm_runner,
            apple_vz_runner,
            stop_after_seconds: env_optional_u64("BRIDGEVM_APPLE_VZ_STOP_AFTER_SECONDS")?,
            force_stop_grace_seconds: env_optional_u64(
                "BRIDGEVM_APPLE_VZ_FORCE_STOP_GRACE_SECONDS",
            )?,
            verify_apple_vz_runner_entitlement: true,
        }))
    }

    fn validate(&self) -> Result<()> {
        require_executable(
            &self.lightvm_runner,
            "BRIDGEVM_LIGHTVM_RUNNER/lightvm-runner",
        )?;
        require_executable(
            &self.apple_vz_runner,
            "BRIDGEVM_APPLE_VZ_RUNNER/AppleVzRunner",
        )?;
        if self.verify_apple_vz_runner_entitlement {
            verify_apple_vz_runner_entitlement(&self.apple_vz_runner)?;
        }
        Ok(())
    }

    /// Build the `lightvm-runner` argv, optionally restoring a saved Apple VZ
    /// machine state (`--apple-vz-restore-state`) for a Fast Mode resume.
    fn runner_args_with_restore(
        &self,
        launch_spec_path: &Path,
        restore_state: Option<&Path>,
    ) -> Vec<String> {
        let mut args = vec![
            "--launch-spec".to_string(),
            launch_spec_path.display().to_string(),
            "--require-ready".to_string(),
            "--launch".to_string(),
            "--apple-vz-runner".to_string(),
            self.apple_vz_runner.display().to_string(),
            "--apple-vz-allow-real-start".to_string(),
        ];
        if let Some(state_path) = restore_state {
            args.push("--apple-vz-restore-state".to_string());
            args.push(state_path.display().to_string());
        }
        if let Some(seconds) = self.stop_after_seconds {
            args.push("--apple-vz-stop-after-seconds".to_string());
            args.push(seconds.to_string());
        }
        if let Some(seconds) = self.force_stop_grace_seconds {
            args.push("--apple-vz-force-stop-grace-seconds".to_string());
            args.push(seconds.to_string());
        }
        args
    }
}

fn bundled_helper_path(name: &str) -> Option<PathBuf> {
    let exe = env::current_exe().ok()?;
    bundled_helper_path_from_exe(&exe, name)
}

fn bundled_helper_path_from_exe(exe: &Path, name: &str) -> Option<PathBuf> {
    let helper = exe.parent()?.join(name);
    if helper.is_file() && is_executable(&helper) {
        Some(helper)
    } else {
        None
    }
}

fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::metadata(path)
            .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        path.is_file()
    }
}

fn require_executable(path: &Path, label: &str) -> Result<()> {
    let metadata = fs::metadata(path)
        .with_context(|| format!("{label} is missing or not readable: {}", path.display()))?;
    if !metadata.is_file() {
        anyhow::bail!("{label} is not a file: {}", path.display());
    }
    if !is_executable(path) {
        anyhow::bail!("{label} is not executable: {}", path.display());
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn verify_apple_vz_runner_entitlement(path: &Path) -> Result<()> {
    let output = Command::new("codesign")
        .args(["-d", "--entitlements", ":-"])
        .arg(path)
        .output()
        .with_context(|| {
            format!(
                "failed to inspect AppleVzRunner entitlements: {}",
                path.display()
            )
        })?;
    if !output.status.success() {
        anyhow::bail!(
            "AppleVzRunner entitlement preflight failed for {}: {}",
            path.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    if !entitlement_plist_has_true(&stdout, "com.apple.security.virtualization") {
        anyhow::bail!(
            "AppleVzRunner is missing com.apple.security.virtualization entitlement: {}",
            path.display()
        );
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn verify_apple_vz_runner_entitlement(_path: &Path) -> Result<()> {
    Ok(())
}

fn entitlement_plist_has_true(plist: &str, key: &str) -> bool {
    let key_tag = format!("<key>{key}</key>");
    let Some(after_key) = plist.split_once(&key_tag).map(|(_, after)| after) else {
        return false;
    };
    let value = after_key.trim_start();
    value.starts_with("<true/>") || value.starts_with("<true />")
}

fn env_flag_enabled(name: &str) -> bool {
    env::var(name)
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
}

/// Test-only extra QEMU args for daemon-spawned Compatibility backends.
///
/// Read from `BRIDGEVM_COMPAT_EXTRA_QEMU_ARGS` and shell-word split. This is an
/// integration-test seam (e.g. attaching a NoCloud cidata seed ISO for the
/// application-consistent live opt-in smoke) and is unset in normal operation.
fn compat_extra_qemu_args() -> Vec<String> {
    match env::var("BRIDGEVM_COMPAT_EXTRA_QEMU_ARGS") {
        Ok(value) => shell_word_split(&value),
        Err(_) => Vec::new(),
    }
}

/// Minimal POSIX-ish shell word splitter supporting single and double quotes.
/// Sufficient for passing QEMU `-drive file=...,...` style args from tests.
fn shell_word_split(input: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut in_word = false;
    let mut quote: Option<char> = None;
    let mut chars = input.chars();
    while let Some(c) = chars.next() {
        match quote {
            Some(q) => {
                if c == q {
                    quote = None;
                } else if q == '"' && c == '\\' {
                    if let Some(next) = chars.next() {
                        current.push(next);
                    }
                } else {
                    current.push(c);
                }
            }
            None => {
                if c == '\'' || c == '"' {
                    quote = Some(c);
                    in_word = true;
                } else if c == '\\' {
                    if let Some(next) = chars.next() {
                        current.push(next);
                        in_word = true;
                    }
                } else if c.is_whitespace() {
                    if in_word {
                        words.push(std::mem::take(&mut current));
                        in_word = false;
                    }
                } else {
                    current.push(c);
                    in_word = true;
                }
            }
        }
    }
    if in_word {
        words.push(current);
    }
    words
}

fn env_optional_u64(name: &str) -> Result<Option<u64>> {
    let Some(value) = env::var(name).ok().filter(|value| !value.trim().is_empty()) else {
        return Ok(None);
    };
    let parsed = value
        .parse::<u64>()
        .with_context(|| format!("{name} must be a positive integer"))?;
    if parsed == 0 {
        anyhow::bail!("{name} must be a positive integer");
    }
    Ok(Some(parsed))
}

fn launch_readiness_blocker_summary(readiness: &LaunchReadinessMetadata) -> String {
    if readiness.blockers.is_empty() {
        return "unknown blocker".to_string();
    }
    readiness
        .blockers
        .iter()
        .map(|blocker| match (&blocker.path, &blocker.capability) {
            (Some(path), _) => format!("{} ({})", blocker.code, path.display()),
            (None, Some(capability)) => format!("{} ({capability})", blocker.code),
            (None, None) => blocker.code.clone(),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

impl DaemonState {
    fn new(store: VmStore) -> Self {
        Self {
            store,
            children: HashMap::new(),
        }
    }

    fn handle_request(&mut self, request: BridgeVmRequest) -> BridgeVmResponse {
        if let Err(error) = self.reconcile_children() {
            return BridgeVmResponse::Error {
                message: error.to_string(),
            };
        }

        match request {
            BridgeVmRequest::RunBackend { name, spawn: true } => self
                .spawn_backend(&name)
                .unwrap_or_else(|error| BridgeVmResponse::Error {
                    message: error.to_string(),
                }),
            BridgeVmRequest::ResumeBackend { name } => self
                .resume_backend_supervised(&name)
                .unwrap_or_else(|error| BridgeVmResponse::Error {
                    message: error.to_string(),
                }),
            BridgeVmRequest::SuspendBackend { name } => self
                .suspend_backend_supervised(&name)
                .unwrap_or_else(|error| BridgeVmResponse::Error {
                    message: error.to_string(),
                }),
            BridgeVmRequest::StopBackend { name } if self.children.contains_key(&name) => self
                .stop_owned_backend(&name)
                .unwrap_or_else(|error| BridgeVmResponse::Error {
                    message: error.to_string(),
                }),
            BridgeVmRequest::RestartVm { name } if self.children.contains_key(&name) => self
                .restart_owned_backend(&name)
                .unwrap_or_else(|error| BridgeVmResponse::Error {
                    message: error.to_string(),
                }),
            BridgeVmRequest::GuestToolsSendCommand { name, envelope }
                if self.children.contains_key(&name) =>
            {
                self.send_guest_tools_command(&name, envelope)
                    .unwrap_or_else(|error| BridgeVmResponse::Error {
                        message: error.to_string(),
                    })
            }
            BridgeVmRequest::GuestToolsMountApprovedShare {
                name,
                share,
                request_id,
            } if self.children.contains_key(&name) => {
                guest_tools_mount_approved_share_envelope(&self.store, &name, &share, request_id)
                    .and_then(|envelope| {
                        self.send_guest_tools_command(&name, envelope)
                            .map_err(|error| error.to_string())
                    })
                    .unwrap_or_else(|message| BridgeVmResponse::Error { message })
            }
            BridgeVmRequest::SnapshotPreflightStatus { name, consistency }
                if self.children.contains_key(&name) =>
            {
                self.owned_backend_snapshot_preflight_status(&name, consistency)
                    .unwrap_or_else(|error| BridgeVmResponse::Error {
                        message: error.to_string(),
                    })
            }
            BridgeVmRequest::ExecuteApplicationConsistentSnapshot {
                vm,
                name,
                freeze_timeout_millis,
            } if self.children.contains_key(&vm) => self
                .execute_application_consistent_snapshot(&vm, &name, freeze_timeout_millis)
                .unwrap_or_else(|error| BridgeVmResponse::Error {
                    message: error.to_string(),
                }),
            BridgeVmRequest::CreatePerformanceSample {
                name,
                output,
                artifact_bytes,
                iterations,
                sync,
            } if self.children.contains_key(&name) => self
                .create_performance_sample_with_optional_guest_benchmark(
                    &name,
                    output,
                    artifact_bytes,
                    iterations,
                    sync,
                )
                .unwrap_or_else(|error| BridgeVmResponse::Error {
                    message: error.to_string(),
                }),
            request => handle_request(&self.store, request),
        }
    }

    fn reconcile_children(&mut self) -> Result<()> {
        let mut exited = Vec::new();
        let mut terminal = Vec::new();
        for (name, backend) in &mut self.children {
            if backend
                .child
                .try_wait()
                .with_context(|| format!("failed to poll backend '{name}'"))?
                .is_some()
            {
                exited.push(name.clone());
                continue;
            }

            let Ok((bundle, _)) = self.store.get_vm(name) else {
                continue;
            };

            if let Err(error) = reconcile_guest_tools_session(&self.store, name, backend) {
                eprintln!("bridgevmd guest-tools supervisor failed for '{name}': {error:#}");
            }
            if let Err(error) = drain_guest_tools_messages(&self.store, name, backend) {
                eprintln!("bridgevmd guest-tools drain failed for '{name}': {error:#}");
            }

            let socket_path = qmp_socket_path(&bundle);
            if !socket_path.exists() {
                continue;
            }

            if backend.qmp.is_none() {
                backend.qmp = connect_supervisor_qmp(&socket_path).ok();
            }

            let qmp_report = qmp_supervisor_report(&mut backend.qmp, &socket_path);
            if let Some(drain) = qmp_report.drain.as_ref() {
                if let Err(error) = write_qmp_supervisor_metadata(&self.store, name, drain) {
                    eprintln!("bridgevmd QMP supervisor metadata failed for '{name}': {error:#}");
                }
            }
            if qmp_report.terminal {
                terminal.push(name.clone());
            }
        }

        for name in exited {
            self.children.remove(&name);
            let _ = self.store.transition_state(&name, VmRuntimeState::Stopped);
            self.store
                .clear_runner_metadata(&name)
                .with_context(|| format!("failed to clear runner metadata for '{name}'"))?;
        }
        for name in terminal {
            if self.children.contains_key(&name) {
                self.cleanup_owned_backend(&name, false)
                    .with_context(|| format!("failed to clean up terminal backend '{name}'"))?;
            }
        }
        Ok(())
    }

    fn spawn_backend(&mut self, name: &str) -> Result<BridgeVmResponse> {
        if self.children.contains_key(name) {
            anyhow::bail!("backend is already running for '{name}'");
        }

        let (bundle, manifest, _) = self
            .store
            .get_vm_with_active_disk(name)
            .context("failed to read VM")?;
        if manifest.mode == VmMode::Fast {
            if let Some(config) = FastModeSpawnConfig::from_env()? {
                return self.spawn_fast_backend(name, bundle, manifest, config);
            }

            let response = handle_request(
                &self.store,
                BridgeVmRequest::RunBackend {
                    name: name.to_string(),
                    spawn: false,
                },
            )
            .into_result()
            .map_err(anyhow::Error::msg)?;
            let BridgeVmResponse::RunnerStatus {
                metadata: Some(mut metadata),
                ..
            } = response
            else {
                anyhow::bail!("Fast Mode dry-run planning did not return runner metadata");
            };
            let readiness =
                metadata
                    .launch_readiness
                    .get_or_insert_with(|| LaunchReadinessMetadata {
                        ready: false,
                        blockers: Vec::new(),
                    });
            add_fast_spawn_blocker(readiness);
            let spawn_error = fast_spawn_not_implemented_error(readiness);
            self.store
                .write_runner_metadata(name, &metadata)
                .context("failed to write Fast Mode runner metadata")?;
            anyhow::bail!("{}", spawn_error);
        }
        let (disk, active_disk) = self
            .store
            .prepare_active_disk(name)
            .context("failed to prepare active disk")?;
        if !disk.exists {
            if let Some(command) = &disk.create_command {
                anyhow::bail!(
                    "active disk is not ready: {}; create it with: {}",
                    disk.path.display(),
                    command.join(" ")
                );
            }
            anyhow::bail!("active disk is not ready: {}", disk.path.display());
        }

        let mut command = build_compatibility_command(&manifest, &bundle)
            .map_err(|error| anyhow::anyhow!("{}", compatibility_qemu_command_error(error)))?;
        // Pin this VM to a free VNC display so concurrent Compat VMs don't
        // collide on TCP 5900. Avoid displays already handed to live children
        // (their QEMU may not have bound the port yet, so a bare probe would
        // hand the same :0 to two back-to-back launches).
        let avoid = self.live_vnc_displays();
        assign_free_vnc_display(&mut command, &avoid).map_err(|error| anyhow::anyhow!(error))?;
        // Test-only escape hatch (mirrors BRIDGEVM_APPLE_VZ_RUNNER): append extra
        // QEMU args without touching the product command builder. The
        // application-consistent live opt-in smoke uses this to attach a NoCloud
        // cidata seed ISO so a daemon-owned guest can boot the agent. Args are
        // shell-word split; empty/unset means no change.
        let extra_compat_args = compat_extra_qemu_args();
        let log_path = bundle.join("logs").join("qemu.log");
        let guest_tools = self
            .store
            .guest_tools_runner_metadata(name)
            .context("failed to prepare guest tools runner metadata")?;
        fs::create_dir_all(bundle.join("logs")).context("failed to create VM log directory")?;
        let stdout = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .context("failed to open QEMU log file")?;
        let stderr = stdout
            .try_clone()
            .context("failed to clone QEMU log file")?;
        let child = Command::new(&command.program)
            .args(&command.args)
            .args(&extra_compat_args)
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr))
            .spawn()
            .with_context(|| format!("failed to spawn {}", command.program))?;

        let metadata = RunnerMetadata {
            engine: "fullvm".to_string(),
            pid: Some(child.id()),
            command: command.render_shell_words(),
            log_path,
            started_at_unix: now_unix(),
            dry_run: false,
            launch_spec_path: None,
            guest_tools: Some(guest_tools),
            disk: Some(disk),
            active_disk: Some(active_disk),
            launch_readiness: None,
        };
        self.store
            .write_runner_metadata(name, &metadata)
            .context("failed to write runner metadata")?;
        self.store
            .transition_state(name, VmRuntimeState::Running)
            .context("failed to mark VM running")?;
        self.children
            .insert(name.to_string(), SupervisedBackend::new(child));

        Ok(BridgeVmResponse::RunnerStatus {
            metadata: Some(metadata),
            qmp_supervisor: self
                .store
                .qmp_supervisor_metadata(name)
                .context("failed to read QMP supervisor metadata")?,
        })
    }

    fn spawn_fast_backend(
        &mut self,
        name: &str,
        bundle: PathBuf,
        manifest: bridgevm_config::VmManifest,
        config: FastModeSpawnConfig,
    ) -> Result<BridgeVmResponse> {
        self.spawn_fast_backend_with_restore(name, bundle, manifest, config, None)
    }

    fn spawn_fast_backend_with_restore(
        &mut self,
        name: &str,
        bundle: PathBuf,
        mut manifest: bridgevm_config::VmManifest,
        config: FastModeSpawnConfig,
        restore_state: Option<PathBuf>,
    ) -> Result<BridgeVmResponse> {
        config.validate()?;
        // Battery-adaptive `auto` resources on a fresh cold start (the app's
        // primary path goes through the daemon). Not on resume: a restored VM
        // must reuse the exact saved-state config.
        if restore_state.is_none() {
            apply_power_aware_fast_resources(&mut manifest);
        }
        let (disk, active_disk) = self
            .store
            .prepare_active_disk(name)
            .context("failed to prepare active disk")?;
        let plan = build_fast_plan(&manifest, &bundle).context("failed to build Apple VZ plan")?;
        let launch_spec_path = write_launch_spec_artifact(&bundle, plan.launch_spec())
            .context("failed to write Fast Mode Apple VZ launch spec")?;
        let readiness = launch_readiness_metadata(&plan.launch_spec().readiness);
        if !readiness.ready {
            anyhow::bail!(
                "Fast Mode launch readiness failed: {}",
                launch_readiness_blocker_summary(&readiness)
            );
        }

        fs::create_dir_all(bundle.join("logs")).context("failed to create VM log directory")?;
        let log_path: PathBuf = plan.launch_spec().logs.runner_log_path.clone().into();
        let stdout = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .context("failed to open Apple VZ runner log file")?;
        let stderr = stdout
            .try_clone()
            .context("failed to clone Apple VZ runner log file")?;

        let args = config.runner_args_with_restore(&launch_spec_path, restore_state.as_deref());
        let mut child = Command::new(&config.lightvm_runner);
        child.args(&args);
        child.env("BRIDGEVM_APPLE_VZ_ALLOW_REAL_START", "1");
        let child = child
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr))
            .spawn()
            .with_context(|| {
                format!(
                    "failed to spawn Fast Mode runner {}",
                    config.lightvm_runner.display()
                )
            })?;

        let mut command = vec![config.lightvm_runner.display().to_string()];
        command.extend(args);
        let metadata = RunnerMetadata {
            engine: "lightvm".to_string(),
            pid: Some(child.id()),
            command,
            log_path,
            started_at_unix: now_unix(),
            dry_run: false,
            launch_spec_path: Some(launch_spec_path),
            guest_tools: None,
            disk: Some(disk),
            active_disk: Some(active_disk),
            launch_readiness: Some(readiness),
        };
        self.store
            .write_runner_metadata(name, &metadata)
            .context("failed to write Fast Mode runner metadata")?;
        self.store
            .transition_state(name, VmRuntimeState::Running)
            .context("failed to mark VM running")?;
        self.children
            .insert(name.to_string(), SupervisedBackend::new(child));

        Ok(BridgeVmResponse::RunnerStatus {
            metadata: Some(metadata),
            qmp_supervisor: self
                .store
                .qmp_supervisor_metadata(name)
                .context("failed to read QMP supervisor metadata")?,
        })
    }

    fn stop_owned_backend(&mut self, name: &str) -> Result<BridgeVmResponse> {
        self.cleanup_owned_backend(name, true)
    }

    fn restart_owned_backend(&mut self, name: &str) -> Result<BridgeVmResponse> {
        self.cleanup_owned_backend(name, true)?;
        Ok(BridgeVmResponse::State {
            name: name.to_string(),
            metadata: self
                .store
                .transition_state(name, VmRuntimeState::Running)
                .context("failed to mark VM running after restart")?,
        })
    }

    /// Suspend a backend through the daemon.
    ///
    /// Suspend is synchronous (pause -> save state -> quit). If the daemon owns
    /// the child, drop our `Child`/QMP handles first (without killing) so the
    /// api suspend path can drive QMP and terminate the recorded pid without the
    /// reconcile loop racing it. The api suspend path leaves the VM `suspended`.
    fn suspend_backend_supervised(&mut self, name: &str) -> Result<BridgeVmResponse> {
        // Release the owned handles before the synchronous suspend so the
        // supervisor does not poll/clear state underneath it. The api suspend
        // path is responsible for terminating the recorded pid.
        self.children.remove(name);
        let metadata = suspend_backend(&self.store, name).map_err(anyhow::Error::msg)?;
        Ok(BridgeVmResponse::RunnerStatus {
            metadata: Some(metadata),
            qmp_supervisor: self
                .store
                .qmp_supervisor_metadata(name)
                .context("failed to read QMP supervisor metadata")?,
        })
    }

    /// Resume a backend through the daemon, tracking the new child in the
    /// supervisor exactly like cold-start `run` so reconcile/stop see it.
    ///
    /// Fast Mode: relaunch `lightvm-runner` with `--apple-vz-restore-state`.
    /// Compatibility Mode: relaunch QEMU with `-loadvm <tag>`. In both cases the
    /// child is inserted into `self.children`. When the Fast Mode real-start
    /// env is not configured, fall back to the daemon-less api resume (which is
    /// detached, matching legacy behavior).
    fn resume_backend_supervised(&mut self, name: &str) -> Result<BridgeVmResponse> {
        if self.children.contains_key(name) {
            anyhow::bail!("backend is already running for '{name}'");
        }
        let (bundle, manifest, _) = self
            .store
            .get_vm_with_active_disk(name)
            .context("failed to read VM")?;

        match manifest.mode {
            VmMode::Fast => {
                let state_path = fast_suspend_state_path(&bundle, name);
                if !state_path.exists() {
                    anyhow::bail!(
                        "no saved Fast Mode state to resume from at {}; suspend the VM first",
                        state_path.display()
                    );
                }
                if let Some(config) = FastModeSpawnConfig::from_env()? {
                    return self.spawn_fast_backend_with_restore(
                        name,
                        bundle,
                        manifest,
                        config,
                        Some(state_path),
                    );
                }
                // Real-start env not configured: fall back to detached api resume.
                let metadata = resume_backend(&self.store, name).map_err(anyhow::Error::msg)?;
                Ok(BridgeVmResponse::RunnerStatus {
                    metadata: Some(metadata),
                    qmp_supervisor: self
                        .store
                        .qmp_supervisor_metadata(name)
                        .context("failed to read QMP supervisor metadata")?,
                })
            }
            VmMode::Compatibility => self.resume_compatibility_supervised(name, &bundle, &manifest),
        }
    }

    fn resume_compatibility_supervised(
        &mut self,
        name: &str,
        bundle: &Path,
        manifest: &bridgevm_config::VmManifest,
    ) -> Result<BridgeVmResponse> {
        let marker_path = compat_suspend_marker_path(bundle, name);
        if !marker_path.exists() {
            anyhow::bail!(
                "no saved Compatibility Mode state to resume from at {}; suspend the VM first",
                marker_path.display()
            );
        }
        let (disk, active_disk) = self
            .store
            .prepare_active_disk(name)
            .context("failed to prepare active disk")?;
        if !disk.exists {
            anyhow::bail!("active disk is not ready: {}", disk.path.display());
        }

        let mut command = build_compatibility_resume_command(manifest, bundle)
            .map_err(|error| anyhow::anyhow!("{error}"))?;
        // Pin a free VNC display so a resumed Compat VM doesn't collide on 5900,
        // avoiding displays already owned by this daemon's live children.
        let avoid = self.live_vnc_displays();
        assign_free_vnc_display(&mut command, &avoid).map_err(|error| anyhow::anyhow!(error))?;
        let log_path = bundle.join("logs").join("qemu.log");
        let guest_tools = self
            .store
            .guest_tools_runner_metadata(name)
            .context("failed to prepare guest tools runner metadata")?;
        fs::create_dir_all(bundle.join("logs")).context("failed to create VM log directory")?;
        let stdout = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .context("failed to open QEMU log file")?;
        let stderr = stdout
            .try_clone()
            .context("failed to clone QEMU log file")?;
        let child = Command::new(&command.program)
            .args(&command.args)
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr))
            .spawn()
            .with_context(|| format!("failed to spawn {}", command.program))?;

        let metadata = RunnerMetadata {
            engine: "fullvm".to_string(),
            pid: Some(child.id()),
            command: command.render_shell_words(),
            log_path,
            started_at_unix: now_unix(),
            dry_run: false,
            launch_spec_path: None,
            guest_tools: Some(guest_tools),
            disk: Some(disk),
            active_disk: Some(active_disk),
            launch_readiness: None,
        };
        self.store
            .write_runner_metadata(name, &metadata)
            .context("failed to write runner metadata")?;
        // Resume marker consumed.
        let _ = fs::remove_file(&marker_path);
        self.store
            .transition_state(name, VmRuntimeState::Running)
            .context("failed to mark VM running")?;
        self.children
            .insert(name.to_string(), SupervisedBackend::new(child));

        Ok(BridgeVmResponse::RunnerStatus {
            metadata: Some(metadata),
            qmp_supervisor: self
                .store
                .qmp_supervisor_metadata(name)
                .context("failed to read QMP supervisor metadata")?,
        })
    }

    fn send_guest_tools_command(
        &mut self,
        name: &str,
        envelope: AgentEnvelope,
    ) -> Result<BridgeVmResponse> {
        Ok(BridgeVmResponse::GuestToolsCommand {
            command: self.send_guest_tools_command_record(name, envelope)?,
        })
    }

    fn send_guest_tools_command_record(
        &mut self,
        name: &str,
        envelope: AgentEnvelope,
    ) -> Result<GuestToolsCommandRecord> {
        let backend = self
            .children
            .get_mut(name)
            .with_context(|| format!("backend is not owned by this daemon for '{name}'"))?;
        let session = backend
            .guest_tools
            .as_ref()
            .with_context(|| format!("guest tools session is not connected for '{name}'"))?;
        let stream = backend
            .guest_tools_stream
            .as_mut()
            .with_context(|| format!("guest tools stream is not connected for '{name}'"))?;

        backend
            .guest_tools_commands
            .begin_host_command(session, &envelope)
            .map_err(|error| anyhow::anyhow!("guest tools command rejected: {error:?}"))?;
        write_envelope_line(stream.get_mut(), &envelope)
            .map_err(|error| anyhow::anyhow!("failed to write guest tools command: {error:?}"))?;

        Ok(GuestToolsCommandRecord {
            vm: name.to_string(),
            request_id: envelope.request_id,
            pending_commands: backend.guest_tools_commands.pending_count(),
        })
    }

    fn create_performance_sample_with_optional_guest_benchmark(
        &mut self,
        name: &str,
        output: PathBuf,
        artifact_bytes: Option<u64>,
        iterations: Option<u16>,
        sync: bool,
    ) -> Result<BridgeVmResponse> {
        let mut sample =
            create_performance_sample(&self.store, name, output, artifact_bytes, iterations, sync)
                .map_err(anyhow::Error::msg)?;

        match self.run_guest_benchmark_for_sample(name, sample.created_at_unix) {
            Ok(Some(completed)) => record_guest_benchmark_result(&mut sample, &completed),
            Ok(None) => sample.notes.push(
                "guest benchmark skipped because no benchmark-capable guest-tools session was connected"
                    .to_string(),
            ),
            Err(error) => sample
                .notes
                .push(format!("guest benchmark skipped: {error}")),
        }

        if let Ok(status) = inspect_guest_tools_status(&self.store, name) {
            sample.metrics = status
                .runtime
                .as_ref()
                .and_then(|runtime| runtime.metrics.clone());
            sample.guest_tools = status;
        }
        fs::write(
            &sample.artifact,
            serde_json::to_string_pretty(&sample).context("failed to serialize sample")?,
        )
        .with_context(|| {
            format!(
                "failed to update performance sample metadata at {}",
                sample.artifact.display()
            )
        })?;

        Ok(BridgeVmResponse::PerformanceSample { sample })
    }

    fn run_guest_benchmark_for_sample(
        &mut self,
        name: &str,
        created_at_unix: u64,
    ) -> Result<Option<CompletedGuestToolsCommand>> {
        let supports_benchmark = self
            .children
            .get(name)
            .and_then(|backend| backend.guest_tools.as_ref())
            .is_some_and(|session| session.supports("benchmark"));
        if !supports_benchmark {
            return Ok(None);
        }

        let request_id = format!("performance-sample:{created_at_unix}:guest-benchmark");
        let envelope = AgentEnvelope::with_request_id(
            AgentMessage::RunBenchmark {
                duration_millis: Some(DEFAULT_BENCHMARK_DURATION_MILLIS),
            },
            request_id.clone(),
        );
        self.send_guest_tools_command_record(name, envelope)?;
        self.wait_for_guest_tools_command_result(
            name,
            &request_id,
            Duration::from_millis(MAX_BENCHMARK_DURATION_MILLIS.saturating_add(5_000)),
        )
        .map(Some)
    }

    fn execute_application_consistent_snapshot(
        &mut self,
        vm: &str,
        snapshot: &str,
        freeze_timeout_millis: Option<u64>,
    ) -> Result<BridgeVmResponse> {
        let BridgeVmResponse::SnapshotPreflightStatus { preflight } = self
            .owned_backend_snapshot_preflight_status(
                vm,
                SnapshotConsistency::ApplicationConsistent,
            )?
        else {
            anyhow::bail!("snapshot preflight request returned unexpected response");
        };
        if !preflight.ready {
            anyhow::bail!(
                "application-consistent snapshot preflight is not ready: {}",
                preflight
                    .blockers
                    .iter()
                    .map(|blocker| blocker.code.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }

        let freeze_request_id = format!("application-consistent-snapshot:{snapshot}:freeze");
        let thaw_request_id = format!("application-consistent-snapshot:{snapshot}:thaw");

        self.send_guest_tools_command_record(
            vm,
            guest_tools_freeze_filesystem_envelope(
                freeze_request_id.clone(),
                freeze_timeout_millis,
            ),
        )?;
        let freeze_result = self.wait_for_guest_tools_command_result(
            vm,
            &freeze_request_id,
            command_result_timeout(freeze_timeout_millis),
        )?;
        if !freeze_result.ok {
            // Freeze did not enter the boundary (the agent rejected it), so the
            // guest is not quiesced and there is nothing to thaw. Still issue a
            // best-effort thaw so a partially-frozen agent cannot get stuck.
            let thaw_attempted = self.dispatch_and_await_thaw(vm, &thaw_request_id).is_ok();
            anyhow::bail!(
                "guest tools freeze failed for application-consistent snapshot '{}': {}; thaw attempted: {}",
                snapshot,
                freeze_result
                    .error_code
                    .as_deref()
                    .unwrap_or("command-result-not-ok"),
                thaw_attempted
            );
        }

        // The guest is now frozen. From here on the filesystem MUST be thawed no
        // matter what happens to the snapshot, so we capture the snapshot result
        // WITHOUT propagating it, then unconditionally dispatch + await the thaw,
        // and only afterwards surface any errors. This guarantees the thaw is
        // always sent even when the snapshot fails.
        let snapshot_result =
            self.store
                .create_snapshot(vm, snapshot, SnapshotKind::ApplicationConsistent);
        let thaw_result = self.dispatch_and_await_thaw(vm, &thaw_request_id);

        let snapshot_metadata = snapshot_result.with_context(|| {
            format!("failed to create application-consistent snapshot '{snapshot}'")
        })?;
        let thaw_result = thaw_result.with_context(|| {
            format!("snapshot '{snapshot}' was recorded, but thaw dispatch failed")
        })?;
        if !thaw_result.ok {
            anyhow::bail!(
                "snapshot '{}' was recorded, but guest tools thaw failed: {}",
                snapshot,
                thaw_result
                    .error_code
                    .as_deref()
                    .unwrap_or("command-result-not-ok")
            );
        }

        Ok(BridgeVmResponse::ApplicationConsistentSnapshotExecution {
            execution: ApplicationConsistentSnapshotExecutionRecord {
                vm: vm.to_string(),
                snapshot: snapshot.to_string(),
                freeze_request_id,
                thaw_request_id,
                pending_commands_after_freeze: freeze_result.pending_commands,
                pending_commands_after_thaw: thaw_result.pending_commands,
                snapshot_created_at_unix: snapshot_metadata.created_at_unix,
                freeze_result: freeze_result.into_record(),
                thaw_result: thaw_result.into_record(),
                preflight_ready: true,
                note: "Received successful guest-tools freeze/thaw CommandResult frames around snapshot creation; with the agent's Real fsfreeze backend this enters the OS fsfreeze boundary, but this still does not prove OS-level application consistency (it depends on guest applications flushing their own state).".to_string(),
            },
        })
    }

    /// Dispatches a ThawFilesystem command and waits for its CommandResult.
    ///
    /// This is the single thaw step used by [`execute_application_consistent_snapshot`]
    /// so that the freeze boundary is always closed exactly once, regardless of
    /// whether the snapshot succeeded or failed.
    fn dispatch_and_await_thaw(
        &mut self,
        vm: &str,
        thaw_request_id: &str,
    ) -> Result<CompletedGuestToolsCommand> {
        self.send_guest_tools_command_record(
            vm,
            guest_tools_thaw_filesystem_envelope(thaw_request_id.to_string()),
        )?;
        self.wait_for_guest_tools_command_result(
            vm,
            thaw_request_id,
            GUEST_TOOLS_COMMAND_RESULT_TIMEOUT,
        )
    }

    fn owned_backend_snapshot_preflight_status(
        &self,
        name: &str,
        consistency: bridgevm_api::SnapshotConsistency,
    ) -> Result<BridgeVmResponse> {
        let response = handle_request(
            &self.store,
            BridgeVmRequest::SnapshotPreflightStatus {
                name: name.to_string(),
                consistency,
            },
        )
        .into_result()
        .map_err(anyhow::Error::msg)?;
        let BridgeVmResponse::SnapshotPreflightStatus { mut preflight } = response else {
            anyhow::bail!("snapshot preflight request returned unexpected response");
        };

        preflight.backend_freeze_thaw_supported = true;
        preflight
            .blockers
            .retain(|blocker| blocker.code != "backend-freeze-thaw-unavailable");
        preflight.ready = preflight.blockers.is_empty();

        Ok(BridgeVmResponse::SnapshotPreflightStatus { preflight })
    }

    fn wait_for_guest_tools_command_result(
        &mut self,
        name: &str,
        request_id: &str,
        timeout: Duration,
    ) -> Result<CompletedGuestToolsCommand> {
        let deadline = Instant::now() + timeout;
        loop {
            let backend = self
                .children
                .get_mut(name)
                .with_context(|| format!("backend is not owned by this daemon for '{name}'"))?;
            let session = backend
                .guest_tools
                .clone()
                .with_context(|| format!("guest tools session is not connected for '{name}'"))?;
            let Some(reader) = backend.guest_tools_stream.as_mut() else {
                anyhow::bail!("guest tools stream is not connected for '{name}'");
            };

            let envelope = match read_envelope_line(reader) {
                Ok(Some(envelope)) => envelope,
                Ok(None) => {
                    backend.guest_tools = None;
                    backend.guest_tools_stream = None;
                    anyhow::bail!("guest tools stream closed while waiting for '{request_id}'");
                }
                Err(error) if error.is_idle_io() => {
                    if Instant::now() >= deadline {
                        anyhow::bail!(
                            "timed out waiting for guest tools command result '{request_id}'"
                        );
                    }
                    thread::sleep(Duration::from_millis(10));
                    continue;
                }
                Err(error) => {
                    backend.guest_tools = None;
                    backend.guest_tools_stream = None;
                    anyhow::bail!("failed to read guest tools frame: {error:?}");
                }
            };

            if let Some(completed) =
                process_guest_tools_envelope(&self.store, name, backend, &session, envelope)?
            {
                if completed.request_id == request_id {
                    return Ok(completed);
                }
            }

            if Instant::now() >= deadline {
                anyhow::bail!("timed out waiting for guest tools command result '{request_id}'");
            }
        }
    }

    fn cleanup_owned_backend(
        &mut self,
        name: &str,
        send_qmp_quit: bool,
    ) -> Result<BridgeVmResponse> {
        let (bundle, _) = self.store.get_vm(name).context("failed to read VM")?;
        let socket_path = qmp_socket_path(&bundle);
        if send_qmp_quit && socket_path.exists() {
            qmp_quit(&socket_path).context("failed to send QMP quit")?;
        }

        let mut backend = self
            .children
            .remove(name)
            .with_context(|| format!("backend is not owned by this daemon for '{name}'"))?;
        let mut exited = false;
        for _ in 0..40 {
            if backend
                .child
                .try_wait()
                .with_context(|| format!("failed to poll backend '{name}'"))?
                .is_some()
            {
                exited = true;
                break;
            }
            thread::sleep(Duration::from_millis(50));
        }

        if !exited {
            match backend.child.kill() {
                Ok(()) => {}
                // The child can exit between our poll and the kill; Rust returns
                // InvalidInput for an already-exited child. Fine -- reap below.
                Err(error) if error.kind() == ErrorKind::InvalidInput => {}
                // A genuine kill failure: still reap what we can so the child can
                // never orphan, then surface the error.
                Err(error) => {
                    let _ = backend.child.wait();
                    return Err(error)
                        .with_context(|| format!("failed to terminate backend '{name}'"));
                }
            }
            let _ = backend.child.wait();
        }

        self.store
            .transition_state(name, VmRuntimeState::Stopped)
            .context("failed to mark VM stopped")?;
        self.store
            .clear_runner_metadata(name)
            .context("failed to clear runner metadata")?;
        Ok(BridgeVmResponse::RunnerStatus {
            metadata: None,
            qmp_supervisor: self
                .store
                .qmp_supervisor_metadata(name)
                .context("failed to read QMP supervisor metadata")?,
        })
    }

    /// VNC display numbers currently owned by this daemon's live supervised
    /// backends, read back from their recorded launch commands. A newly launched
    /// Compat VM avoids these so it doesn't collide on an in-use VNC port even
    /// before the owning VM's QEMU has finished binding it.
    fn live_vnc_displays(&self) -> Vec<u16> {
        self.children
            .keys()
            .filter_map(|name| self.store.runner_metadata(name).ok().flatten())
            .filter(|metadata| !metadata.dry_run && metadata.pid.is_some())
            .filter_map(|metadata| vnc_display_in_command(&metadata.command))
            .collect()
    }

    /// Tear down every backend this daemon spawned — gracefully (QMP `quit` for
    /// Compatibility Mode, then `SIGTERM`/`SIGKILL`) — so no QEMU/AppleVzRunner
    /// child is orphaned when `bridgevmd` exits. The daemon has no re-adoption
    /// path (a restarted daemon does not reclaim children by pid), so a child it
    /// leaves behind is a pure leak that keeps holding its ports. Best-effort:
    /// failing to reap one backend is logged and does not block the rest, and
    /// any child that somehow survives a failed cleanup is force-killed.
    fn shutdown_reap_children(&mut self) {
        let names: Vec<String> = self.children.keys().cloned().collect();
        for name in names {
            if let Err(error) = self.cleanup_owned_backend(&name, true) {
                // The graceful path bailed (e.g. an unresponsive QMP socket).
                // If the child is still owned here, cleanup failed before
                // killing it, so force-kill so it cannot orphan; otherwise it
                // was already killed and only a later metadata step failed.
                if let Some(mut backend) = self.children.remove(&name) {
                    eprintln!(
                        "bridgevmd shutdown: graceful reap of '{name}' failed ({error:#}); force-killing"
                    );
                    let _ = backend.child.kill();
                    let _ = backend.child.wait();
                } else {
                    eprintln!(
                        "bridgevmd shutdown: reaped backend '{name}' but post-kill cleanup failed: {error:#}"
                    );
                }
            }
        }
    }
}

fn connect_supervisor_qmp(socket_path: &Path) -> Result<QmpClient, QemuError> {
    let mut client = QmpClient::connect_with_timeout(socket_path, Duration::from_millis(25))?;
    client.negotiate()?;
    Ok(client)
}

fn compatibility_qemu_command_error(error: QemuError) -> String {
    format!("failed to build Compatibility Mode QEMU command: {error}")
}

struct QmpSupervisorReport {
    terminal: bool,
    drain: Option<QmpEventDrain>,
}

fn qmp_supervisor_report(
    client: &mut Option<QmpClient>,
    socket_path: &Path,
) -> QmpSupervisorReport {
    let Some(client_ref) = client.as_mut() else {
        return QmpSupervisorReport {
            terminal: qmp_status_is_terminal(socket_path),
            drain: None,
        };
    };

    match client_ref.drain_events(QMP_SUPERVISOR_DRAIN_LIMIT) {
        Ok(drain) => {
            let terminal = drain.has_terminal_event();
            let should_record =
                drain.envelopes_read > 0 || drain.limit_reached || drain.terminal_event.is_some();
            QmpSupervisorReport {
                terminal,
                drain: should_record.then_some(drain),
            }
        }
        Err(error) if error.is_qmp_idle() => QmpSupervisorReport {
            terminal: false,
            drain: None,
        },
        Err(_) => {
            *client = None;
            QmpSupervisorReport {
                terminal: qmp_status_is_terminal(socket_path),
                drain: None,
            }
        }
    }
}

fn qmp_status_is_terminal(socket_path: &Path) -> bool {
    query_status(socket_path)
        .map(|status| status.is_terminal())
        .unwrap_or(false)
}

fn command_result_timeout(freeze_timeout_millis: Option<u64>) -> Duration {
    freeze_timeout_millis
        .map(|millis| Duration::from_millis(millis).saturating_add(Duration::from_secs(1)))
        .unwrap_or(GUEST_TOOLS_COMMAND_RESULT_TIMEOUT)
}

fn write_qmp_supervisor_metadata(store: &VmStore, name: &str, drain: &QmpEventDrain) -> Result<()> {
    store
        .write_qmp_supervisor_metadata(
            name,
            &QmpSupervisorMetadata {
                events: drain.events.clone(),
                terminal_event: drain.terminal_event.clone(),
                envelopes_read: drain.envelopes_read,
                limit_reached: drain.limit_reached,
                updated_at_unix: now_unix(),
            },
        )
        .context("failed to write QMP supervisor metadata")
}

fn reconcile_guest_tools_session(
    store: &VmStore,
    name: &str,
    backend: &mut SupervisedBackend,
) -> Result<()> {
    if backend.guest_tools.is_some() {
        return Ok(());
    }

    let metadata = store
        .guest_tools_runner_metadata(name)
        .context("failed to read guest tools runner metadata")?;
    if !metadata.socket_path.exists() {
        return Ok(());
    }

    // Connect host-first and HOLD the connection. The guest agent emits its
    // `GuestHello` once, as the first frame on the channel, when it boots ~a
    // minute in. If we reconnected on every tick we would usually attach AFTER
    // that one-shot hello had already flushed and instead read a later frame
    // (its periodic Heartbeat), which `read_guest_session` rejects as
    // `ExpectedGuestHello`. Connecting once up front (well before the agent is
    // up) guarantees the hello reaches us as the first frame on this held
    // reader. This mirrors how the live opt-in harness connects before
    // launching the agent.
    if backend.guest_tools_pending.is_none() {
        let stream = match UnixStream::connect(&metadata.socket_path) {
            Ok(stream) => stream,
            Err(error)
                if matches!(
                    error.kind(),
                    ErrorKind::NotFound | ErrorKind::ConnectionRefused | ErrorKind::WouldBlock
                ) =>
            {
                return Ok(());
            }
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "failed to connect guest tools socket {}",
                        metadata.socket_path.display()
                    )
                });
            }
        };
        stream
            .set_read_timeout(Some(Duration::from_millis(25)))
            .context("failed to configure guest tools read timeout")?;
        backend.guest_tools_pending = Some(stream);
    }

    let policy = guest_tools_agent_policy(store, name).map_err(anyhow::Error::msg)?;
    let stream = backend
        .guest_tools_pending
        .as_mut()
        .expect("guest tools pending stream present");

    // Peek (MSG_PEEK) for a COMPLETE newline-terminated frame before consuming
    // anything. This makes the held connection resumable: the agent's one-shot
    // GuestHello can be split across host reads (virtio-serial chunks it), and a
    // plain `read_line` over the 25ms-timeout socket would consume a partial
    // frame, lose those bytes when the timeout fires mid-frame, then fail to
    // parse the tail and reset -- permanently missing the (already-flushed)
    // hello. Only consuming once the whole line is present means a mid-frame
    // timeout can never drop bytes.
    let mut peek = [0u8; 16384];
    // `UnixStream::peek` is unstable on stable Rust, so peek via libc recv(2)
    // with MSG_PEEK. SAFETY: `fd` is a valid open socket owned by `stream` for
    // the duration of the call, and `peek` is a valid writable buffer of
    // `peek.len()` bytes. The socket's SO_RCVTIMEO (the read timeout set above)
    // applies, so this returns EAGAIN rather than blocking when no data is ready.
    let peeked = unsafe {
        libc::recv(
            stream.as_raw_fd(),
            peek.as_mut_ptr() as *mut libc::c_void,
            peek.len(),
            libc::MSG_PEEK,
        )
    };
    let peeked = if peeked > 0 {
        peeked as usize
    } else if peeked == 0 {
        // EOF: the socket closed (VM/QEMU gone). Drop + reconnect next tick.
        backend.guest_tools_pending = None;
        return Ok(());
    } else {
        let error = std::io::Error::last_os_error();
        if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) {
            // No data yet (agent still booting). Keep the held connection.
            return Ok(());
        }
        return Err(error).context("failed to peek guest tools socket");
    };
    let Some(newline) = peek[..peeked].iter().position(|&byte| byte == b'\n') else {
        if peeked == peek.len() {
            // A frame fills the entire peek window with no newline: oversized or
            // malformed (a well-formed GuestHello is far smaller). Waiting would
            // spin forever since the newline can never appear inside the window.
            // Reset so the next tick reconnects host-first.
            eprintln!(
                "bridgevmd resetting guest-tools session for '{name}': oversized handshake frame"
            );
            backend.guest_tools_pending = None;
            return Ok(());
        }
        // Only a partial frame is buffered so far -- leave it unconsumed and
        // wait for the rest on a later tick.
        return Ok(());
    };

    // A whole line is present, so consuming exactly it cannot time out mid-frame.
    let mut frame = vec![0u8; newline + 1];
    stream
        .read_exact(&mut frame)
        .context("failed to read guest hello frame")?;
    let frame = String::from_utf8_lossy(&frame);
    let session = decode_envelope_line(&frame)
        .map_err(AgentSessionIoError::from)
        .and_then(|envelope| {
            accept_guest_hello(&envelope, &policy).map_err(AgentSessionIoError::from)
        });
    match session {
        Ok(session) => {
            write_guest_tools_runtime(store, name, &session, GuestToolsRuntimeUpdate::Connected)?;
            let stream = backend
                .guest_tools_pending
                .take()
                .expect("guest tools pending stream present after accept");
            backend.guest_tools = Some(session);
            // Bytes the agent sent right after the hello (its initial Heartbeat +
            // status burst) are still in the kernel socket buffer; the drain
            // reader picks them up.
            backend.guest_tools_stream = Some(BufReader::new(stream));
        }
        // The first frame was not a valid GuestHello -> reset and reconnect.
        Err(error) => {
            eprintln!("bridgevmd resetting guest-tools session for '{name}': {error:?}");
            backend.guest_tools_pending = None;
        }
    }
    Ok(())
}

fn drain_guest_tools_messages(
    store: &VmStore,
    name: &str,
    backend: &mut SupervisedBackend,
) -> Result<()> {
    let Some(session) = backend.guest_tools.clone() else {
        return Ok(());
    };
    for _ in 0..GUEST_TOOLS_DRAIN_LIMIT {
        let envelope = {
            let Some(reader) = backend.guest_tools_stream.as_mut() else {
                return Ok(());
            };
            match read_envelope_line(reader) {
                Ok(Some(envelope)) => envelope,
                Ok(None) => {
                    backend.guest_tools = None;
                    backend.guest_tools_stream = None;
                    return Ok(());
                }
                Err(error) if error.is_idle_io() => return Ok(()),
                Err(error) => {
                    backend.guest_tools = None;
                    backend.guest_tools_stream = None;
                    anyhow::bail!("failed to read guest tools frame: {error:?}");
                }
            }
        };

        process_guest_tools_envelope(store, name, backend, &session, envelope)?;
    }

    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CompletedGuestToolsCommand {
    request_id: String,
    capability: Option<String>,
    ok: bool,
    error_code: Option<String>,
    message: Option<String>,
    result: Option<serde_json::Value>,
    metadata: Option<serde_json::Value>,
    completed_at_unix: u64,
    pending_commands: usize,
}

impl CompletedGuestToolsCommand {
    fn into_record(self) -> ApplicationConsistentSnapshotCommandResultRecord {
        ApplicationConsistentSnapshotCommandResultRecord {
            request_id: self.request_id,
            capability: self.capability,
            ok: self.ok,
            error_code: self.error_code,
            message: self.message,
            completed_at_unix: self.completed_at_unix,
        }
    }
}

fn record_guest_benchmark_result(
    sample: &mut PerformanceSampleMetadata,
    completed: &CompletedGuestToolsCommand,
) {
    sample
        .notes
        .retain(|note| note != "host-side sample; no guest benchmark workloads were executed");
    if !completed.ok {
        let reason = completed
            .error_code
            .as_deref()
            .or(completed.message.as_deref())
            .unwrap_or("command-result-not-ok");
        sample.notes.push(format!(
            "guest benchmark command did not produce measurements: {reason}"
        ));
        return;
    }

    sample.notes.push(format!(
        "guest benchmark executed over daemon-owned guest-tools session (request id {})",
        completed.request_id
    ));
    let Some(result) = completed.result.as_ref() else {
        sample
            .notes
            .push("guest benchmark completed without a result payload".to_string());
        return;
    };

    push_guest_benchmark_measurement(
        &mut sample.measurements,
        result,
        "/budget_duration_millis",
        "guest_benchmark_budget_millis",
        "milliseconds",
        "guest_tools.benchmark.budget_duration_millis",
    );
    push_guest_benchmark_measurement(
        &mut sample.measurements,
        result,
        "/cpu/iterations",
        "guest_benchmark_cpu_iterations",
        "count",
        "guest_tools.benchmark.cpu.iterations",
    );
    push_guest_benchmark_measurement(
        &mut sample.measurements,
        result,
        "/cpu/elapsed_millis",
        "guest_benchmark_cpu_elapsed_millis",
        "milliseconds",
        "guest_tools.benchmark.cpu.elapsed_millis",
    );
    push_guest_benchmark_measurement(
        &mut sample.measurements,
        result,
        "/cpu/ops_per_sec",
        "guest_benchmark_cpu_ops_per_sec",
        "ops_per_second",
        "guest_tools.benchmark.cpu.ops_per_sec",
    );
    push_guest_benchmark_measurement(
        &mut sample.measurements,
        result,
        "/disk/bytes_written",
        "guest_benchmark_disk_bytes_written",
        "bytes",
        "guest_tools.benchmark.disk.bytes_written",
    );
    push_guest_benchmark_measurement(
        &mut sample.measurements,
        result,
        "/disk/elapsed_millis",
        "guest_benchmark_disk_elapsed_millis",
        "milliseconds",
        "guest_tools.benchmark.disk.elapsed_millis",
    );
    push_guest_benchmark_measurement(
        &mut sample.measurements,
        result,
        "/disk/mib_per_sec",
        "guest_benchmark_disk_mib_per_sec",
        "MiB_per_second",
        "guest_tools.benchmark.disk.mib_per_sec",
    );
    if let Some(error) = result.get("disk_error").and_then(|value| value.as_str()) {
        sample.notes.push(format!(
            "guest benchmark disk micro-benchmark skipped: {error}"
        ));
    }
}

fn push_guest_benchmark_measurement(
    measurements: &mut Vec<PerformanceMeasurementRecord>,
    result: &serde_json::Value,
    pointer: &str,
    name: &str,
    unit: &str,
    source: &str,
) {
    if let Some(value) = result.pointer(pointer).and_then(|value| value.as_u64()) {
        measurements.push(PerformanceMeasurementRecord {
            name: name.to_string(),
            value,
            unit: unit.to_string(),
            source: source.to_string(),
            metadata_only: false,
        });
    }
}

fn process_guest_tools_envelope(
    store: &VmStore,
    name: &str,
    backend: &mut SupervisedBackend,
    session: &AgentSession,
    envelope: AgentEnvelope,
) -> Result<Option<CompletedGuestToolsCommand>> {
    authorize_message(session, &envelope.message)
        .map_err(|error| anyhow::anyhow!("unauthorized guest tools message: {error:?}"))?;
    match &envelope.message {
        AgentMessage::CommandResult {
            request_id,
            ok,
            error_code,
            message,
            result,
            metadata,
        } => {
            let pending = backend
                .guest_tools_commands
                .complete_command_result(&envelope)
                .map_err(|error| {
                    anyhow::anyhow!("unexpected guest tools command result: {error:?}")
                })?;
            let completed_at_unix = now_unix();
            write_guest_tools_runtime(
                store,
                name,
                session,
                GuestToolsRuntimeUpdate::CommandResult {
                    request_id: request_id.clone(),
                    capability: pending.capability.clone(),
                    ok: *ok,
                    error_code: error_code.clone(),
                    message: message.clone(),
                    result: result.clone(),
                    metadata: metadata.clone(),
                    completed_at_unix,
                },
            )?;
            if *ok {
                match pending.message {
                    AgentMessage::MountShare {
                        name: share_name,
                        host_path_token,
                    } => {
                        write_guest_tools_runtime(
                            store,
                            name,
                            session,
                            GuestToolsRuntimeUpdate::MountShare {
                                name: share_name,
                                host_path_token,
                            },
                        )?;
                    }
                    AgentMessage::UnmountShare { name: share_name } => {
                        write_guest_tools_runtime(
                            store,
                            name,
                            session,
                            GuestToolsRuntimeUpdate::UnmountShare { name: share_name },
                        )?;
                    }
                    _ => {}
                }
            }
            Ok(Some(CompletedGuestToolsCommand {
                request_id: request_id.clone(),
                capability: pending.capability,
                ok: *ok,
                error_code: error_code.clone(),
                message: message.clone(),
                result: result.clone(),
                metadata: metadata.clone(),
                completed_at_unix,
                pending_commands: backend.guest_tools_commands.pending_count(),
            }))
        }
        AgentMessage::Heartbeat => {
            write_guest_tools_runtime(store, name, session, GuestToolsRuntimeUpdate::Heartbeat)?;
            Ok(None)
        }
        AgentMessage::GuestIpChanged { addresses } => {
            write_guest_tools_runtime(
                store,
                name,
                session,
                GuestToolsRuntimeUpdate::GuestIp(
                    addresses
                        .iter()
                        .map(|address| GuestToolsIpAddressMetadata {
                            address: address.address.to_string(),
                            interface: address.interface.clone(),
                        })
                        .collect(),
                ),
            )?;
            Ok(None)
        }
        AgentMessage::GuestMetrics {
            cpu_percent,
            memory_used_mib,
        } => {
            write_guest_tools_runtime(
                store,
                name,
                session,
                GuestToolsRuntimeUpdate::Metrics {
                    cpu_percent: *cpu_percent,
                    memory_used_mib: *memory_used_mib,
                },
            )?;
            Ok(None)
        }
        AgentMessage::AgentUpdateAvailable {
            current_version,
            available_version,
            download_url,
            signature,
        } => {
            write_guest_tools_runtime(
                store,
                name,
                session,
                GuestToolsRuntimeUpdate::AgentUpdateAvailable {
                    current_version: current_version.clone(),
                    available_version: available_version.clone(),
                    download_url: download_url.clone(),
                    signature: signature.clone(),
                },
            )?;
            Ok(None)
        }
        AgentMessage::ClipboardChanged { text } => {
            write_guest_tools_runtime(
                store,
                name,
                session,
                GuestToolsRuntimeUpdate::Clipboard { text: text.clone() },
            )?;
            Ok(None)
        }
        _ => Ok(None),
    }
}

enum GuestToolsRuntimeUpdate {
    Connected,
    Heartbeat,
    GuestIp(Vec<GuestToolsIpAddressMetadata>),
    MountShare {
        name: String,
        host_path_token: String,
    },
    UnmountShare {
        name: String,
    },
    Metrics {
        cpu_percent: u8,
        memory_used_mib: u64,
    },
    CommandResult {
        request_id: String,
        capability: Option<String>,
        ok: bool,
        error_code: Option<String>,
        message: Option<String>,
        result: Option<serde_json::Value>,
        metadata: Option<serde_json::Value>,
        completed_at_unix: u64,
    },
    AgentUpdateAvailable {
        current_version: String,
        available_version: String,
        download_url: Option<String>,
        signature: Option<String>,
    },
    Clipboard {
        text: String,
    },
}

fn write_guest_tools_runtime(
    store: &VmStore,
    name: &str,
    session: &AgentSession,
    update: GuestToolsRuntimeUpdate,
) -> Result<()> {
    let now = now_unix();
    let mut metadata = store
        .guest_tools_runtime_metadata(name)
        .context("failed to read guest tools runtime metadata")?
        .unwrap_or_else(|| GuestToolsRuntimeMetadata {
            connected: true,
            guest_os: Some(session.guest_os.clone()),
            agent_version: session.agent_version.clone(),
            capabilities: session
                .capabilities
                .iter()
                .map(|capability| capability.name.clone())
                .collect(),
            last_heartbeat_at_unix: None,
            guest_ip_addresses: Vec::new(),
            shared_folders: Vec::new(),
            metrics: None,
            last_command_result: None,
            agent_update: None,
            clipboard: None,
            updated_at_unix: now,
        });

    metadata.connected = true;
    metadata.guest_os = Some(session.guest_os.clone());
    metadata.agent_version = session.agent_version.clone();
    metadata.capabilities = session
        .capabilities
        .iter()
        .map(|capability| capability.name.clone())
        .collect();
    metadata.updated_at_unix = now;

    match update {
        GuestToolsRuntimeUpdate::Connected => {}
        GuestToolsRuntimeUpdate::Heartbeat => metadata.last_heartbeat_at_unix = Some(now),
        GuestToolsRuntimeUpdate::GuestIp(addresses) => metadata.guest_ip_addresses = addresses,
        GuestToolsRuntimeUpdate::MountShare {
            name,
            host_path_token,
        } => {
            if let Some(folder) = metadata
                .shared_folders
                .iter_mut()
                .find(|folder| folder.name == name)
            {
                folder.host_path_token = host_path_token;
                folder.mounted_at_unix = now;
            } else {
                metadata
                    .shared_folders
                    .push(GuestToolsSharedFolderMetadata {
                        name,
                        host_path_token,
                        mounted_at_unix: now,
                    });
            }
        }
        GuestToolsRuntimeUpdate::UnmountShare { name } => {
            metadata.shared_folders.retain(|folder| folder.name != name);
        }
        GuestToolsRuntimeUpdate::Metrics {
            cpu_percent,
            memory_used_mib,
        } => {
            metadata.metrics = Some(GuestToolsMetricsMetadata {
                cpu_percent,
                memory_used_mib,
                updated_at_unix: now,
            });
        }
        GuestToolsRuntimeUpdate::CommandResult {
            request_id,
            capability,
            ok,
            error_code,
            message,
            result,
            metadata: command_metadata,
            completed_at_unix,
        } => {
            metadata.last_command_result = Some(GuestToolsCommandResultMetadata {
                request_id,
                capability,
                ok,
                error_code,
                message,
                result,
                metadata: command_metadata,
                completed_at_unix,
            });
        }
        GuestToolsRuntimeUpdate::AgentUpdateAvailable {
            current_version,
            available_version,
            download_url,
            signature,
        } => {
            metadata.agent_update = Some(GuestToolsAgentUpdateMetadata {
                current_version,
                available_version,
                download_url,
                signature,
                observed_at_unix: now,
            });
        }
        GuestToolsRuntimeUpdate::Clipboard { text } => {
            metadata.clipboard = Some(GuestToolsClipboardMetadata {
                text,
                updated_at_unix: now,
            });
        }
    }

    store
        .write_guest_tools_runtime_metadata(name, &metadata)
        .context("failed to write guest tools runtime metadata")
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bridgevm_agent_protocol::{
        AgentAuth, AgentCapability, AgentEnvelope, AgentMessage, GuestIpAddress, PROTOCOL_VERSION,
    };
    use bridgevm_agentd::encode_envelope_line;
    use bridgevm_config::{BootMode, Guest, SharedFolder, VmManifest, VmMode};
    use std::io::Read;
    use std::net::TcpListener;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::thread::JoinHandle;

    static TEST_ID: AtomicU64 = AtomicU64::new(0);

    fn temp_store() -> VmStore {
        let mut path = PathBuf::from("/tmp");
        let id = TEST_ID.fetch_add(1, Ordering::Relaxed);
        path.push(format!("bvmd-{}-{}", std::process::id(), id));
        VmStore::new(path)
    }

    fn compatibility_manifest(name: &str) -> VmManifest {
        VmManifest::new(
            name,
            VmMode::Compatibility,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "x86_64".to_string(),
            },
            "64GiB",
        )
    }

    fn fast_manifest(name: &str) -> VmManifest {
        VmManifest::new(
            name,
            VmMode::Fast,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "arm64".to_string(),
            },
            "80GiB",
        )
    }

    fn ready_fast_manifest(name: &str) -> VmManifest {
        let mut manifest = fast_manifest(name);
        manifest.storage.primary.path = "disks/root.raw".to_string();
        manifest.storage.primary.format = "raw".to_string();
        manifest.boot = Some(bridgevm_config::Boot {
            mode: BootMode::LinuxKernel,
            installer_image: None,
            kernel_path: Some("boot/vmlinuz".to_string()),
            initrd_path: None,
            kernel_command_line: Some("console=hvc0 root=/dev/vda rw".to_string()),
            macos_restore_image: None,
        });
        manifest
    }

    fn write_executable(path: &Path, contents: &str) {
        fs::write(path, contents).unwrap();
        let mut permissions = fs::metadata(path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).unwrap();
    }

    fn daemon_request(store: VmStore, request: BridgeVmRequest) -> BridgeVmResponse {
        let (mut client, server) = UnixStream::pair().unwrap();
        serde_json::to_writer(&mut client, &request).unwrap();
        client.write_all(b"\n").unwrap();

        let mut state = DaemonState::new(store);
        handle_connection(&mut state, server).unwrap();

        let mut line = String::new();
        BufReader::new(client).read_line(&mut line).unwrap();
        serde_json::from_str(line.trim_end()).unwrap()
    }

    #[test]
    fn bind_daemon_listener_refuses_live_socket() {
        let store = temp_store();
        let socket_path = store.root().join("run").join("bridgevmd.sock");
        fs::create_dir_all(socket_path.parent().unwrap()).unwrap();
        let _live_listener = UnixListener::bind(&socket_path).unwrap();

        let error = bind_daemon_listener(&socket_path).unwrap_err();

        assert!(error.to_string().contains("already in use"));
        assert!(UnixStream::connect(&socket_path).is_ok());
    }

    #[test]
    fn bind_daemon_listener_refuses_non_socket_path() {
        let store = temp_store();
        let socket_path = store.root().join("run").join("bridgevmd.sock");
        fs::create_dir_all(socket_path.parent().unwrap()).unwrap();
        fs::write(&socket_path, "not a socket").unwrap();

        let error = bind_daemon_listener(&socket_path).unwrap_err();

        assert!(error.to_string().contains("not a socket"));
        assert_eq!(fs::read_to_string(&socket_path).unwrap(), "not a socket");
    }

    #[test]
    fn daemon_connection_lists_vms_with_swift_dashboard_wire_shape() {
        let store = temp_store();
        store.create_vm(&compatibility_manifest("legacy")).unwrap();

        let response = daemon_request(store.clone(), BridgeVmRequest::ListVms);
        let BridgeVmResponse::VmList { vms } = response else {
            panic!("expected VM list response");
        };
        assert_eq!(vms.len(), 1);
        assert_eq!(vms[0].name, "legacy");
        assert_eq!(vms[0].mode, "compatibility");
        assert_eq!(vms[0].guest_os, "ubuntu");
        assert_eq!(vms[0].guest_arch, "x86_64");
        assert_eq!(vms[0].state, "stopped");
        assert!(vms[0].path.ends_with("vms/legacy.vmbridge"));

        let json = serde_json::to_string(&BridgeVmResponse::VmList { vms }).unwrap();
        assert!(json.contains(r#""type":"vm_list""#));
        assert!(json.contains(r#""guest_os":"ubuntu""#));
        assert!(json.contains(r#""guest_arch":"x86_64""#));
    }

    #[test]
    fn daemon_connection_creates_vm_from_dashboard_manifest_shape() {
        let store = temp_store();
        let mut manifest = VmManifest::new(
            "Ubuntu Daily",
            VmMode::Fast,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "arm64".to_string(),
            },
            "80GiB",
        );
        manifest.boot = Some(bridgevm_config::Boot {
            mode: BootMode::LinuxInstaller,
            installer_image: Some("installers/ubuntu-arm64.iso".to_string()),
            kernel_path: None,
            initrd_path: None,
            kernel_command_line: None,
            macos_restore_image: None,
        });

        let response = daemon_request(
            store.clone(),
            BridgeVmRequest::CreateVm {
                manifest: manifest.clone(),
            },
        );
        let BridgeVmResponse::Vm { vm } = response else {
            panic!("expected VM create response");
        };
        assert_eq!(vm.name, "Ubuntu Daily");
        assert_eq!(vm.mode, "fast");
        assert_eq!(vm.guest_os, "ubuntu");
        assert_eq!(vm.guest_arch, "arm64");
        assert_eq!(vm.state, "stopped");

        let (_, stored) = store.get_vm("Ubuntu Daily").unwrap();
        assert_eq!(stored.network.hostname, "ubuntu-daily.bridgevm.local");
        assert_eq!(
            stored
                .boot
                .as_ref()
                .and_then(|boot| boot.installer_image.as_deref()),
            Some("installers/ubuntu-arm64.iso")
        );

        let response = daemon_request(store, BridgeVmRequest::ListVms);
        let BridgeVmResponse::VmList { vms } = response else {
            panic!("expected VM list response");
        };
        assert_eq!(vms.len(), 1);
        assert_eq!(vms[0].name, "Ubuntu Daily");
    }

    #[test]
    fn daemon_connection_lists_templates_for_dashboard_creation_flow() {
        let store = temp_store();

        let response = daemon_request(store, BridgeVmRequest::ListTemplates);
        let BridgeVmResponse::BootTemplates { templates } = response else {
            panic!("expected boot templates response");
        };

        let ubuntu = templates
            .iter()
            .find(|template| template.id == "ubuntu-arm64-installer")
            .expect("ubuntu arm64 installer template");
        assert_eq!(ubuntu.guest_os, "ubuntu");
        assert_eq!(ubuntu.guest_arch, "arm64");
        assert_eq!(ubuntu.mode, BootMode::LinuxInstaller);

        let json = serde_json::to_string(&BridgeVmResponse::BootTemplates { templates }).unwrap();
        assert!(json.contains(r#""type":"boot_templates""#));
        assert!(json.contains(r#""id":"ubuntu-arm64-installer""#));
        assert!(json.contains(r#""mode":"linux-installer""#));
    }

    #[test]
    fn daemon_connection_reports_boot_media_status_for_dashboard_detail() {
        let store = temp_store();
        let source = store.root().join("fixtures").join("ubuntu.iso");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::write(&source, b"fake installer").unwrap();

        let mut manifest = VmManifest::new(
            "ubuntu",
            VmMode::Fast,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "arm64".to_string(),
            },
            "80GiB",
        );
        manifest.boot = Some(bridgevm_config::Boot {
            mode: BootMode::LinuxInstaller,
            installer_image: Some("installers/ubuntu-arm64.iso".to_string()),
            kernel_path: None,
            initrd_path: None,
            kernel_command_line: None,
            macos_restore_image: None,
        });
        store.create_vm(&manifest).unwrap();

        let response = daemon_request(
            store.clone(),
            BridgeVmRequest::ImportBootMedia {
                name: "ubuntu".to_string(),
                source,
                kind: None,
            },
        );
        let BridgeVmResponse::BootMediaImported { import } = response else {
            panic!("expected boot media import response");
        };
        assert_eq!(import.vm, "ubuntu");
        assert_eq!(import.kind, bridgevm_api::BootMediaKind::InstallerImage);
        assert_eq!(import.bytes, 14);

        let response = daemon_request(
            store,
            BridgeVmRequest::InspectBootMediaStatus {
                name: "ubuntu".to_string(),
            },
        );
        let BridgeVmResponse::BootMediaStatus { status } = response else {
            panic!("expected boot media status response");
        };
        assert_eq!(status.vm, "ubuntu");
        assert_eq!(status.entries.len(), 1);
        let entry = &status.entries[0];
        assert_eq!(entry.kind, bridgevm_api::BootMediaKind::InstallerImage);
        assert!(entry.path.ends_with("installers/ubuntu-arm64.iso"));
        assert!(entry.exists);
        assert_eq!(entry.bytes, Some(14));
        assert_eq!(entry.last_import.as_ref().unwrap().bytes, 14);

        let json = serde_json::to_string(&BridgeVmResponse::BootMediaStatus { status }).unwrap();
        assert!(json.contains(r#""type":"boot_media_status""#));
        assert!(json.contains(r#""kind":"installer-image""#));
        assert!(json.contains(r#""bytes":14"#));
        assert!(!json.contains("size_bytes"));
    }

    #[test]
    fn daemon_connection_imports_boot_media_for_dashboard_detail() {
        let store = temp_store();
        let source = store.root().join("fixtures").join("ubuntu.iso");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::write(&source, b"fake installer").unwrap();

        let mut manifest = VmManifest::new(
            "ubuntu",
            VmMode::Fast,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "arm64".to_string(),
            },
            "80GiB",
        );
        manifest.boot = Some(bridgevm_config::Boot {
            mode: BootMode::LinuxInstaller,
            installer_image: Some("installers/ubuntu-arm64.iso".to_string()),
            kernel_path: None,
            initrd_path: None,
            kernel_command_line: None,
            macos_restore_image: None,
        });
        store.create_vm(&manifest).unwrap();

        let response = daemon_request(
            store.clone(),
            BridgeVmRequest::ImportBootMedia {
                name: "ubuntu".to_string(),
                source: source.clone(),
                kind: Some(bridgevm_api::BootMediaKind::InstallerImage),
            },
        );
        let BridgeVmResponse::BootMediaImported { import } = response else {
            panic!("expected boot media import response");
        };
        assert_eq!(import.vm, "ubuntu");
        assert_eq!(import.kind, bridgevm_api::BootMediaKind::InstallerImage);
        assert_eq!(import.source, source);
        assert!(import.destination.ends_with("installers/ubuntu-arm64.iso"));
        assert_eq!(import.bytes, 14);
        assert!(!import.replaced);
        assert_eq!(fs::read(&import.destination).unwrap(), b"fake installer");

        let json = serde_json::to_string(&BridgeVmResponse::BootMediaImported { import }).unwrap();
        assert!(json.contains(r#""type":"boot_media_imported""#));
        assert!(json.contains(r#""kind":"installer-image""#));
        assert!(json.contains(r#""bytes":14"#));

        let response = daemon_request(
            store,
            BridgeVmRequest::InspectBootMediaStatus {
                name: "ubuntu".to_string(),
            },
        );
        let BridgeVmResponse::BootMediaStatus { status } = response else {
            panic!("expected boot media status response");
        };
        let entry = status.entries.first().expect("boot media entry");
        assert!(entry.exists);
        assert_eq!(entry.bytes, Some(14));
        assert_eq!(entry.last_import.as_ref().unwrap().bytes, 14);
    }

    #[test]
    fn daemon_connection_verifies_and_plans_boot_media_download_for_dashboard_detail() {
        let store = temp_store();
        let source = store.root().join("fixtures").join("ubuntu.iso");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::write(&source, b"fake installer").unwrap();

        let mut manifest = VmManifest::new(
            "ubuntu",
            VmMode::Fast,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "arm64".to_string(),
            },
            "80GiB",
        );
        manifest.boot = Some(bridgevm_config::Boot {
            mode: BootMode::LinuxInstaller,
            installer_image: Some("installers/ubuntu-arm64.iso".to_string()),
            kernel_path: None,
            initrd_path: None,
            kernel_command_line: None,
            macos_restore_image: None,
        });
        store.create_vm(&manifest).unwrap();

        let response = daemon_request(
            store.clone(),
            BridgeVmRequest::ImportBootMedia {
                name: "ubuntu".to_string(),
                source,
                kind: Some(bridgevm_api::BootMediaKind::InstallerImage),
            },
        );
        let BridgeVmResponse::BootMediaImported { import: _ } = response else {
            panic!("expected boot media import response");
        };
        let expected_sha256 =
            "941ef2fd249e8e3535908e3663515a85a291c538016f75be86032da473029b3e".to_string();

        let response = daemon_request(
            store.clone(),
            BridgeVmRequest::VerifyBootMedia {
                name: "ubuntu".to_string(),
                expected_sha256: expected_sha256.clone(),
                kind: Some(bridgevm_api::BootMediaKind::InstallerImage),
            },
        );
        let BridgeVmResponse::BootMediaVerified { verification } = response else {
            panic!("expected boot media verification response");
        };
        assert_eq!(verification.vm, "ubuntu");
        assert_eq!(
            verification.kind,
            bridgevm_api::BootMediaKind::InstallerImage
        );
        assert_eq!(verification.expected_sha256, expected_sha256);
        assert_eq!(verification.actual_sha256, expected_sha256);
        assert!(verification.verified);
        assert_eq!(verification.bytes, 14);

        let json =
            serde_json::to_string(&BridgeVmResponse::BootMediaVerified { verification }).unwrap();
        assert!(json.contains(r#""type":"boot_media_verified""#));
        assert!(json.contains(r#""kind":"installer-image""#));
        assert!(json.contains(r#""verified":true"#));

        let download_body = b"downloaded installer";
        let downloaded_sha256 =
            "462fbe30bef6a4c53bf4aa9514ec72707270a518e9f98b4aa348432a4fc9fc3c".to_string();
        let (download_url, server) = serve_one_http_response(download_body);

        let response = daemon_request(
            store.clone(),
            BridgeVmRequest::PlanBootMediaDownload {
                name: "ubuntu".to_string(),
                url: download_url.clone(),
                expected_sha256: Some(downloaded_sha256.clone()),
                kind: Some(bridgevm_api::BootMediaKind::InstallerImage),
            },
        );
        let BridgeVmResponse::BootMediaDownloadPlanned { plan } = response else {
            panic!("expected boot media download plan response");
        };
        assert_eq!(plan.vm, "ubuntu");
        assert_eq!(plan.kind, bridgevm_api::BootMediaKind::InstallerImage);
        assert_eq!(plan.url, download_url);
        assert_eq!(
            plan.expected_sha256.as_deref(),
            Some(downloaded_sha256.as_str())
        );
        assert!(plan.exists);
        assert_eq!(plan.bytes, Some(14));
        assert!(plan.last_import.is_some());
        assert!(plan.last_verification.is_some());

        let json =
            serde_json::to_string(&BridgeVmResponse::BootMediaDownloadPlanned { plan }).unwrap();
        assert!(json.contains(r#""type":"boot_media_download_planned""#));
        assert!(json.contains(r#""kind":"installer-image""#));
        assert!(json.contains(r#""planned_at_unix""#));

        let response = daemon_request(
            store.clone(),
            BridgeVmRequest::DownloadBootMedia {
                name: "ubuntu".to_string(),
                kind: Some(bridgevm_api::BootMediaKind::InstallerImage),
            },
        );
        server.join().expect("http test server should finish");
        let BridgeVmResponse::BootMediaDownloaded { download } = response else {
            panic!("expected boot media downloaded response");
        };
        assert_eq!(download.vm, "ubuntu");
        assert_eq!(download.kind, bridgevm_api::BootMediaKind::InstallerImage);
        assert!(download.replaced);
        assert_eq!(download.bytes, Some(download_body.len() as u64));
        assert_eq!(
            download.actual_sha256.as_deref(),
            Some(downloaded_sha256.as_str())
        );
        assert_eq!(download.verified, Some(true));
        assert!(download.downloaded);

        let response = daemon_request(
            store,
            BridgeVmRequest::InspectBootMediaStatus {
                name: "ubuntu".to_string(),
            },
        );
        let BridgeVmResponse::BootMediaStatus { status } = response else {
            panic!("expected boot media status response");
        };
        let entry = status.entries.first().expect("boot media entry");
        assert!(entry.last_verification.as_ref().unwrap().verified);
        assert!(entry.last_download.as_ref().unwrap().downloaded);
        assert_eq!(entry.last_download_plan.as_ref().unwrap().url, download_url);
    }

    fn serve_one_http_response(body: &'static [u8]) -> (String, JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind local test server");
        let address = listener
            .local_addr()
            .expect("read local test server address");
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept curl connection");
            let mut request = [0_u8; 1024];
            let _ = stream.read(&mut request);
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .expect("write http response headers");
            stream.write_all(body).expect("write http response body");
        });
        (format!("http://{address}/ubuntu.iso"), handle)
    }

    #[test]
    fn daemon_connection_returns_network_planner_errors() {
        let store = temp_store();
        store.create_vm(&compatibility_manifest("legacy")).unwrap();

        let response = daemon_request(
            store.clone(),
            BridgeVmRequest::AddPort {
                name: "legacy".to_string(),
                host: 0,
                guest: 22,
            },
        );
        let BridgeVmResponse::Error { message } = response else {
            panic!("expected network planner error");
        };
        assert!(message.contains("invalid port forward 0:22"));

        let response = daemon_request(
            store,
            BridgeVmRequest::ListPorts {
                name: "legacy".to_string(),
            },
        );
        let BridgeVmResponse::PortForwards { ports } = response else {
            panic!("expected port forwards response");
        };
        assert!(ports.forwards.is_empty());
    }

    #[test]
    fn daemon_qemu_error_message_preserves_network_blocker_requirement() {
        let message = compatibility_qemu_command_error(QemuError::UnsupportedNetworkRequirement {
            mode: "advanced".to_string(),
            blocker: "qemu-advanced-network-unimplemented".to_string(),
            requirement:
                "Compatibility Mode QEMU requires an advanced network schema and launcher wiring before launch"
                    .to_string(),
        });

        assert!(
            message.contains("failed to build Compatibility Mode QEMU command"),
            "{message}"
        );
        assert!(
            message.contains("QEMU launch blocker qemu-advanced-network-unimplemented"),
            "{message}"
        );
        assert!(
            message.contains("requirement: Compatibility Mode QEMU requires an advanced network schema and launcher wiring before launch"),
            "{message}"
        );
    }

    #[test]
    fn daemon_fast_spawn_error_updates_runner_metadata_with_blocker() {
        let store = temp_store();
        store.create_vm(&fast_manifest("fast-linux")).unwrap();

        let response = daemon_request(
            store.clone(),
            BridgeVmRequest::RunBackend {
                name: "fast-linux".to_string(),
                spawn: true,
            },
        );
        let BridgeVmResponse::Error { message } = response else {
            panic!("expected Fast Mode spawn error");
        };
        assert!(
            message.contains("Fast Mode spawn is not implemented yet"),
            "{message}"
        );
        assert!(message.contains("launch blockers:"), "{message}");
        assert!(message.contains("missing-primary-disk"), "{message}");
        assert!(
            message.contains("fast-mode-spawn-unimplemented"),
            "{message}"
        );

        let metadata = store
            .runner_metadata("fast-linux")
            .unwrap()
            .expect("Fast spawn blocker writes dry-run runner metadata");
        assert!(metadata.dry_run);
        assert_eq!(metadata.engine, "lightvm");
        let readiness = metadata
            .launch_readiness
            .expect("Fast Mode runner metadata includes launch readiness");
        assert!(!readiness.ready);
        assert!(readiness
            .blockers
            .iter()
            .any(|blocker| blocker.code == "fast-mode-spawn-unimplemented"));
    }

    #[test]
    fn bundled_helper_discovery_uses_executable_siblings() {
        let store = temp_store();
        let helpers = store.root().join("BridgeVM.app/Contents/Helpers");
        fs::create_dir_all(&helpers).unwrap();
        let bridgevmd = helpers.join("bridgevmd");
        let apple_vz_runner = helpers.join("AppleVzRunner");
        write_executable(&bridgevmd, "#!/bin/sh\n");
        write_executable(&apple_vz_runner, "#!/bin/sh\n");

        assert_eq!(
            bundled_helper_path_from_exe(&bridgevmd, "AppleVzRunner"),
            Some(apple_vz_runner)
        );

        fs::remove_dir_all(store.root()).unwrap();
    }

    #[test]
    fn bundled_helper_discovery_rejects_non_executable_siblings() {
        let store = temp_store();
        let helpers = store.root().join("BridgeVM.app/Contents/Helpers");
        fs::create_dir_all(&helpers).unwrap();
        let bridgevmd = helpers.join("bridgevmd");
        let apple_vz_runner = helpers.join("AppleVzRunner");
        write_executable(&bridgevmd, "#!/bin/sh\n");
        fs::write(&apple_vz_runner, b"not executable").unwrap();

        assert_eq!(
            bundled_helper_path_from_exe(&bridgevmd, "AppleVzRunner"),
            None
        );

        fs::remove_dir_all(store.root()).unwrap();
    }

    #[test]
    fn fast_spawn_config_validate_rejects_non_executable_apple_vz_runner() {
        let store = temp_store();
        fs::create_dir_all(store.root()).unwrap();
        let lightvm_runner = store.root().join("fake-lightvm-runner");
        let apple_vz_runner = store.root().join("fake-AppleVzRunner");
        write_executable(&lightvm_runner, "#!/bin/sh\n");
        fs::write(&apple_vz_runner, b"not executable").unwrap();

        let error = FastModeSpawnConfig {
            lightvm_runner,
            apple_vz_runner: apple_vz_runner.clone(),
            stop_after_seconds: None,
            force_stop_grace_seconds: None,
            verify_apple_vz_runner_entitlement: false,
        }
        .validate()
        .unwrap_err();
        let message = format!("{error:#}");

        assert!(
            message.contains("BRIDGEVM_APPLE_VZ_RUNNER/AppleVzRunner is not executable"),
            "{message}"
        );
        assert!(message.contains(&apple_vz_runner.display().to_string()));

        fs::remove_dir_all(store.root()).unwrap();
    }

    #[test]
    fn entitlement_plist_requires_virtualization_true_value() {
        let true_plist = r#"
            <plist version="1.0">
            <dict>
              <key>com.apple.security.virtualization</key>
              <true/>
            </dict>
            </plist>
        "#;
        let false_plist = r#"
            <plist version="1.0">
            <dict>
              <key>com.apple.security.virtualization</key>
              <false/>
            </dict>
            </plist>
        "#;
        let missing_plist = r#"
            <plist version="1.0">
            <dict>
              <key>com.apple.security.app-sandbox</key>
              <true/>
            </dict>
            </plist>
        "#;

        assert!(entitlement_plist_has_true(
            true_plist,
            "com.apple.security.virtualization"
        ));
        assert!(!entitlement_plist_has_true(
            false_plist,
            "com.apple.security.virtualization"
        ));
        assert!(!entitlement_plist_has_true(
            missing_plist,
            "com.apple.security.virtualization"
        ));
    }

    #[test]
    fn daemon_fast_spawn_preflight_failure_does_not_mutate_runtime_state() {
        let store = temp_store();
        store.create_vm(&ready_fast_manifest("fast-linux")).unwrap();
        let bundle = store.bundle_path("fast-linux");
        fs::create_dir_all(bundle.join("boot")).unwrap();
        fs::write(bundle.join("boot").join("vmlinuz"), b"kernel").unwrap();

        let lightvm_runner = store.root().join("fake-lightvm-runner");
        let apple_vz_runner = store.root().join("fake-AppleVzRunner");
        write_executable(&lightvm_runner, "#!/bin/sh\n");
        fs::write(&apple_vz_runner, b"not executable").unwrap();

        let mut state = DaemonState::new(store.clone());
        let error = state
            .spawn_fast_backend(
                "fast-linux",
                bundle.clone(),
                ready_fast_manifest("fast-linux"),
                FastModeSpawnConfig {
                    lightvm_runner,
                    apple_vz_runner: apple_vz_runner.clone(),
                    stop_after_seconds: None,
                    force_stop_grace_seconds: None,
                    verify_apple_vz_runner_entitlement: false,
                },
            )
            .unwrap_err();
        let message = format!("{error:#}");

        assert!(
            message.contains("BRIDGEVM_APPLE_VZ_RUNNER/AppleVzRunner is not executable"),
            "{message}"
        );
        assert_eq!(
            store.state("fast-linux").unwrap().state,
            VmRuntimeState::Stopped
        );
        assert_eq!(store.runner_metadata("fast-linux").unwrap(), None);
        assert!(!state.children.contains_key("fast-linux"));
        assert!(
            !bundle.join("disks").join("root.raw").exists(),
            "preflight should fail before preparing the active disk"
        );

        fs::remove_dir_all(store.root()).unwrap();
    }

    #[test]
    fn daemon_fast_spawn_opt_in_supervises_lightvm_runner_child() {
        let store = temp_store();
        store.create_vm(&ready_fast_manifest("fast-linux")).unwrap();
        let bundle = store.bundle_path("fast-linux");
        fs::create_dir_all(bundle.join("boot")).unwrap();
        fs::create_dir_all(bundle.join("disks")).unwrap();
        fs::write(bundle.join("boot").join("vmlinuz"), b"kernel").unwrap();
        fs::write(bundle.join("disks").join("root.raw"), b"disk").unwrap();

        let lightvm_runner = store.root().join("fake-lightvm-runner");
        let argv_log = store.root().join("fake-lightvm-argv.txt");
        let apple_vz_runner = store.root().join("fake-AppleVzRunner");
        write_executable(
            &lightvm_runner,
            &format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\nsleep 2\n",
                argv_log.display()
            ),
        );
        write_executable(&apple_vz_runner, "#!/bin/sh\ncat >/dev/null\n");

        let mut state = DaemonState::new(store.clone());
        let response = state.spawn_fast_backend(
            "fast-linux",
            bundle.clone(),
            ready_fast_manifest("fast-linux"),
            FastModeSpawnConfig {
                lightvm_runner: lightvm_runner.clone(),
                apple_vz_runner: apple_vz_runner.clone(),
                stop_after_seconds: Some(5),
                force_stop_grace_seconds: Some(1),
                verify_apple_vz_runner_entitlement: false,
            },
        );
        let BridgeVmResponse::RunnerStatus {
            metadata: Some(metadata),
            ..
        } = response.unwrap()
        else {
            panic!("expected supervised Fast Mode runner metadata");
        };

        assert_eq!(metadata.engine, "lightvm");
        assert!(!metadata.dry_run);
        assert!(metadata.pid.is_some());
        assert_eq!(
            metadata.command.first().unwrap(),
            &lightvm_runner.display().to_string()
        );
        assert!(metadata.command.contains(&"--launch".to_string()));
        assert!(metadata.command.contains(&"--require-ready".to_string()));
        assert!(metadata
            .command
            .contains(&"--apple-vz-allow-real-start".to_string()));
        assert!(metadata
            .command
            .contains(&apple_vz_runner.display().to_string()));
        let expected_launch_spec = bundle.join("metadata").join("apple-vz-launch.json");
        assert_eq!(
            metadata.launch_spec_path.as_ref(),
            Some(&expected_launch_spec)
        );
        assert_eq!(
            store.state("fast-linux").unwrap().state,
            VmRuntimeState::Running
        );
        assert!(state.children.contains_key("fast-linux"));

        state.cleanup_owned_backend("fast-linux", false).unwrap();
        assert_eq!(store.runner_metadata("fast-linux").unwrap(), None);
    }

    #[test]
    fn daemon_fast_spawn_immediate_exit_reconcile_clears_runtime_state() {
        let store = temp_store();
        store.create_vm(&ready_fast_manifest("fast-linux")).unwrap();
        let bundle = store.bundle_path("fast-linux");
        fs::create_dir_all(bundle.join("boot")).unwrap();
        fs::create_dir_all(bundle.join("disks")).unwrap();
        fs::write(bundle.join("boot").join("vmlinuz"), b"kernel").unwrap();
        fs::write(bundle.join("disks").join("root.raw"), b"disk").unwrap();

        let lightvm_runner = store.root().join("fake-lightvm-runner");
        let apple_vz_runner = store.root().join("fake-AppleVzRunner");
        write_executable(&lightvm_runner, "#!/bin/sh\necho fast-fail >&2\nexit 7\n");
        write_executable(&apple_vz_runner, "#!/bin/sh\ncat >/dev/null\n");

        let mut state = DaemonState::new(store.clone());
        let response = state
            .spawn_fast_backend(
                "fast-linux",
                bundle.clone(),
                ready_fast_manifest("fast-linux"),
                FastModeSpawnConfig {
                    lightvm_runner,
                    apple_vz_runner,
                    stop_after_seconds: None,
                    force_stop_grace_seconds: None,
                    verify_apple_vz_runner_entitlement: false,
                },
            )
            .unwrap();
        let BridgeVmResponse::RunnerStatus {
            metadata: Some(metadata),
            ..
        } = response
        else {
            panic!("expected supervised Fast Mode runner metadata");
        };

        assert_eq!(
            store.state("fast-linux").unwrap().state,
            VmRuntimeState::Running
        );
        assert!(state.children.contains_key("fast-linux"));

        for _ in 0..120 {
            state.reconcile_children().unwrap();
            if !state.children.contains_key("fast-linux") {
                break;
            }
            thread::sleep(Duration::from_millis(25));
        }

        assert!(!state.children.contains_key("fast-linux"));
        assert_eq!(
            store.state("fast-linux").unwrap().state,
            VmRuntimeState::Stopped
        );
        assert_eq!(store.runner_metadata("fast-linux").unwrap(), None);
        assert!(
            fs::read_to_string(metadata.log_path)
                .unwrap()
                .contains("fast-fail"),
            "runner stderr should be captured in the Fast Mode log"
        );

        fs::remove_dir_all(store.root()).unwrap();
    }

    #[test]
    fn daemon_connection_creates_redacted_diagnostic_bundle() {
        let store = temp_store();
        store.create_vm(&compatibility_manifest("legacy")).unwrap();
        let token = store.guest_tools_token("legacy").unwrap().token;
        let bundle_path = store.bundle_path("legacy");
        fs::write(
            bundle_path.join("logs").join("qemu.log"),
            format!("guest tools token {token}\n"),
        )
        .unwrap();
        fs::write(
            bundle_path.join("metadata").join("download.json"),
            r#"{"url":"https://example.invalid/image.iso?signature=secret"}"#,
        )
        .unwrap();

        let output = store.root().join("daemon-diagnostics");
        let request = BridgeVmRequest::CreateDiagnosticBundle {
            name: "legacy".to_string(),
            output,
        };
        let response = daemon_request(store.clone(), request);
        let BridgeVmResponse::DiagnosticBundle { bundle } = response else {
            panic!("expected diagnostic bundle response");
        };

        assert!(bundle.output.exists());
        assert!(bundle.files.contains(&PathBuf::from("manifest.yaml")));
        assert!(bundle.files.contains(&PathBuf::from("logs/qemu.log")));
        assert!(bundle
            .files
            .contains(&PathBuf::from("metadata/download.json")));
        assert!(bundle
            .files
            .contains(&PathBuf::from("diagnostic-bundle.json")));

        let log = fs::read_to_string(bundle.output.join("logs").join("qemu.log")).unwrap();
        assert!(!log.contains(&token));
        assert!(log.contains("<redacted>"));
        let download =
            fs::read_to_string(bundle.output.join("metadata").join("download.json")).unwrap();
        assert!(!download.contains("signature=secret"));
        assert!(download.contains("https://example.invalid/image.iso?<redacted>"));
    }

    #[test]
    fn daemon_connection_creates_performance_sample() {
        let store = temp_store();
        store.create_vm(&compatibility_manifest("legacy")).unwrap();

        let output = store.root().join("daemon-performance");
        let request = BridgeVmRequest::CreatePerformanceSample {
            name: "legacy".to_string(),
            output,
            artifact_bytes: Some(1024),
            iterations: Some(2),
            sync: false,
        };
        let response = daemon_request(store, request);
        let BridgeVmResponse::PerformanceSample { sample } = response else {
            panic!("expected performance sample response");
        };

        assert!(sample.output.exists());
        assert!(sample.artifact.exists());
        assert_eq!(sample.artifact_bytes, 1024);
        assert_eq!(sample.iterations, 2);
        assert_eq!(sample.probes.len(), 2);
        assert!(sample
            .measurements
            .iter()
            .any(
                |measurement| measurement.name == "host_artifact_write_total_bytes"
                    && measurement.value == 2048
                    && !measurement.metadata_only
            ));
    }

    #[test]
    fn daemon_performance_sample_runs_guest_benchmark_when_session_is_connected() {
        let store = temp_store();
        store.create_vm(&compatibility_manifest("legacy")).unwrap();
        store
            .transition_state("legacy", VmRuntimeState::Running)
            .unwrap();

        let token = store.guest_tools_token("legacy").unwrap().token;
        let guest_tools = store.guest_tools_runner_metadata("legacy").unwrap();
        let listener = UnixListener::bind(&guest_tools.socket_path).unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let hello = AgentEnvelope::new(AgentMessage::GuestHello {
                version: PROTOCOL_VERSION,
                guest_os: "linux".to_string(),
                agent_version: Some("1.0.0".to_string()),
                capabilities: vec![
                    AgentCapability {
                        name: "heartbeat".to_string(),
                        version: 1,
                    },
                    AgentCapability {
                        name: "benchmark".to_string(),
                        version: 1,
                    },
                ],
                auth: Some(AgentAuth::ToolsToken { token }),
            });
            stream
                .write_all(encode_envelope_line(&hello).unwrap().as_bytes())
                .unwrap();

            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut command_line = String::new();
            reader.read_line(&mut command_line).unwrap();
            let command: AgentEnvelope = serde_json::from_str(command_line.trim_end()).unwrap();
            assert!(command
                .request_id
                .as_deref()
                .unwrap()
                .starts_with("performance-sample:"));
            assert_eq!(
                command.message,
                AgentMessage::RunBenchmark {
                    duration_millis: Some(DEFAULT_BENCHMARK_DURATION_MILLIS)
                }
            );

            let result = AgentEnvelope::new(AgentMessage::CommandResult {
                request_id: command.request_id.unwrap(),
                ok: true,
                error_code: None,
                message: Some("benchmark complete".to_string()),
                result: Some(serde_json::json!({
                    "requested_duration_millis": DEFAULT_BENCHMARK_DURATION_MILLIS,
                    "budget_duration_millis": DEFAULT_BENCHMARK_DURATION_MILLIS,
                    "cpu": {
                        "iterations": 4096,
                        "elapsed_millis": 1000,
                        "ops_per_sec": 4096,
                        "checksum": 12345
                    },
                    "disk": {
                        "bytes_written": 4096,
                        "elapsed_millis": 2,
                        "mib_per_sec": 25
                    }
                })),
                metadata: None,
            });
            stream
                .write_all(encode_envelope_line(&result).unwrap().as_bytes())
                .unwrap();
            thread::sleep(Duration::from_millis(250));
        });

        let child = Command::new("sh").arg("-c").arg("sleep 5").spawn().unwrap();
        let mut state = DaemonState::new(store.clone());
        state
            .children
            .insert("legacy".to_string(), SupervisedBackend::new(child));

        let output = store.root().join("daemon-performance-with-benchmark");
        let response = state
            .handle_request(BridgeVmRequest::CreatePerformanceSample {
                name: "legacy".to_string(),
                output,
                artifact_bytes: Some(1024),
                iterations: Some(1),
                sync: false,
            })
            .into_result()
            .unwrap();
        let BridgeVmResponse::PerformanceSample { sample } = response else {
            panic!("expected performance sample response");
        };

        assert!(sample
            .notes
            .iter()
            .any(|note| note.contains("guest benchmark executed")));
        assert!(!sample
            .notes
            .iter()
            .any(|note| note.contains("no guest benchmark workloads")));
        assert!(sample.measurements.iter().any(|measurement| {
            measurement.name == "guest_benchmark_cpu_iterations"
                && measurement.value == 4096
                && !measurement.metadata_only
        }));
        assert!(sample.measurements.iter().any(|measurement| {
            measurement.name == "guest_benchmark_disk_bytes_written"
                && measurement.value == 4096
                && !measurement.metadata_only
        }));
        let artifact = fs::read_to_string(&sample.artifact).unwrap();
        assert!(artifact.contains("guest_benchmark_cpu_ops_per_sec"));
        let runtime = sample
            .guest_tools
            .runtime
            .expect("refreshed guest tools runtime");
        assert_eq!(
            runtime.last_command_result.unwrap().capability.as_deref(),
            Some("benchmark")
        );

        state.cleanup_owned_backend("legacy", false).unwrap();
        server.join().unwrap();
    }

    #[test]
    fn reconcile_children_clears_exited_backend_state() {
        let store = temp_store();
        store.create_vm(&compatibility_manifest("legacy")).unwrap();
        store
            .transition_state("legacy", VmRuntimeState::Running)
            .unwrap();
        store
            .write_runner_metadata(
                "legacy",
                &RunnerMetadata {
                    engine: "fullvm".to_string(),
                    pid: Some(0),
                    command: vec!["sh".to_string(), "-c".to_string(), "exit 0".to_string()],
                    log_path: store.bundle_path("legacy").join("logs").join("qemu.log"),
                    started_at_unix: now_unix(),
                    dry_run: false,
                    launch_spec_path: None,
                    guest_tools: None,
                    disk: None,
                    active_disk: None,
                    launch_readiness: None,
                },
            )
            .unwrap();

        let child = Command::new("sh").arg("-c").arg("exit 0").spawn().unwrap();
        let mut state = DaemonState::new(store.clone());
        state
            .children
            .insert("legacy".to_string(), SupervisedBackend::new(child));

        for _ in 0..40 {
            state.reconcile_children().unwrap();
            if state.children.is_empty() {
                break;
            }
            thread::sleep(Duration::from_millis(25));
        }

        assert!(state.children.is_empty());
        assert_eq!(
            store.state("legacy").unwrap().state,
            VmRuntimeState::Stopped
        );
        assert_eq!(store.runner_metadata("legacy").unwrap(), None);
    }

    #[test]
    fn cleanup_owned_backend_clears_already_exited_child_state() {
        let store = temp_store();
        store.create_vm(&compatibility_manifest("legacy")).unwrap();
        store
            .transition_state("legacy", VmRuntimeState::Running)
            .unwrap();
        store
            .write_runner_metadata(
                "legacy",
                &RunnerMetadata {
                    engine: "fullvm".to_string(),
                    pid: Some(0),
                    command: vec!["sh".to_string(), "-c".to_string(), "exit 0".to_string()],
                    log_path: store.bundle_path("legacy").join("logs").join("qemu.log"),
                    started_at_unix: now_unix(),
                    dry_run: false,
                    launch_spec_path: None,
                    guest_tools: None,
                    disk: None,
                    active_disk: None,
                    launch_readiness: None,
                },
            )
            .unwrap();

        let child = Command::new("sh").arg("-c").arg("exit 0").spawn().unwrap();
        let mut state = DaemonState::new(store.clone());
        state
            .children
            .insert("legacy".to_string(), SupervisedBackend::new(child));

        for _ in 0..40 {
            if state
                .children
                .get_mut("legacy")
                .unwrap()
                .child
                .try_wait()
                .unwrap()
                .is_some()
            {
                break;
            }
            thread::sleep(Duration::from_millis(25));
        }

        state.cleanup_owned_backend("legacy", false).unwrap();

        assert!(state.children.is_empty());
        assert_eq!(
            store.state("legacy").unwrap().state,
            VmRuntimeState::Stopped
        );
        assert_eq!(store.runner_metadata("legacy").unwrap(), None);
    }

    #[test]
    fn daemon_routes_compat_resume_to_supervised_handler() {
        // Without a suspend marker the supervised compat resume reports the
        // marker error. This proves the daemon routes ResumeBackend through the
        // supervised path (the generic api fallback would have produced the
        // same marker error only via resume_compatibility_backend, never the
        // legacy "not wired yet" message).
        let store = temp_store();
        store.create_vm(&compatibility_manifest("legacy")).unwrap();
        store
            .transition_state("legacy", VmRuntimeState::Running)
            .unwrap();

        let mut state = DaemonState::new(store.clone());
        let error = state.resume_backend_supervised("legacy").unwrap_err();
        let message = format!("{error:#}");
        assert!(
            message.contains("no saved Compatibility Mode state to resume from"),
            "{message}"
        );
        assert!(!state.children.contains_key("legacy"));
        fs::remove_dir_all(store.root()).unwrap();
    }

    #[test]
    fn daemon_routes_fast_resume_to_supervised_handler() {
        // Fast resume with no saved state and no real-start env reports the Fast
        // state-missing error, proving the request reached the supervised Fast
        // resume branch (not the compat branch and not "not wired yet").
        let store = temp_store();
        store.create_vm(&fast_manifest("fast-linux")).unwrap();
        store
            .transition_state("fast-linux", VmRuntimeState::Running)
            .unwrap();

        let mut state = DaemonState::new(store.clone());
        let error = state.resume_backend_supervised("fast-linux").unwrap_err();
        let message = format!("{error:#}");
        assert!(
            message.contains("no saved Fast Mode state to resume from"),
            "{message}"
        );
        assert!(!state.children.contains_key("fast-linux"));
        fs::remove_dir_all(store.root()).unwrap();
    }

    #[test]
    fn reconcile_children_cleans_up_terminal_qmp_event() {
        let store = temp_store();
        store.create_vm(&compatibility_manifest("legacy")).unwrap();
        store
            .transition_state("legacy", VmRuntimeState::Running)
            .unwrap();
        store
            .write_runner_metadata(
                "legacy",
                &RunnerMetadata {
                    engine: "fullvm".to_string(),
                    pid: Some(0),
                    command: vec!["sh".to_string(), "-c".to_string(), "sleep 5".to_string()],
                    log_path: store.bundle_path("legacy").join("logs").join("qemu.log"),
                    started_at_unix: now_unix(),
                    dry_run: false,
                    launch_spec_path: None,
                    guest_tools: None,
                    disk: None,
                    active_disk: None,
                    launch_readiness: None,
                },
            )
            .unwrap();

        let (bundle, _) = store.get_vm("legacy").unwrap();
        let socket_path = qmp_socket_path(&bundle);
        let listener = UnixListener::bind(&socket_path).unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .write_all(br#"{"QMP":{"version":{"qemu":{"major":8,"minor":2,"micro":0}}}}"#)
                .unwrap();
            stream.write_all(b"\n").unwrap();

            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            assert!(line.contains("qmp_capabilities"));
            stream.write_all(br#"{"return":{}}"#).unwrap();
            stream.write_all(b"\n").unwrap();

            stream
                .write_all(br#"{"event":"BLOCK_JOB_COMPLETED","data":{"device":"drive0"}}"#)
                .unwrap();
            stream.write_all(b"\n").unwrap();
            stream
                .write_all(br#"{"event":"SHUTDOWN","data":{"guest":true}}"#)
                .unwrap();
            stream.write_all(b"\n").unwrap();
        });

        let child = Command::new("sh").arg("-c").arg("sleep 5").spawn().unwrap();
        let mut state = DaemonState::new(store.clone());
        state
            .children
            .insert("legacy".to_string(), SupervisedBackend::new(child));

        state.reconcile_children().unwrap();

        assert!(state.children.is_empty());
        assert_eq!(
            store.state("legacy").unwrap().state,
            VmRuntimeState::Stopped
        );
        assert_eq!(store.runner_metadata("legacy").unwrap(), None);
        let qmp = store
            .qmp_supervisor_metadata("legacy")
            .unwrap()
            .expect("qmp supervisor metadata");
        assert_eq!(qmp.envelopes_read, 2);
        assert_eq!(
            qmp.events
                .iter()
                .map(|event| event.name.as_str())
                .collect::<Vec<_>>(),
            ["BLOCK_JOB_COMPLETED", "SHUTDOWN"]
        );
        assert_eq!(qmp.terminal_event.as_ref().unwrap().name, "SHUTDOWN");
        assert_eq!(
            qmp.terminal_event.as_ref().unwrap().data.as_ref().unwrap(),
            &serde_json::json!({"guest": true})
        );
        assert!(!qmp.limit_reached);
        server.join().unwrap();
    }

    #[test]
    fn reconcile_children_records_nonterminal_qmp_events_without_cleanup() {
        let store = temp_store();
        store.create_vm(&compatibility_manifest("legacy")).unwrap();
        store
            .transition_state("legacy", VmRuntimeState::Running)
            .unwrap();
        store
            .write_runner_metadata(
                "legacy",
                &RunnerMetadata {
                    engine: "fullvm".to_string(),
                    pid: Some(0),
                    command: vec!["sh".to_string(), "-c".to_string(), "sleep 5".to_string()],
                    log_path: store.bundle_path("legacy").join("logs").join("qemu.log"),
                    started_at_unix: now_unix(),
                    dry_run: false,
                    launch_spec_path: None,
                    guest_tools: None,
                    disk: None,
                    active_disk: None,
                    launch_readiness: None,
                },
            )
            .unwrap();

        let (bundle, _) = store.get_vm("legacy").unwrap();
        let socket_path = qmp_socket_path(&bundle);
        let listener = UnixListener::bind(&socket_path).unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .write_all(br#"{"QMP":{"version":{"qemu":{"major":8,"minor":2,"micro":0}}}}"#)
                .unwrap();
            stream.write_all(b"\n").unwrap();

            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            assert!(line.contains("qmp_capabilities"));
            stream.write_all(br#"{"return":{}}"#).unwrap();
            stream.write_all(b"\n").unwrap();
            stream
                .write_all(br#"{"event":"RESUME","data":{"status":"running"}}"#)
                .unwrap();
            stream.write_all(b"\n").unwrap();
            thread::sleep(Duration::from_millis(100));
        });

        let child = Command::new("sh").arg("-c").arg("sleep 5").spawn().unwrap();
        let mut state = DaemonState::new(store.clone());
        state
            .children
            .insert("legacy".to_string(), SupervisedBackend::new(child));

        state.reconcile_children().unwrap();

        assert!(state.children.contains_key("legacy"));
        assert_eq!(
            store.state("legacy").unwrap().state,
            VmRuntimeState::Running
        );
        let qmp = store
            .qmp_supervisor_metadata("legacy")
            .unwrap()
            .expect("qmp supervisor metadata");
        assert_eq!(qmp.envelopes_read, 1);
        assert_eq!(qmp.events.len(), 1);
        assert_eq!(qmp.events[0].name, "RESUME");
        assert_eq!(
            qmp.events[0].data.as_ref().unwrap(),
            &serde_json::json!({"status": "running"})
        );
        assert!(qmp.terminal_event.is_none());
        assert!(!qmp.limit_reached);

        state.cleanup_owned_backend("legacy", false).unwrap();
        server.join().unwrap();
    }

    #[test]
    fn reconcile_children_records_qmp_drain_limit_metadata() {
        let store = temp_store();
        store.create_vm(&compatibility_manifest("legacy")).unwrap();
        store
            .transition_state("legacy", VmRuntimeState::Running)
            .unwrap();
        store
            .write_runner_metadata(
                "legacy",
                &RunnerMetadata {
                    engine: "fullvm".to_string(),
                    pid: Some(0),
                    command: vec!["sh".to_string(), "-c".to_string(), "sleep 5".to_string()],
                    log_path: store.bundle_path("legacy").join("logs").join("qemu.log"),
                    started_at_unix: now_unix(),
                    dry_run: false,
                    launch_spec_path: None,
                    guest_tools: None,
                    disk: None,
                    active_disk: None,
                    launch_readiness: None,
                },
            )
            .unwrap();

        let (bundle, _) = store.get_vm("legacy").unwrap();
        let socket_path = qmp_socket_path(&bundle);
        let listener = UnixListener::bind(&socket_path).unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .write_all(br#"{"QMP":{"version":{"qemu":{"major":8,"minor":2,"micro":0}}}}"#)
                .unwrap();
            stream.write_all(b"\n").unwrap();

            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            assert!(line.contains("qmp_capabilities"));
            stream.write_all(br#"{"return":{}}"#).unwrap();
            stream.write_all(b"\n").unwrap();

            for seq in 0..QMP_SUPERVISOR_DRAIN_LIMIT {
                writeln!(stream, r#"{{"event":"RESUME","data":{{"seq":{seq}}}}}"#).unwrap();
            }
            thread::sleep(Duration::from_millis(100));
        });

        let child = Command::new("sh").arg("-c").arg("sleep 5").spawn().unwrap();
        let mut state = DaemonState::new(store.clone());
        state
            .children
            .insert("legacy".to_string(), SupervisedBackend::new(child));

        state.reconcile_children().unwrap();

        assert!(state.children.contains_key("legacy"));
        assert_eq!(
            store.state("legacy").unwrap().state,
            VmRuntimeState::Running
        );
        let qmp = store
            .qmp_supervisor_metadata("legacy")
            .unwrap()
            .expect("qmp supervisor metadata");
        assert_eq!(qmp.envelopes_read, QMP_SUPERVISOR_DRAIN_LIMIT);
        assert_eq!(qmp.events.len(), QMP_SUPERVISOR_DRAIN_LIMIT);
        assert_eq!(qmp.events.first().unwrap().name, "RESUME");
        assert_eq!(
            qmp.events
                .first()
                .unwrap()
                .data
                .as_ref()
                .and_then(|data| data.get("seq"))
                .and_then(|seq| seq.as_u64()),
            Some(0)
        );
        assert_eq!(
            qmp.events
                .last()
                .unwrap()
                .data
                .as_ref()
                .and_then(|data| data.get("seq"))
                .and_then(|seq| seq.as_u64()),
            Some((QMP_SUPERVISOR_DRAIN_LIMIT - 1) as u64)
        );
        assert!(qmp.terminal_event.is_none());
        assert!(qmp.limit_reached);

        state.cleanup_owned_backend("legacy", false).unwrap();
        server.join().unwrap();
    }

    #[test]
    fn reconcile_children_bootstraps_guest_tools_session() {
        let store = temp_store();
        store.create_vm(&compatibility_manifest("legacy")).unwrap();
        store
            .transition_state("legacy", VmRuntimeState::Running)
            .unwrap();

        let token = store.guest_tools_token("legacy").unwrap().token;
        let guest_tools = store.guest_tools_runner_metadata("legacy").unwrap();
        let listener = UnixListener::bind(&guest_tools.socket_path).unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let envelope = AgentEnvelope::new(AgentMessage::GuestHello {
                version: PROTOCOL_VERSION,
                guest_os: "linux".to_string(),
                agent_version: Some("1.0.0".to_string()),
                capabilities: vec![
                    AgentCapability {
                        name: "heartbeat".to_string(),
                        version: 1,
                    },
                    AgentCapability {
                        name: "guest-ip".to_string(),
                        version: 1,
                    },
                    AgentCapability {
                        name: "guest-metrics".to_string(),
                        version: 1,
                    },
                    AgentCapability {
                        name: "clipboard".to_string(),
                        version: 1,
                    },
                ],
                auth: Some(AgentAuth::ToolsToken { token }),
            });
            let line = encode_envelope_line(&envelope).unwrap();
            stream.write_all(line.as_bytes()).unwrap();
            stream
                .write_all(
                    encode_envelope_line(&AgentEnvelope::new(AgentMessage::Heartbeat))
                        .unwrap()
                        .as_bytes(),
                )
                .unwrap();
            stream
                .write_all(
                    encode_envelope_line(&AgentEnvelope::new(AgentMessage::GuestIpChanged {
                        addresses: vec![GuestIpAddress {
                            address: "10.0.2.15".parse().unwrap(),
                            interface: Some("eth0".to_string()),
                        }],
                    }))
                    .unwrap()
                    .as_bytes(),
                )
                .unwrap();
            stream
                .write_all(
                    encode_envelope_line(&AgentEnvelope::new(AgentMessage::ClipboardChanged {
                        text: "first guest value".to_string(),
                    }))
                    .unwrap()
                    .as_bytes(),
                )
                .unwrap();
            stream
                .write_all(
                    encode_envelope_line(&AgentEnvelope::new(AgentMessage::ClipboardChanged {
                        text: "latest guest value".to_string(),
                    }))
                    .unwrap()
                    .as_bytes(),
                )
                .unwrap();
            stream
                .write_all(
                    encode_envelope_line(&AgentEnvelope::new(AgentMessage::GuestMetrics {
                        cpu_percent: 17,
                        memory_used_mib: 512,
                    }))
                    .unwrap()
                    .as_bytes(),
                )
                .unwrap();
            thread::sleep(Duration::from_millis(250));
        });

        let child = Command::new("sh").arg("-c").arg("sleep 5").spawn().unwrap();
        let mut state = DaemonState::new(store.clone());
        state
            .children
            .insert("legacy".to_string(), SupervisedBackend::new(child));

        state.reconcile_children().unwrap();

        let backend = state.children.get("legacy").unwrap();
        let session = backend.guest_tools.as_ref().expect("guest tools session");
        assert_eq!(session.guest_os, "linux");
        assert_eq!(session.agent_version.as_deref(), Some("1.0.0"));
        assert!(session.supports("heartbeat"));
        assert!(session.supports("guest-ip"));
        assert!(session.supports("guest-metrics"));
        assert!(session.supports("clipboard"));

        let runtime = store
            .guest_tools_runtime_metadata("legacy")
            .unwrap()
            .expect("runtime metadata");
        assert!(runtime.connected);
        assert_eq!(runtime.guest_os.as_deref(), Some("linux"));
        assert!(runtime.last_heartbeat_at_unix.is_some());
        assert_eq!(runtime.guest_ip_addresses.len(), 1);
        assert_eq!(runtime.guest_ip_addresses[0].address, "10.0.2.15");
        assert_eq!(
            runtime.guest_ip_addresses[0].interface.as_deref(),
            Some("eth0")
        );
        let metrics = runtime.metrics.expect("guest metrics");
        assert_eq!(metrics.cpu_percent, 17);
        assert_eq!(metrics.memory_used_mib, 512);
        let clipboard = runtime.clipboard.expect("clipboard metadata");
        assert_eq!(clipboard.text, "latest guest value");
        assert!(clipboard.updated_at_unix > 0);

        state.cleanup_owned_backend("legacy", false).unwrap();
        server.join().unwrap();
    }

    #[test]
    fn reconcile_holds_connection_and_catches_delayed_guest_hello() {
        // Regression guard for the live application-consistent path: the guest
        // agent emits its GuestHello exactly once, as the first frame, a beat
        // after the host connects. The daemon must connect host-first and HOLD
        // that connection across reconcile ticks so it catches the delayed
        // hello, instead of reconnecting each tick and racing past it.
        let store = temp_store();
        store.create_vm(&compatibility_manifest("legacy")).unwrap();
        store
            .transition_state("legacy", VmRuntimeState::Running)
            .unwrap();

        let token = store.guest_tools_token("legacy").unwrap().token;
        let guest_tools = store.guest_tools_runner_metadata("legacy").unwrap();
        let listener = UnixListener::bind(&guest_tools.socket_path).unwrap();

        let (send_hello, await_hello) = std::sync::mpsc::channel::<()>();
        let server = thread::spawn(move || {
            // Accept the daemon's host-first connection, then withhold the hello
            // until the test has run the first (pending, no-data) reconcile —
            // emulating the guest agent coming up a moment after the host
            // attaches. The hello must land on this SAME held connection.
            let (mut stream, _) = listener.accept().unwrap();
            await_hello.recv().unwrap();
            let hello = AgentEnvelope::new(AgentMessage::GuestHello {
                version: PROTOCOL_VERSION,
                guest_os: "linux".to_string(),
                agent_version: Some("1.0.0".to_string()),
                capabilities: vec![AgentCapability {
                    name: "heartbeat".to_string(),
                    version: 1,
                }],
                auth: Some(AgentAuth::ToolsToken { token }),
            });
            stream
                .write_all(encode_envelope_line(&hello).unwrap().as_bytes())
                .unwrap();
            thread::sleep(Duration::from_millis(250));
        });

        let child = Command::new("sh").arg("-c").arg("sleep 5").spawn().unwrap();
        let mut state = DaemonState::new(store.clone());
        state
            .children
            .insert("legacy".to_string(), SupervisedBackend::new(child));

        // First reconcile: connects host-first, reads no hello yet -> the
        // connection is HELD (pending), no session accepted, no reset.
        state.reconcile_children().unwrap();
        {
            let backend = state.children.get("legacy").unwrap();
            assert!(
                backend.guest_tools.is_none(),
                "no session should be accepted before the hello arrives"
            );
            assert!(
                backend.guest_tools_pending.is_some(),
                "the host-first connection must be held while waiting for the hello"
            );
        }

        // The agent now writes its one-shot hello on the SAME held connection.
        send_hello.send(()).unwrap();
        thread::sleep(Duration::from_millis(50));

        // Second reconcile: reads the delayed hello on the held connection and
        // accepts the session (proving the connection was not dropped/reconnected).
        state.reconcile_children().unwrap();
        {
            let backend = state.children.get("legacy").unwrap();
            let session = backend
                .guest_tools
                .as_ref()
                .expect("session accepted from the delayed hello on the held connection");
            assert_eq!(session.guest_os, "linux");
            assert!(session.supports("heartbeat"));
            assert!(
                backend.guest_tools_pending.is_none(),
                "the held connection should move to the active stream once accepted"
            );
        }
        let runtime = store
            .guest_tools_runtime_metadata("legacy")
            .unwrap()
            .expect("runtime metadata");
        assert!(runtime.connected);

        state.cleanup_owned_backend("legacy", false).unwrap();
        server.join().unwrap();
    }

    #[test]
    fn reconcile_reassembles_a_guest_hello_split_across_reads() {
        // The agent's one-shot GuestHello can arrive split across host reads
        // (virtio-serial chunks it), with a gap longer than the socket read
        // timeout. The held connection must NOT consume + lose the partial frame
        // when the timeout fires mid-frame -- it must reassemble and accept once
        // the whole line is present. (Guards the peek-before-consume fix.)
        let store = temp_store();
        store.create_vm(&compatibility_manifest("legacy")).unwrap();
        store
            .transition_state("legacy", VmRuntimeState::Running)
            .unwrap();

        let token = store.guest_tools_token("legacy").unwrap().token;
        let guest_tools = store.guest_tools_runner_metadata("legacy").unwrap();
        let listener = UnixListener::bind(&guest_tools.socket_path).unwrap();

        let (start_send, await_send) = std::sync::mpsc::channel::<()>();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            await_send.recv().unwrap();
            let hello = AgentEnvelope::new(AgentMessage::GuestHello {
                version: PROTOCOL_VERSION,
                guest_os: "linux".to_string(),
                agent_version: Some("1.0.0".to_string()),
                capabilities: vec![AgentCapability {
                    name: "heartbeat".to_string(),
                    version: 1,
                }],
                auth: Some(AgentAuth::ToolsToken { token }),
            });
            let line = encode_envelope_line(&hello).unwrap();
            let bytes = line.as_bytes();
            let mid = bytes.len() / 2;
            // First half, then a pause LONGER than the 25ms read timeout (so a
            // naive read would time out mid-frame), then the rest.
            stream.write_all(&bytes[..mid]).unwrap();
            stream.flush().unwrap();
            thread::sleep(Duration::from_millis(120));
            stream.write_all(&bytes[mid..]).unwrap();
            stream.flush().unwrap();
            thread::sleep(Duration::from_millis(250));
        });

        let child = Command::new("sh").arg("-c").arg("sleep 5").spawn().unwrap();
        let mut state = DaemonState::new(store.clone());
        state
            .children
            .insert("legacy".to_string(), SupervisedBackend::new(child));

        // First reconcile: connect host-first, nothing to read yet -> held.
        state.reconcile_children().unwrap();
        assert!(state
            .children
            .get("legacy")
            .unwrap()
            .guest_tools_pending
            .is_some());

        start_send.send(()).unwrap();

        // Poll reconcile until the split hello is reassembled + accepted. While
        // only the first half is buffered, the connection must stay held (the
        // partial frame must never be consumed/lost or the connection reset).
        let mut accepted = false;
        for _ in 0..40 {
            thread::sleep(Duration::from_millis(20));
            state.reconcile_children().unwrap();
            let backend = state.children.get("legacy").unwrap();
            if backend.guest_tools.is_some() {
                accepted = true;
                break;
            }
            assert!(
                backend.guest_tools_pending.is_some(),
                "the connection must be held while the frame is incomplete"
            );
        }
        assert!(
            accepted,
            "split GuestHello was never reassembled + accepted"
        );
        assert_eq!(
            state
                .children
                .get("legacy")
                .unwrap()
                .guest_tools
                .as_ref()
                .unwrap()
                .guest_os,
            "linux"
        );

        state.cleanup_owned_backend("legacy", false).unwrap();
        server.join().unwrap();
    }

    #[test]
    fn shutdown_reaps_supervised_children_so_none_orphan() {
        // Regression guard: killing bridgevmd must not leave its spawned QEMU /
        // AppleVzRunner children orphaned (still running, still holding ports).
        let store = temp_store();
        store.create_vm(&compatibility_manifest("legacy")).unwrap();
        store
            .transition_state("legacy", VmRuntimeState::Running)
            .unwrap();

        // A long-lived stand-in for a spawned backend process.
        let child = Command::new("sh")
            .arg("-c")
            .arg("sleep 60")
            .spawn()
            .unwrap();
        let pid = child.id() as libc::pid_t;
        let mut state = DaemonState::new(store.clone());
        state
            .children
            .insert("legacy".to_string(), SupervisedBackend::new(child));

        // Sanity: the child is alive before shutdown.
        assert_eq!(
            unsafe { libc::kill(pid, 0) },
            0,
            "the supervised child should be alive before shutdown"
        );

        state.shutdown_reap_children();

        assert!(
            !state.children.contains_key("legacy"),
            "the supervised child must be removed from the daemon on shutdown"
        );
        // The spawned process must be reaped (SIGKILL + wait), not orphaned.
        let mut gone = false;
        for _ in 0..40 {
            if unsafe { libc::kill(pid, 0) } == -1 {
                gone = true;
                break;
            }
            thread::sleep(Duration::from_millis(50));
        }
        assert!(
            gone,
            "the supervised child must be killed on shutdown, not left orphaned"
        );
        assert_eq!(
            store.state("legacy").unwrap().state,
            VmRuntimeState::Stopped,
            "the VM should be marked Stopped after its backend is reaped"
        );
    }

    #[test]
    fn reconcile_children_records_agent_update_notice_as_runtime_metadata() {
        let store = temp_store();
        store.create_vm(&compatibility_manifest("legacy")).unwrap();
        store
            .transition_state("legacy", VmRuntimeState::Running)
            .unwrap();

        let token = store.guest_tools_token("legacy").unwrap().token;
        let guest_tools = store.guest_tools_runner_metadata("legacy").unwrap();
        let listener = UnixListener::bind(&guest_tools.socket_path).unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let hello = AgentEnvelope::new(AgentMessage::GuestHello {
                version: PROTOCOL_VERSION,
                guest_os: "linux".to_string(),
                agent_version: Some("1.0.0".to_string()),
                capabilities: vec![
                    AgentCapability {
                        name: "heartbeat".to_string(),
                        version: 1,
                    },
                    AgentCapability {
                        name: "agent-update".to_string(),
                        version: 1,
                    },
                ],
                auth: Some(AgentAuth::ToolsToken { token }),
            });
            stream
                .write_all(encode_envelope_line(&hello).unwrap().as_bytes())
                .unwrap();
            stream
                .write_all(
                    encode_envelope_line(&AgentEnvelope::new(AgentMessage::AgentUpdateAvailable {
                        current_version: "1.0.0".to_string(),
                        available_version: "1.1.0".to_string(),
                        download_url: Some("https://updates.example/bridgevm-tools".to_string()),
                        signature: Some("signature-bytes".to_string()),
                    }))
                    .unwrap()
                    .as_bytes(),
                )
                .unwrap();
            thread::sleep(Duration::from_millis(250));
        });

        let child = Command::new("sh").arg("-c").arg("sleep 5").spawn().unwrap();
        let mut state = DaemonState::new(store.clone());
        state
            .children
            .insert("legacy".to_string(), SupervisedBackend::new(child));

        state.reconcile_children().unwrap();

        let backend = state.children.get("legacy").unwrap();
        assert_eq!(backend.guest_tools_commands.pending_count(), 0);
        let runtime = store
            .guest_tools_runtime_metadata("legacy")
            .unwrap()
            .expect("runtime metadata");
        assert!(runtime.connected);
        assert!(runtime
            .capabilities
            .iter()
            .any(|name| name == "agent-update"));
        let update = runtime.agent_update.expect("agent update metadata");
        assert_eq!(update.current_version, "1.0.0");
        assert_eq!(update.available_version, "1.1.0");
        assert_eq!(
            update.download_url.as_deref(),
            Some("https://updates.example/bridgevm-tools")
        );
        assert_eq!(update.signature.as_deref(), Some("signature-bytes"));
        assert!(update.observed_at_unix > 0);

        state.cleanup_owned_backend("legacy", false).unwrap();
        server.join().unwrap();
    }

    #[test]
    fn daemon_sends_guest_tools_command_and_tracks_result() {
        let store = temp_store();
        let mut manifest = compatibility_manifest("legacy");
        manifest.shared_folders = vec![SharedFolder {
            name: "work".to_string(),
            host_path: "/Users/me/work".to_string(),
            read_only: false,
            host_path_token: Some("share-token-1".to_string()),
        }];
        store.create_vm(&manifest).unwrap();
        store
            .transition_state("legacy", VmRuntimeState::Running)
            .unwrap();

        let token = store.guest_tools_token("legacy").unwrap().token;
        let guest_tools = store.guest_tools_runner_metadata("legacy").unwrap();
        let listener = UnixListener::bind(&guest_tools.socket_path).unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let hello = AgentEnvelope::new(AgentMessage::GuestHello {
                version: PROTOCOL_VERSION,
                guest_os: "linux".to_string(),
                agent_version: Some("1.0.0".to_string()),
                capabilities: vec![
                    AgentCapability {
                        name: "heartbeat".to_string(),
                        version: 1,
                    },
                    AgentCapability {
                        name: "clipboard".to_string(),
                        version: 1,
                    },
                    AgentCapability {
                        name: "shared-folders".to_string(),
                        version: 1,
                    },
                ],
                auth: Some(AgentAuth::ToolsToken { token }),
            });
            stream
                .write_all(encode_envelope_line(&hello).unwrap().as_bytes())
                .unwrap();

            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut command_line = String::new();
            reader.read_line(&mut command_line).unwrap();
            let command: AgentEnvelope = serde_json::from_str(command_line.trim_end()).unwrap();
            assert_eq!(command.request_id.as_deref(), Some("clipboard-1"));
            assert_eq!(
                command.message,
                AgentMessage::SetClipboard {
                    text: "hello from host".to_string()
                }
            );

            let result = AgentEnvelope::new(AgentMessage::CommandResult {
                request_id: "clipboard-1".to_string(),
                ok: true,
                error_code: None,
                message: Some("clipboard accepted".to_string()),
                result: Some(serde_json::json!({
                    "text_length": 15,
                    "changed": true
                })),
                metadata: Some(serde_json::json!({
                    "handler": "clipboard",
                    "duration_ms": 3
                })),
            });
            stream
                .write_all(encode_envelope_line(&result).unwrap().as_bytes())
                .unwrap();

            let mut command_line = String::new();
            reader.read_line(&mut command_line).unwrap();
            let command: AgentEnvelope = serde_json::from_str(command_line.trim_end()).unwrap();
            assert_eq!(command.request_id.as_deref(), Some("mount-1"));
            assert_eq!(
                command.message,
                AgentMessage::MountShare {
                    name: "work".to_string(),
                    host_path_token: "share-token-1".to_string()
                }
            );

            let result = AgentEnvelope::new(AgentMessage::CommandResult {
                request_id: "mount-1".to_string(),
                ok: true,
                error_code: None,
                message: None,
                result: None,
                metadata: None,
            });
            stream
                .write_all(encode_envelope_line(&result).unwrap().as_bytes())
                .unwrap();
            thread::sleep(Duration::from_millis(250));
        });

        let child = Command::new("sh").arg("-c").arg("sleep 5").spawn().unwrap();
        let mut state = DaemonState::new(store.clone());
        state
            .children
            .insert("legacy".to_string(), SupervisedBackend::new(child));

        state.reconcile_children().unwrap();
        let command = AgentEnvelope::with_request_id(
            AgentMessage::SetClipboard {
                text: "hello from host".to_string(),
            },
            "clipboard-1",
        );
        let response = state
            .handle_request(BridgeVmRequest::GuestToolsSendCommand {
                name: "legacy".to_string(),
                envelope: command,
            })
            .into_result()
            .unwrap();
        let BridgeVmResponse::GuestToolsCommand { command } = response else {
            panic!("expected guest tools command response");
        };
        assert_eq!(command.request_id.as_deref(), Some("clipboard-1"));
        assert_eq!(command.pending_commands, 1);

        state.reconcile_children().unwrap();
        let backend = state.children.get("legacy").unwrap();
        assert_eq!(backend.guest_tools_commands.pending_count(), 0);
        let runtime = store
            .guest_tools_runtime_metadata("legacy")
            .unwrap()
            .expect("runtime metadata");
        let result = runtime.last_command_result.expect("last command result");
        assert_eq!(result.request_id, "clipboard-1");
        assert_eq!(result.capability.as_deref(), Some("clipboard"));
        assert!(result.ok);
        assert_eq!(result.message.as_deref(), Some("clipboard accepted"));
        assert_eq!(
            result.result,
            Some(serde_json::json!({
                "text_length": 15,
                "changed": true
            }))
        );
        assert_eq!(
            result.metadata,
            Some(serde_json::json!({
                "handler": "clipboard",
                "duration_ms": 3
            }))
        );

        let response = state
            .handle_request(BridgeVmRequest::GuestToolsMountApprovedShare {
                name: "legacy".to_string(),
                share: "work".to_string(),
                request_id: Some("mount-1".to_string()),
            })
            .into_result()
            .unwrap();
        let BridgeVmResponse::GuestToolsCommand { command } = response else {
            panic!("expected guest tools command response");
        };
        assert_eq!(command.request_id.as_deref(), Some("mount-1"));
        assert_eq!(command.pending_commands, 1);

        state.reconcile_children().unwrap();
        let backend = state.children.get("legacy").unwrap();
        assert_eq!(backend.guest_tools_commands.pending_count(), 0);
        let runtime = store
            .guest_tools_runtime_metadata("legacy")
            .unwrap()
            .expect("runtime metadata");
        assert_eq!(runtime.shared_folders.len(), 1);
        assert_eq!(runtime.shared_folders[0].name, "work");
        assert_eq!(runtime.shared_folders[0].host_path_token, "share-token-1");
        let result = runtime.last_command_result.expect("last command result");
        assert_eq!(result.request_id, "mount-1");
        assert_eq!(result.capability.as_deref(), Some("shared-folders"));
        assert!(result.ok);

        state.cleanup_owned_backend("legacy", false).unwrap();
        server.join().unwrap();
    }

    #[test]
    fn daemon_executes_application_consistent_snapshot_scaffold_commands() {
        let store = temp_store();
        store.create_vm(&compatibility_manifest("legacy")).unwrap();
        store
            .transition_state("legacy", VmRuntimeState::Running)
            .unwrap();

        let token = store.guest_tools_token("legacy").unwrap().token;
        let guest_tools = store.guest_tools_runner_metadata("legacy").unwrap();
        let listener = UnixListener::bind(&guest_tools.socket_path).unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let hello = AgentEnvelope::new(AgentMessage::GuestHello {
                version: PROTOCOL_VERSION,
                guest_os: "linux".to_string(),
                agent_version: Some("1.0.0".to_string()),
                capabilities: vec![
                    AgentCapability {
                        name: "heartbeat".to_string(),
                        version: 1,
                    },
                    AgentCapability {
                        name: "fs-freeze".to_string(),
                        version: 1,
                    },
                    AgentCapability {
                        name: "fs-thaw".to_string(),
                        version: 1,
                    },
                ],
                auth: Some(AgentAuth::ToolsToken { token }),
            });
            stream
                .write_all(encode_envelope_line(&hello).unwrap().as_bytes())
                .unwrap();

            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut freeze_line = String::new();
            reader.read_line(&mut freeze_line).unwrap();
            let freeze: AgentEnvelope = serde_json::from_str(freeze_line.trim_end()).unwrap();
            assert_eq!(
                freeze.request_id.as_deref(),
                Some("application-consistent-snapshot:before-upgrade:freeze")
            );
            assert_eq!(
                freeze.message,
                AgentMessage::FreezeFilesystem {
                    timeout_millis: Some(5_000),
                }
            );
            stream
                .write_all(
                    encode_envelope_line(&AgentEnvelope::new(AgentMessage::CommandResult {
                        request_id: "application-consistent-snapshot:before-upgrade:freeze"
                            .to_string(),
                        ok: true,
                        error_code: None,
                        message: Some("freeze scaffold acknowledged".to_string()),
                        result: None,
                        metadata: None,
                    }))
                    .unwrap()
                    .as_bytes(),
                )
                .unwrap();

            let mut thaw_line = String::new();
            reader.read_line(&mut thaw_line).unwrap();
            let thaw: AgentEnvelope = serde_json::from_str(thaw_line.trim_end()).unwrap();
            assert_eq!(
                thaw.request_id.as_deref(),
                Some("application-consistent-snapshot:before-upgrade:thaw")
            );
            assert_eq!(thaw.message, AgentMessage::ThawFilesystem);
            stream
                .write_all(
                    encode_envelope_line(&AgentEnvelope::new(AgentMessage::CommandResult {
                        request_id: "application-consistent-snapshot:before-upgrade:thaw"
                            .to_string(),
                        ok: true,
                        error_code: None,
                        message: Some("thaw scaffold acknowledged".to_string()),
                        result: None,
                        metadata: None,
                    }))
                    .unwrap()
                    .as_bytes(),
                )
                .unwrap();
            thread::sleep(Duration::from_millis(250));
        });

        let child = Command::new("sh").arg("-c").arg("sleep 5").spawn().unwrap();
        let mut state = DaemonState::new(store.clone());
        state
            .children
            .insert("legacy".to_string(), SupervisedBackend::new(child));

        state.reconcile_children().unwrap();
        let preflight = state
            .handle_request(BridgeVmRequest::SnapshotPreflightStatus {
                name: "legacy".to_string(),
                consistency: bridgevm_api::SnapshotConsistency::ApplicationConsistent,
            })
            .into_result()
            .unwrap();
        let BridgeVmResponse::SnapshotPreflightStatus { preflight } = preflight else {
            panic!("expected snapshot preflight response");
        };
        assert!(preflight.backend_freeze_thaw_supported);
        assert!(preflight.ready);

        let response = state
            .handle_request(BridgeVmRequest::ExecuteApplicationConsistentSnapshot {
                vm: "legacy".to_string(),
                name: "before-upgrade".to_string(),
                freeze_timeout_millis: Some(5_000),
            })
            .into_result()
            .unwrap();
        let BridgeVmResponse::ApplicationConsistentSnapshotExecution { execution } = response
        else {
            panic!("expected application-consistent snapshot execution response");
        };
        assert_eq!(execution.vm, "legacy");
        assert_eq!(execution.snapshot, "before-upgrade");
        assert_eq!(execution.pending_commands_after_freeze, 0);
        assert_eq!(execution.pending_commands_after_thaw, 0);
        assert_eq!(
            execution.freeze_result.capability.as_deref(),
            Some("fs-freeze")
        );
        assert!(execution.freeze_result.ok);
        assert_eq!(execution.thaw_result.capability.as_deref(), Some("fs-thaw"));
        assert!(execution.thaw_result.ok);

        let snapshots = store.snapshots("legacy").unwrap();
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].kind, SnapshotKind::ApplicationConsistent);

        state.cleanup_owned_backend("legacy", false).unwrap();
        server.join().unwrap();
    }

    #[test]
    fn daemon_thaws_after_application_consistent_snapshot_failure() {
        let store = temp_store();
        store.create_vm(&compatibility_manifest("legacy")).unwrap();
        store
            .transition_state("legacy", VmRuntimeState::Running)
            .unwrap();
        store
            .create_snapshot(
                "legacy",
                "duplicate",
                bridgevm_storage::SnapshotKind::ApplicationConsistent,
            )
            .unwrap();

        let token = store.guest_tools_token("legacy").unwrap().token;
        let guest_tools = store.guest_tools_runner_metadata("legacy").unwrap();
        let listener = UnixListener::bind(&guest_tools.socket_path).unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let hello = AgentEnvelope::new(AgentMessage::GuestHello {
                version: PROTOCOL_VERSION,
                guest_os: "linux".to_string(),
                agent_version: Some("1.0.0".to_string()),
                capabilities: vec![
                    AgentCapability {
                        name: "heartbeat".to_string(),
                        version: 1,
                    },
                    AgentCapability {
                        name: "fs-freeze".to_string(),
                        version: 1,
                    },
                    AgentCapability {
                        name: "fs-thaw".to_string(),
                        version: 1,
                    },
                ],
                auth: Some(AgentAuth::ToolsToken { token }),
            });
            stream
                .write_all(encode_envelope_line(&hello).unwrap().as_bytes())
                .unwrap();

            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut freeze_line = String::new();
            reader.read_line(&mut freeze_line).unwrap();
            let freeze: AgentEnvelope = serde_json::from_str(freeze_line.trim_end()).unwrap();
            assert_eq!(
                freeze.request_id.as_deref(),
                Some("application-consistent-snapshot:duplicate:freeze")
            );
            stream
                .write_all(
                    encode_envelope_line(&AgentEnvelope::new(AgentMessage::CommandResult {
                        request_id: "application-consistent-snapshot:duplicate:freeze".to_string(),
                        ok: true,
                        error_code: None,
                        message: Some("freeze scaffold acknowledged".to_string()),
                        result: None,
                        metadata: None,
                    }))
                    .unwrap()
                    .as_bytes(),
                )
                .unwrap();

            let mut thaw_line = String::new();
            reader.read_line(&mut thaw_line).unwrap();
            let thaw: AgentEnvelope = serde_json::from_str(thaw_line.trim_end()).unwrap();
            assert_eq!(
                thaw.request_id.as_deref(),
                Some("application-consistent-snapshot:duplicate:thaw")
            );
            assert_eq!(thaw.message, AgentMessage::ThawFilesystem);
            stream
                .write_all(
                    encode_envelope_line(&AgentEnvelope::new(AgentMessage::CommandResult {
                        request_id: "application-consistent-snapshot:duplicate:thaw".to_string(),
                        ok: true,
                        error_code: None,
                        message: Some("thaw scaffold acknowledged".to_string()),
                        result: None,
                        metadata: None,
                    }))
                    .unwrap()
                    .as_bytes(),
                )
                .unwrap();
            thread::sleep(Duration::from_millis(250));
        });

        let child = Command::new("sh").arg("-c").arg("sleep 5").spawn().unwrap();
        let mut state = DaemonState::new(store.clone());
        state
            .children
            .insert("legacy".to_string(), SupervisedBackend::new(child));

        state.reconcile_children().unwrap();
        let response =
            state.handle_request(BridgeVmRequest::ExecuteApplicationConsistentSnapshot {
                vm: "legacy".to_string(),
                name: "duplicate".to_string(),
                freeze_timeout_millis: Some(5_000),
            });
        let BridgeVmResponse::Error { message } = response else {
            panic!("expected duplicate snapshot error");
        };
        assert!(message.contains("failed to create application-consistent snapshot"));

        state.reconcile_children().unwrap();
        let runtime = store
            .guest_tools_runtime_metadata("legacy")
            .unwrap()
            .expect("runtime metadata");
        let result = runtime.last_command_result.expect("last command result");
        assert_eq!(
            result.request_id,
            "application-consistent-snapshot:duplicate:thaw"
        );
        assert_eq!(result.capability.as_deref(), Some("fs-thaw"));
        assert!(result.ok);

        state.cleanup_owned_backend("legacy", false).unwrap();
        server.join().unwrap();
    }

    #[test]
    fn shell_word_split_handles_quotes_and_escapes() {
        assert_eq!(
            shell_word_split("-drive file=/tmp/a b.iso,if=virtio,format=raw"),
            vec![
                "-drive".to_string(),
                "file=/tmp/a".to_string(),
                "b.iso,if=virtio,format=raw".to_string(),
            ]
        );
        assert_eq!(
            shell_word_split("-drive 'file=/tmp/with space.iso,if=virtio'"),
            vec![
                "-drive".to_string(),
                "file=/tmp/with space.iso,if=virtio".to_string(),
            ]
        );
        assert_eq!(
            shell_word_split("-drive \"file=/tmp/x.iso,id=cidata\""),
            vec![
                "-drive".to_string(),
                "file=/tmp/x.iso,id=cidata".to_string()
            ]
        );
        assert_eq!(
            shell_word_split("file=/tmp/a\\ b.iso"),
            vec!["file=/tmp/a b.iso".to_string()]
        );
        assert!(shell_word_split("   ").is_empty());
    }

    #[test]
    fn daemon_surfaces_thaw_failure_after_successful_snapshot() {
        // The snapshot succeeds and the freeze entered the boundary, but the
        // agent's thaw reply is ok:false. The orchestration must still have
        // DISPATCHED the thaw (the guest cannot be left frozen silently) and
        // then surface the thaw failure to the caller.
        let store = temp_store();
        store.create_vm(&compatibility_manifest("legacy")).unwrap();
        store
            .transition_state("legacy", VmRuntimeState::Running)
            .unwrap();

        let token = store.guest_tools_token("legacy").unwrap().token;
        let guest_tools = store.guest_tools_runner_metadata("legacy").unwrap();
        let listener = UnixListener::bind(&guest_tools.socket_path).unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let hello = AgentEnvelope::new(AgentMessage::GuestHello {
                version: PROTOCOL_VERSION,
                guest_os: "linux".to_string(),
                agent_version: Some("1.0.0".to_string()),
                capabilities: vec![
                    AgentCapability {
                        name: "heartbeat".to_string(),
                        version: 1,
                    },
                    AgentCapability {
                        name: "fs-freeze".to_string(),
                        version: 1,
                    },
                    AgentCapability {
                        name: "fs-thaw".to_string(),
                        version: 1,
                    },
                ],
                auth: Some(AgentAuth::ToolsToken { token }),
            });
            stream
                .write_all(encode_envelope_line(&hello).unwrap().as_bytes())
                .unwrap();

            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut freeze_line = String::new();
            reader.read_line(&mut freeze_line).unwrap();
            let freeze: AgentEnvelope = serde_json::from_str(freeze_line.trim_end()).unwrap();
            assert_eq!(
                freeze.request_id.as_deref(),
                Some("application-consistent-snapshot:after-thaw-fail:freeze")
            );
            assert_eq!(
                freeze.message,
                AgentMessage::FreezeFilesystem {
                    timeout_millis: Some(5_000),
                }
            );
            stream
                .write_all(
                    encode_envelope_line(&AgentEnvelope::new(AgentMessage::CommandResult {
                        request_id: "application-consistent-snapshot:after-thaw-fail:freeze"
                            .to_string(),
                        ok: true,
                        error_code: None,
                        message: Some("freeze acknowledged".to_string()),
                        result: None,
                        metadata: None,
                    }))
                    .unwrap()
                    .as_bytes(),
                )
                .unwrap();

            // The thaw MUST still be dispatched even after a successful
            // snapshot. Reply ok:false to assert the failure is surfaced.
            let mut thaw_line = String::new();
            reader.read_line(&mut thaw_line).unwrap();
            let thaw: AgentEnvelope = serde_json::from_str(thaw_line.trim_end()).unwrap();
            assert_eq!(
                thaw.request_id.as_deref(),
                Some("application-consistent-snapshot:after-thaw-fail:thaw")
            );
            assert_eq!(thaw.message, AgentMessage::ThawFilesystem);
            stream
                .write_all(
                    encode_envelope_line(&AgentEnvelope::new(AgentMessage::CommandResult {
                        request_id: "application-consistent-snapshot:after-thaw-fail:thaw"
                            .to_string(),
                        ok: false,
                        error_code: Some("filesystem-thaw-failed".to_string()),
                        message: Some("fsfreeze -u failed".to_string()),
                        result: None,
                        metadata: None,
                    }))
                    .unwrap()
                    .as_bytes(),
                )
                .unwrap();
            thread::sleep(Duration::from_millis(250));
        });

        let child = Command::new("sh").arg("-c").arg("sleep 5").spawn().unwrap();
        let mut state = DaemonState::new(store.clone());
        state
            .children
            .insert("legacy".to_string(), SupervisedBackend::new(child));

        state.reconcile_children().unwrap();
        let response =
            state.handle_request(BridgeVmRequest::ExecuteApplicationConsistentSnapshot {
                vm: "legacy".to_string(),
                name: "after-thaw-fail".to_string(),
                freeze_timeout_millis: Some(5_000),
            });
        let BridgeVmResponse::Error { message } = response else {
            panic!("expected thaw-failure error response");
        };
        assert!(
            message.contains("guest tools thaw failed"),
            "unexpected error: {message}"
        );

        // The snapshot was recorded (thaw failed only afterwards), and the thaw
        // command WAS dispatched + tracked as the last command result.
        let snapshots = store.snapshots("legacy").unwrap();
        assert_eq!(snapshots.len(), 1);
        let runtime = store
            .guest_tools_runtime_metadata("legacy")
            .unwrap()
            .expect("runtime metadata");
        let result = runtime.last_command_result.expect("last command result");
        assert_eq!(
            result.request_id,
            "application-consistent-snapshot:after-thaw-fail:thaw"
        );
        assert_eq!(result.capability.as_deref(), Some("fs-thaw"));
        assert!(!result.ok);

        state.cleanup_owned_backend("legacy", false).unwrap();
        server.join().unwrap();
    }
}
