//! Split out of process.rs by responsibility.

use crate::*;

/// Whether a process with `pid` is currently alive.
///
/// Uses `kill -0`, which sends no signal but performs the permission/existence
/// check, so it reports liveness without disturbing the target.
#[cfg(unix)]
pub(crate) fn process_is_alive(pid: u32) -> bool {
    let Ok(pid) = libc::pid_t::try_from(pid) else {
        return false;
    };
    // SAFETY: signal 0 performs only the POSIX existence/permission check.
    let result = unsafe { libc::kill(pid, 0) };
    if result == 0 {
        return true;
    }
    std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(not(unix))]
pub(crate) fn process_is_alive(_pid: u32) -> bool {
    false
}

/// Send `signal` (e.g. `TERM`, `KILL`) to `pid` via the POSIX `kill` command.
///
/// Returns `Ok(())` even if the process is already gone (a no-op delivery),
/// matching the "make stop idempotent" contract.
#[cfg(unix)]
pub(crate) fn signal_process(pid: u32, signal: &str) -> Result<(), String> {
    let signal_number = match signal {
        "TERM" => libc::SIGTERM,
        "KILL" => libc::SIGKILL,
        _ => return Err(format!("unsupported process signal: {signal}")),
    };
    let native_pid = libc::pid_t::try_from(pid)
        .map_err(|_| format!("pid {pid} is outside the platform pid range"))?;
    // SAFETY: native_pid was range-checked and signal_number is allowlisted.
    if unsafe { libc::kill(native_pid, signal_number) } == 0 {
        return Ok(());
    }
    let error = std::io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::ESRCH) {
        return Ok(());
    }
    Err(format!("failed to send SIG{signal} to pid {pid}: {error}"))
}

#[cfg(not(unix))]
pub(crate) fn signal_process(_pid: u32, _signal: &str) -> Result<(), String> {
    Err("process termination is only supported on unix platforms".to_string())
}

/// Best-effort guard against PID reuse before we signal a recorded pid. A pid in
/// `runner.json` persists across crashes/reboots, and the OS can recycle it for
/// an unrelated process; signalling it blindly could SIGKILL a stranger. Compare
/// the live process's actual start time against the launch time we recorded:
/// only signal it if it started around then.
#[cfg(unix)]
pub(crate) fn recorded_process_is_ours(pid: u32, started_at_unix: u64) -> bool {
    // macOS `ps` exposes `etime` (elapsed, formatted `[[dd-]hh:]mm:ss`), not the
    // BSD/Linux `etimes` raw-seconds field.
    let output = match query_process_elapsed_time(pid) {
        Ok(out) if out.status.success() => out,
        // Can't query (process gone, or ps unavailable) -> don't signal it.
        _ => return false,
    };
    let elapsed_secs = match parse_ps_etime(&String::from_utf8_lossy(&output.stdout)) {
        Some(secs) => secs,
        None => return false,
    };
    let actual_start = now_unix().saturating_sub(elapsed_secs);
    // Allow generous slack for recording delay / clock skew, but reject a process
    // that started well before or after our recorded launch (i.e. a recycled pid).
    const TOLERANCE_SECS: u64 = 120;
    actual_start >= started_at_unix.saturating_sub(TOLERANCE_SECS)
        && actual_start <= started_at_unix.saturating_add(TOLERANCE_SECS)
}

#[cfg(unix)]
pub(crate) fn query_process_elapsed_time(pid: u32) -> Result<BoundedProcessOutput, String> {
    const PS_TIMEOUT: Duration = Duration::from_secs(2);
    const PS_OUTPUT_LIMIT: usize = 4096;

    let mut child = Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "etime="])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("failed to execute ps: {error}"))?;
    let mut stdout = match child.stdout.take() {
        Some(stdout) => stdout,
        None => {
            let _ = child.kill();
            let _ = child.wait();
            return Err("failed to capture ps stdout".to_string());
        }
    };
    let stdout_drain = thread::spawn(move || drain_process_stream(&mut stdout, PS_OUTPUT_LIMIT));
    let deadline = Instant::now() + PS_TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_drain.join();
                return Err("ps elapsed-time query timed out".to_string());
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_drain.join();
                return Err(format!("failed to wait for ps: {error}"));
            }
        }
    };
    let (stdout, exceeded) = join_process_stream(stdout_drain, "ps stdout")?;
    if exceeded {
        return Err(format!(
            "ps elapsed-time output exceeded {PS_OUTPUT_LIMIT}-byte limit"
        ));
    }
    Ok(BoundedProcessOutput {
        status,
        stdout,
        stderr: Vec::new(),
    })
}

