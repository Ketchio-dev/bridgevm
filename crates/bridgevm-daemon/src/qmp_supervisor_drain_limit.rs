//! Split out of main.rs to keep files under 800 lines.

use crate::*;
use anyhow::Context;
use anyhow::Result;
use bridgevm_agentd::AgentCommandTracker;
use bridgevm_agentd::AgentSession;
use bridgevm_api::guest_tools_mount_approved_share_envelope;
use bridgevm_api::handle_request;
use bridgevm_api::BridgeVmRequest;
use bridgevm_api::BridgeVmResponse;
use bridgevm_qemu::QmpClient;
use bridgevm_storage::LaunchReadinessMetadata;
use bridgevm_storage::VmStore;
use clap::Parser;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::BufRead;
use std::io::BufReader;
use std::io::ErrorKind;
use std::io::Read;
use std::io::Write;
use std::os::unix::fs::FileTypeExt;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixListener;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::path::PathBuf;
use std::process::Child;
use std::process::Command;
use std::process::Output;
use std::process::Stdio;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use std::time::Instant;

pub(crate) const QMP_SUPERVISOR_DRAIN_LIMIT: usize = 16;
pub(crate) const GUEST_TOOLS_DRAIN_LIMIT: usize = 16;
pub(crate) const GUEST_TOOLS_COMMAND_RESULT_TIMEOUT: Duration = Duration::from_secs(5);
pub(crate) const MAX_DAEMON_FRAME_BYTES: u64 = 16 * 1024 * 1024;
pub(crate) const MAX_PROXY_FRAMEBUFFER_BYTES: usize = 256 * 1024 * 1024;
pub(crate) const DAEMON_CLIENT_IO_TIMEOUT: Duration = Duration::from_secs(2);
pub(crate) const DAEMON_RESPONSE_WAIT_TIMEOUT: Duration = Duration::from_secs(30);
pub(crate) const MAX_CONCURRENT_DAEMON_CLIENTS: usize = 32;
#[cfg(target_os = "macos")]
pub(crate) const CODESIGN_PREFLIGHT_TIMEOUT: Duration = Duration::from_secs(10);
#[cfg(target_os = "macos")]
pub(crate) const CODESIGN_PREFLIGHT_OUTPUT_LIMIT: usize = 1024 * 1024;

pub(crate) struct PendingDaemonRequest {
    pub(crate) request: BridgeVmRequest,
    pub(crate) response_sender: mpsc::Sender<BridgeVmResponse>,
}

/// Set by the SIGTERM/SIGINT handler so the supervisor loop can reap its
/// spawned QEMU/AppleVzRunner children before exiting. Without this, killing
/// `bridgevmd` (the common case: a service restart, or a test harness tearing
/// the daemon down) would leave its VM processes orphaned — still running and
/// still holding their ports (e.g. VNC :0 / TCP 5900) with no supervisor.
pub(crate) static SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);

extern "C" fn handle_shutdown_signal(_signal: libc::c_int) {
    // Async-signal-safe: only flips an atomic. The actual teardown happens in
    // the supervisor loop, which polls this flag.
    SHUTDOWN_REQUESTED.store(true, Ordering::SeqCst);
}

pub(crate) fn install_shutdown_handlers() {
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
pub(crate) struct Args {
    #[arg(long, value_name = "PATH")]
    pub(crate) store: Option<PathBuf>,
    #[arg(long, default_value = "bridgevmd.sock", value_name = "SOCKET")]
    pub(crate) socket_name: String,
    #[arg(long)]
    pub(crate) once: bool,
    #[arg(long, default_value_t = 250, value_name = "MILLIS")]
    pub(crate) reconcile_interval_ms: u64,
}

pub(crate) fn run() -> Result<()> {
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

pub(crate) fn serve(
    store: VmStore,
    socket_path: &Path,
    reconcile_interval: Duration,
) -> Result<()> {
    let listener = bind_daemon_listener(socket_path)?;
    listener
        .set_nonblocking(true)
        .context("failed to configure daemon socket")?;
    println!("bridgevmd listening");
    install_shutdown_handlers();
    let mut state = DaemonState::new(store);
    let mut last_reconcile = Instant::now();
    let (request_sender, request_receiver) = mpsc::channel::<PendingDaemonRequest>();
    let active_clients = Arc::new(AtomicUsize::new(0));

    loop {
        if SHUTDOWN_REQUESTED.load(Ordering::SeqCst) {
            println!("bridgevmd received shutdown signal; reaping supervised backends");
            state.shutdown_reap_children();
            println!("bridgevmd shutdown complete");
            return Ok(());
        }

        while let Ok(pending) = request_receiver.try_recv() {
            let response = state.handle_request(pending.request);
            let _ = pending.response_sender.send(response);
        }

        match listener.accept() {
            Ok(stream) => {
                spawn_connection_worker(
                    stream.0,
                    request_sender.clone(),
                    Arc::clone(&active_clients),
                );
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

pub(crate) fn spawn_connection_worker(
    stream: UnixStream,
    request_sender: mpsc::Sender<PendingDaemonRequest>,
    active_clients: Arc<AtomicUsize>,
) {
    if active_clients.fetch_add(1, Ordering::AcqRel) >= MAX_CONCURRENT_DAEMON_CLIENTS {
        active_clients.fetch_sub(1, Ordering::AcqRel);
        return;
    }
    let worker_clients = Arc::clone(&active_clients);
    let spawn_result = thread::Builder::new()
        .name("bridgevmd-client".to_string())
        .spawn(move || {
            if let Err(error) = run_connection_worker(stream, request_sender) {
                eprintln!("bridgevmd request failed: {error:#}");
            }
            worker_clients.fetch_sub(1, Ordering::AcqRel);
        });
    if let Err(error) = spawn_result {
        active_clients.fetch_sub(1, Ordering::AcqRel);
        eprintln!("bridgevmd failed to spawn client worker: {error}");
    }
}

pub(crate) fn run_connection_worker(
    mut stream: UnixStream,
    request_sender: mpsc::Sender<PendingDaemonRequest>,
) -> Result<()> {
    // The listener is nonblocking so the supervisor can keep reconciling
    // children and observe shutdown requests.  On macOS an accepted stream
    // can inherit O_NONBLOCK from that listener; restore blocking I/O before
    // applying finite timeouts so a client that has connected but has not yet
    // written its frame is not rejected with EAGAIN.
    stream
        .set_nonblocking(false)
        .context("failed to configure daemon client blocking mode")?;
    stream
        .set_read_timeout(Some(DAEMON_CLIENT_IO_TIMEOUT))
        .context("failed to configure daemon client read timeout")?;
    stream
        .set_write_timeout(Some(DAEMON_CLIENT_IO_TIMEOUT))
        .context("failed to configure daemon client write timeout")?;
    let request = read_daemon_request(&stream)?;
    let (response_sender, response_receiver) = mpsc::channel();
    request_sender
        .send(PendingDaemonRequest {
            request,
            response_sender,
        })
        .context("daemon supervisor stopped before handling request")?;
    let response = response_receiver
        .recv_timeout(DAEMON_RESPONSE_WAIT_TIMEOUT)
        .context("daemon supervisor did not return a response before timeout")?;
    write_daemon_response(&mut stream, &response)
}

pub(crate) fn bind_daemon_listener(socket_path: &Path) -> Result<UnixListener> {
    if let Some(parent) = socket_path.parent() {
        fs::create_dir_all(parent).context("failed to create daemon run directory")?;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))
            .context("failed to protect daemon run directory")?;
    }
    if socket_path.exists() {
        let metadata = match fs::symlink_metadata(socket_path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == ErrorKind::NotFound => {
                return bind_new_daemon_listener(socket_path);
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

    bind_new_daemon_listener(socket_path)
}

pub(crate) fn bind_new_daemon_listener(socket_path: &Path) -> Result<UnixListener> {
    let listener = UnixListener::bind(socket_path).context("failed to bind daemon socket")?;
    if let Err(error) = fs::set_permissions(socket_path, fs::Permissions::from_mode(0o600)) {
        drop(listener);
        let _ = fs::remove_file(socket_path);
        return Err(error).context("failed to protect daemon socket");
    }
    Ok(listener)
}

#[cfg(test)]
pub(crate) fn handle_connection(state: &mut DaemonState, mut stream: UnixStream) -> Result<()> {
    stream.set_read_timeout(Some(DAEMON_CLIENT_IO_TIMEOUT))?;
    stream.set_write_timeout(Some(DAEMON_CLIENT_IO_TIMEOUT))?;
    let request = read_daemon_request(&stream)?;
    let response = state.handle_request(request);
    write_daemon_response(&mut stream, &response)
}

pub(crate) fn read_daemon_request(stream: &UnixStream) -> Result<BridgeVmRequest> {
    let mut frame = Vec::new();
    BufReader::new(stream.try_clone()?)
        .take(MAX_DAEMON_FRAME_BYTES + 1)
        .read_until(b'\n', &mut frame)
        .context("failed to read daemon request")?;
    if frame.is_empty() {
        anyhow::bail!("daemon client sent an empty request");
    }
    if frame.len() as u64 > MAX_DAEMON_FRAME_BYTES {
        anyhow::bail!("daemon request exceeded {MAX_DAEMON_FRAME_BYTES} bytes");
    }
    if frame.last() != Some(&b'\n') {
        anyhow::bail!("daemon client sent an incomplete request frame");
    }
    serde_json::from_slice::<BridgeVmRequest>(&frame).context("invalid request JSON")
}

pub(crate) fn write_daemon_response(
    stream: &mut UnixStream,
    response: &BridgeVmResponse,
) -> Result<()> {
    let mut frame = serde_json::to_vec(response).context("failed to encode daemon response")?;
    frame.push(b'\n');
    if frame.len() as u64 > MAX_DAEMON_FRAME_BYTES {
        anyhow::bail!("daemon response exceeded {MAX_DAEMON_FRAME_BYTES} bytes");
    }
    stream
        .write_all(&frame)
        .context("failed to write daemon response")
}

pub(crate) struct DaemonState {
    pub(crate) store: VmStore,
    pub(crate) children: HashMap<String, SupervisedBackend>,
}

pub(crate) struct SupervisedBackend {
    pub(crate) child: Child,
    pub(crate) qmp: Option<QmpClient>,
    pub(crate) guest_tools: Option<AgentSession>,
    pub(crate) guest_tools_stream: Option<BufReader<UnixStream>>,
    /// A guest-tools socket connection established host-first (right after the
    /// backend is spawned, before the guest agent boots) and HELD open across
    /// reconcile ticks. The guest agent writes its `GuestHello` exactly once,
    /// as the first frame, when it comes up ~a minute into boot. Connecting
    /// fresh on each tick races past that one-shot hello (the daemon would read
    /// a later Heartbeat first -> `ExpectedGuestHello`), so instead we connect
    /// once and keep this reader until the hello arrives or the socket dies.
    pub(crate) guest_tools_pending: Option<UnixStream>,
    pub(crate) guest_tools_commands: AgentCommandTracker,
    pub(crate) proxy_window_crop_targets: HashMap<String, ProxyWindowCropTarget>,
    pub(crate) proxy_window_framebuffer_signature: Option<ProxyWindowFramebufferSignature>,
}

impl SupervisedBackend {
    pub(crate) fn new(child: Child) -> Self {
        Self {
            child,
            qmp: None,
            guest_tools: None,
            guest_tools_stream: None,
            guest_tools_pending: None,
            guest_tools_commands: AgentCommandTracker::new(),
            proxy_window_crop_targets: HashMap::new(),
            proxy_window_framebuffer_signature: None,
        }
    }
}

pub(crate) struct FastModeSpawnConfig {
    pub(crate) lightvm_runner: PathBuf,
    pub(crate) apple_vz_runner: PathBuf,
    pub(crate) stop_after_seconds: Option<u64>,
    pub(crate) force_stop_grace_seconds: Option<u64>,
    pub(crate) verify_apple_vz_runner_entitlement: bool,
}

impl FastModeSpawnConfig {
    pub(crate) fn from_env() -> Result<Option<Self>> {
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

    pub(crate) fn validate(&self) -> Result<()> {
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
    pub(crate) fn runner_args_with_restore(
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

pub(crate) fn bundled_helper_path(name: &str) -> Option<PathBuf> {
    let exe = env::current_exe().ok()?;
    bundled_helper_path_from_exe(&exe, name)
}

pub(crate) fn bundled_helper_path_from_exe(exe: &Path, name: &str) -> Option<PathBuf> {
    let helper = exe.parent()?.join(name);
    if helper.is_file() && is_executable(&helper) {
        Some(helper)
    } else {
        None
    }
}

pub(crate) fn is_executable(path: &Path) -> bool {
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

pub(crate) fn require_executable(path: &Path, label: &str) -> Result<()> {
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
pub(crate) fn verify_apple_vz_runner_entitlement(path: &Path) -> Result<()> {
    let mut command = Command::new("codesign");
    command.args(["-d", "--entitlements", ":-"]).arg(path);
    let output = run_bounded_command_output(
        command,
        CODESIGN_PREFLIGHT_TIMEOUT,
        CODESIGN_PREFLIGHT_OUTPUT_LIMIT,
    )
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

pub(crate) fn run_bounded_command_output(
    mut command: Command,
    timeout: Duration,
    output_limit: usize,
) -> std::io::Result<Output> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let stdout = match child.stdout.take() {
        Some(stdout) => stdout,
        None => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(std::io::Error::other("failed to capture command stdout"));
        }
    };
    let stderr = match child.stderr.take() {
        Some(stderr) => stderr,
        None => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(std::io::Error::other("failed to capture command stderr"));
        }
    };
    let stdout_thread = thread::spawn(move || drain_command_output(stdout, output_limit));
    let stderr_thread = thread::spawn(move || drain_command_output(stderr, output_limit));

    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < timeout => thread::sleep(Duration::from_millis(10)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_thread.join();
                let _ = stderr_thread.join();
                return Err(std::io::Error::new(
                    ErrorKind::TimedOut,
                    format!(
                        "command exceeded {}-millisecond timeout",
                        timeout.as_millis()
                    ),
                ));
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_thread.join();
                let _ = stderr_thread.join();
                return Err(error);
            }
        }
    };

    let (stdout, stdout_exceeded) = join_command_output(stdout_thread, "stdout")?;
    let (stderr, stderr_exceeded) = join_command_output(stderr_thread, "stderr")?;
    if stdout_exceeded || stderr_exceeded {
        return Err(std::io::Error::new(
            ErrorKind::InvalidData,
            format!("command output exceeded {output_limit}-byte per-stream limit"),
        ));
    }
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

pub(crate) fn drain_command_output<R: Read>(
    mut stream: R,
    limit: usize,
) -> std::io::Result<(Vec<u8>, bool)> {
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

pub(crate) fn join_command_output(
    handle: thread::JoinHandle<std::io::Result<(Vec<u8>, bool)>>,
    name: &str,
) -> std::io::Result<(Vec<u8>, bool)> {
    handle
        .join()
        .map_err(|_| std::io::Error::other(format!("command {name} drain thread panicked")))?
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn verify_apple_vz_runner_entitlement(_path: &Path) -> Result<()> {
    Ok(())
}

pub(crate) fn entitlement_plist_has_true(plist: &str, key: &str) -> bool {
    let key_tag = format!("<key>{key}</key>");
    let Some(after_key) = plist.split_once(&key_tag).map(|(_, after)| after) else {
        return false;
    };
    let value = after_key.trim_start();
    value.starts_with("<true/>") || value.starts_with("<true />")
}

pub(crate) fn env_flag_enabled(name: &str) -> bool {
    env::var(name)
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
}

/// Test-only extra QEMU args for daemon-spawned Compatibility backends.
///
/// Read from `BRIDGEVM_COMPAT_EXTRA_QEMU_ARGS` and shell-word split. This is an
/// integration-test seam (e.g. attaching a NoCloud cidata seed ISO for the
/// application-consistent live opt-in smoke) and is unset in normal operation.
pub(crate) fn compat_extra_qemu_args() -> Vec<String> {
    match env::var("BRIDGEVM_COMPAT_EXTRA_QEMU_ARGS") {
        Ok(value) => shell_word_split(&value),
        Err(_) => Vec::new(),
    }
}

/// Minimal POSIX-ish shell word splitter supporting single and double quotes.
/// Sufficient for passing QEMU `-drive file=...,...` style args from tests.
pub(crate) fn shell_word_split(input: &str) -> Vec<String> {
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

pub(crate) fn env_optional_u64(name: &str) -> Result<Option<u64>> {
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

pub(crate) fn launch_readiness_blocker_summary(readiness: &LaunchReadinessMetadata) -> String {
    if readiness.blockers.is_empty() {
        return "unknown blocker".to_string();
    }
    readiness
        .blockers
        .iter()
        .map(|blocker| {
            let mut summary = format!("{}: {}", blocker.code, blocker.message);
            if let Some(path) = &blocker.path {
                summary.push_str(&format!(" ({})", path.display()));
            } else if let Some(capability) = &blocker.capability {
                summary.push_str(&format!(" ({capability})"));
            }
            summary
        })
        .collect::<Vec<_>>()
        .join(", ")
}

impl DaemonState {
    pub(crate) fn new(store: VmStore) -> Self {
        Self {
            store,
            children: HashMap::new(),
        }
    }

    pub(crate) fn handle_request(&mut self, request: BridgeVmRequest) -> BridgeVmResponse {
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
}