/// Parse `ps -o etime` (`[[dd-]hh:]mm:ss`) into elapsed seconds.
pub(crate) fn parse_ps_etime(value: &str) -> Option<u64> {
    let value = value.trim();
    let (days, hms) = match value.split_once('-') {
        Some((days, rest)) => (days.trim().parse::<u64>().ok()?, rest),
        None => (0u64, value),
    };
    let parts: Vec<&str> = hms.split(':').collect();
    let (hours, minutes, seconds): (u64, u64, u64) = match parts.as_slice() {
        [h, m, s] => (h.parse().ok()?, m.parse().ok()?, s.parse().ok()?),
        [m, s] => (0, m.parse().ok()?, s.parse().ok()?),
        _ => return None,
    };
    Some(days * 86_400 + hours * 3_600 + minutes * 60 + seconds)
}

#[cfg(not(unix))]
pub(crate) fn recorded_process_is_ours(_pid: u32, _started_at_unix: u64) -> bool {
    false
}

/// Terminate the recorded backend process: `SIGTERM`, wait up to
/// `grace` for a clean exit, then `SIGKILL` if it is still alive.
///
/// This is the daemon-less stop path's equivalent of the daemon supervisor's
/// `Child::kill`: the API library only has the recorded pid (no `Child`
/// handle), so it signals the pid directly. Idempotent: a pid that is already
/// gone returns [`ProcessTerminationOutcome::AlreadyGone`].
pub(crate) fn terminate_recorded_process(
    pid: u32,
    started_at_unix: u64,
    grace: Duration,
) -> Result<ProcessTerminationOutcome, String> {
    // Guard against PID reuse: only signal a live pid that actually looks like
    // the process we launched. A recycled pid (or one whose identity we can't
    // confirm) is treated as already gone rather than risk killing a stranger.
    if !process_is_alive(pid) || !recorded_process_is_ours(pid, started_at_unix) {
        return Ok(ProcessTerminationOutcome::AlreadyGone);
    }

    signal_process(pid, "TERM")?;

    let deadline = Instant::now() + grace;
    while Instant::now() < deadline {
        if !process_is_alive(pid) {
            return Ok(ProcessTerminationOutcome::ExitedAfterTerm);
        }
        thread::sleep(Duration::from_millis(50));
    }

    if !process_is_alive(pid) {
        return Ok(ProcessTerminationOutcome::ExitedAfterTerm);
    }

    signal_process(pid, "KILL")?;

    // Give SIGKILL a brief window to take effect so a follow-up reconcile/stop
    // does not observe a zombie/live pid.
    let kill_deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < kill_deadline {
        if !process_is_alive(pid) {
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    Ok(ProcessTerminationOutcome::Killed)
}

/// Stop a VM's backend: gracefully quit QEMU over QMP (Compatibility Mode),
/// terminate the recorded child process (`SIGTERM` then `SIGKILL`) so no
/// AppleVzRunner / qemu orphan remains, then clear runtime state and metadata.
///
/// Dry-run VMs (no real recorded pid) keep their metadata-only behavior.
pub fn stop_backend(store: &VmStore, name: &str) -> Result<Option<RunnerMetadata>, String> {
    let (bundle, manifest) = store.get_vm(name).map_err(|error| error.to_string())?;
    let runtime_engine = CurrentRuntimeEngine::for_manifest(&manifest);
    let metadata = store
        .runner_metadata(name)
        .map_err(|error| error.to_string())?;

    // A real backend process is one recorded with a pid and not a dry run.
    // Both Fast (lightvm-runner / AppleVzRunner) and Compatibility (qemu)
    // backends record their child pid here.
    let recorded_pid = metadata
        .as_ref()
        .filter(|metadata| !metadata.dry_run)
        .and_then(|metadata| metadata.pid.map(|pid| (pid, metadata.started_at_unix)));

    // Compatibility Mode: attempt a graceful QMP quit first so QEMU can flush
    // and shut down cleanly. If the socket is gone but we have a live recorded
    // pid, fall through to signal-based termination rather than refusing.
    if runtime_engine.uses_qmp() {
        let socket_path = qmp_socket_path(&bundle);
        if socket_path.exists() {
            // Best-effort: if the guest already quit, the socket may error.
            if let Err(error) = qmp_quit(&socket_path) {
                // Only surface the error when there is no recorded pid to fall
                // back on; otherwise we proceed to terminate the pid directly.
                if recorded_pid.is_none() {
                    return Err(error.to_string());
                }
            }
        } else if recorded_pid.is_none()
            && metadata
                .as_ref()
                .is_some_and(|metadata| metadata.pid.is_some() && !metadata.dry_run)
        {
            // Defensive: pid present but filtered out should not happen, but keep
            // the historical guard for spawned-but-pidless edge cases.
            return Err(format!(
                "QMP socket unavailable: {}; refusing to mark spawned backend stopped",
                socket_path.display()
            ));
        }
    }

    // Release gate: actually terminate the recorded child process so no
    // AppleVzRunner / qemu orphan remains after stop. Dry-run VMs (no real pid)
    // skip this entirely and keep their prior metadata-only behavior.
    if let Some((pid, started_at_unix)) = recorded_pid {
        terminate_recorded_process(
            pid,
            started_at_unix,
            Duration::from_secs(STOP_TERMINATION_GRACE_SECONDS),
        )?;
    }

    // The backend process has been terminated -- the VM is definitively Stopped.
    // Force the transition so an unexpected prior recorded state can't leave a
    // killed backend recorded as Running/Suspended.
    store
        .force_transition_state(name, VmRuntimeState::Stopped)
        .map_err(|error| error.to_string())?;
    store
        .clear_runner_metadata(name)
        .map_err(|error| error.to_string())?;
    Ok(None)
}

pub fn restart_vm(store: &VmStore, name: &str) -> Result<VmRuntimeMetadata, String> {
    stop_backend(store, name)?;
    store
        .transition_state(name, VmRuntimeState::Running)
        .map_err(|error| error.to_string())
}

/// Default number of seconds the Fast Mode runner lets the guest run before it
/// pauses and saves machine state during a suspend.
pub(crate) const FAST_SUSPEND_RUN_SECONDS: u64 = 20;
pub(crate) const FAST_SUSPEND_HOST_TIMEOUT: Duration = Duration::from_secs(3 * 60);
pub(crate) const FAST_SUSPEND_TERMINATION_GRACE: Duration = Duration::from_secs(5);

/// Compute the Apple VZ saved-state file path for a VM.
///
/// Contract: `<bundle>/metadata/suspend-images/<slug(name)>.bin`.
pub fn fast_suspend_state_path(bundle: &Path, name: &str) -> PathBuf {
    bundle
        .join("metadata")
        .join("suspend-images")
        .join(format!("{}.bin", bridgevm_config::slug(name)))
}

/// Locate the `lightvm-runner` executable.
///
/// Honours `BRIDGEVM_LIGHTVM_RUNNER` (absolute path), then looks next to the
/// current executable, then falls back to `PATH` (mirrors the CLI's
/// executable-finding helper semantics).
pub(crate) fn find_lightvm_runner() -> PathBuf {
    if let Some(path) = std::env::var_os("BRIDGEVM_LIGHTVM_RUNNER") {
        return PathBuf::from(path);
    }
    if let Some(path) = bundled_executable_path("lightvm-runner") {
        return path;
    }
    if let Some(path) = path_executable("lightvm-runner") {
        return path;
    }
    PathBuf::from("lightvm-runner")
}

pub(crate) fn bundled_executable_path(name: &str) -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let candidate = exe.parent()?.join(name);
    is_executable_file(&candidate).then_some(candidate)
}

pub(crate) fn path_executable(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| is_executable_file(candidate))
}

pub(crate) fn is_executable_file(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        path.is_file()
            && path
                .metadata()
                .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
                .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        path.is_file()
    }
}

pub(crate) fn spawn_detached_fast_runner(command: &mut Command) -> std::io::Result<Child> {
    command.stdin(Stdio::null());
    #[cfg(unix)]
    {
        command.process_group(0);
    }
    command.spawn()
}

/// Resolve the signed AppleVzRunner from `BRIDGEVM_APPLE_VZ_RUNNER`.
pub(crate) fn require_apple_vz_runner() -> Result<PathBuf, String> {
    let path = std::env::var_os("BRIDGEVM_APPLE_VZ_RUNNER")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
        .ok_or_else(|| {
            "set BRIDGEVM_APPLE_VZ_RUNNER to a signed AppleVzRunner before suspending or resuming a Fast Mode VM"
                .to_string()
        })?;
    if !path.exists() {
        return Err(format!(
            "set BRIDGEVM_APPLE_VZ_RUNNER to a signed AppleVzRunner; {} does not exist",
            path.display()
        ));
    }
    Ok(path)
}

/// Whether `BRIDGEVM_APPLE_VZ_RUNNER` is set to a non-empty path.
///
/// This gates the REAL Fast Mode cold-start launch: when unset, the Fast spawn
/// path stays on the legacy dry-run + runner-required fallback for backward
/// compatibility. When set, `run_backend` (and `resume_backend`) launch a real
/// Apple VZ VM via `lightvm-runner`.
pub fn apple_vz_runner_configured() -> bool {
    std::env::var_os("BRIDGEVM_APPLE_VZ_RUNNER")
        .map(|value| !value.is_empty())
        .unwrap_or(false)
}
