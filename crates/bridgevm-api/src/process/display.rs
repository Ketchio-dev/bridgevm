//! Split out of process.rs by responsibility.

use crate::*;

pub fn display_fast_backend_with_size(
    store: &VmStore,
    name: &str,
    display_size: Option<(u32, u32)>,
) -> Result<RunnerMetadata, String> {
    let (bundle, mut manifest, _) = store
        .get_vm_with_active_disk(name)
        .map_err(|error| error.to_string())?;
    if manifest.mode != VmMode::Fast {
        return Err("embedded display is only implemented for Fast Mode VMs".to_string());
    }
    apply_power_aware_fast_resources(&mut manifest);
    spawn_fast_backend(store, name, &bundle, &manifest, None, true, display_size)
}

/// How long to wait for the QEMU `snapshot-save` job to conclude during a
/// Compatibility Mode suspend before giving up.
pub(crate) const COMPAT_SUSPEND_SNAPSHOT_TIMEOUT_SECONDS: u64 = 120;

/// Suspend a VM end-to-end, dispatching by mode.
///
/// Fast Mode: boots the VM via `lightvm-runner`, lets it run briefly, pauses,
/// saves the VZ machine state to `<bundle>/metadata/suspend-images/<slug>.bin`,
/// and exits (SYNCHRONOUS).
///
/// Compatibility Mode: connects to the running QEMU over QMP, pauses + saves a
/// full internal qcow2 snapshot, then quits QEMU
/// (see [`suspend_compatibility_backend`]).
pub fn suspend_backend(store: &VmStore, name: &str) -> Result<RunnerMetadata, String> {
    let (bundle, mut manifest, _) = store
        .get_vm_with_active_disk(name)
        .map_err(|error| error.to_string())?;
    if CurrentRuntimeEngine::for_manifest(&manifest).uses_qmp() {
        return suspend_compatibility_backend(store, name, &bundle);
    }

    let apple_vz_runner = require_apple_vz_runner()?;
    let lightvm_runner = find_lightvm_runner();

    let (disk, active_disk) = store
        .prepare_active_disk(name)
        .map_err(|error| error.to_string())?;
    apply_active_disk_to_manifest(&mut manifest, &active_disk);

    let plan = build_fast_plan(&manifest, &bundle).map_err(|error| error.to_string())?;
    let launch_spec_path = write_launch_spec_artifact(&bundle, plan.launch_spec())
        .map_err(|error| error.to_string())?;
    let readiness = launch_readiness_metadata(&plan.launch_spec().readiness);
    if !readiness.ready {
        return Err(format!(
            "Fast Mode launch readiness failed: {}",
            launch_readiness_blocker_summary(&readiness)
        ));
    }

    let state_path = fast_suspend_state_path(&bundle, name);
    if let Some(parent) = state_path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    fs::create_dir_all(bundle.join("logs")).map_err(|error| error.to_string())?;
    let log_path: PathBuf = plan.launch_spec().logs.runner_log_path.clone().into();
    let stdout = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|error| error.to_string())?;
    let stderr = stdout.try_clone().map_err(|error| error.to_string())?;

    let args: Vec<String> = vec![
        "--launch-spec".to_string(),
        launch_spec_path.display().to_string(),
        "--require-ready".to_string(),
        "--launch".to_string(),
        "--apple-vz-runner".to_string(),
        apple_vz_runner.display().to_string(),
        "--apple-vz-allow-real-start".to_string(),
        "--apple-vz-stop-after-seconds".to_string(),
        FAST_SUSPEND_RUN_SECONDS.to_string(),
        "--apple-vz-save-state".to_string(),
        state_path.display().to_string(),
    ];

    let mut command = Command::new(&lightvm_runner);
    command
        .args(&args)
        .env("BRIDGEVM_APPLE_VZ_ALLOW_REAL_START", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    #[cfg(unix)]
    command.process_group(0);
    let mut child = command
        .spawn()
        .map_err(|error| format!("failed to run {}: {error}", lightvm_runner.display()))?;
    let status = wait_fast_suspend_runner(&mut child)?;
    if !status.success() {
        return Err(format!(
            "Fast Mode suspend runner exited with status {status}; see {}",
            log_path.display()
        ));
    }
    if !state_path.exists() {
        return Err(format!(
            "Fast Mode suspend runner finished but no saved state was written to {}",
            state_path.display()
        ));
    }

    let mut command = vec![lightvm_runner.display().to_string()];
    command.extend(args);
    let metadata = RunnerMetadata {
        engine: "lightvm".to_string(),
        pid: None,
        command,
        log_path,
        started_at_unix: now_unix(),
        dry_run: false,
        launch_spec_path: Some(launch_spec_path),
        guest_tools: None,
        disk: Some(disk),
        active_disk: Some(active_disk),
        launch_readiness: Some(readiness),
        runtime_control: None,
    };
    store
        .write_runner_metadata(name, &metadata)
        .map_err(|error| error.to_string())?;

    // Mark the suspend-image metadata so the saved state is discoverable
    // (image_exists=true after a successful suspend).
    store
        .mark_fast_suspend_image_exists(name, &state_path)
        .map_err(|error| error.to_string())?;

    // The machine state has been saved -- the VM is definitively Suspended now,
    // whatever the prior recorded state. Force the transition so an unexpected
    // prior state can't strand a saved-but-recorded-Running VM.
    store
        .force_transition_state(name, VmRuntimeState::Suspended)
        .map_err(|error| error.to_string())?;

    Ok(metadata)
}

pub(crate) fn wait_fast_suspend_runner(
    child: &mut Child,
) -> Result<std::process::ExitStatus, String> {
    wait_process_group_bounded(
        child,
        FAST_SUSPEND_HOST_TIMEOUT,
        FAST_SUSPEND_TERMINATION_GRACE,
        "Fast Mode suspend runner",
    )
}

#[cfg(unix)]
pub(crate) fn signal_process_group(pid: u32, signal: libc::c_int) -> Result<(), String> {
    let native_pid = libc::pid_t::try_from(pid)
        .map_err(|_| format!("pid {pid} is outside the platform pid range"))?;
    // SAFETY: the child was placed in a new process group whose id is its pid.
    if unsafe { libc::kill(-native_pid, signal) } == 0 {
        return Ok(());
    }
    let error = std::io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::ESRCH) {
        return Ok(());
    }
    Err(format!("failed to signal process group {pid}: {error}"))
}

pub(crate) fn wait_process_group_bounded(
    child: &mut Child,
    timeout: Duration,
    termination_grace: Duration,
    label: &str,
) -> Result<std::process::ExitStatus, String> {
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
            Ok(None) => break,
            Err(error) => {
                #[cfg(unix)]
                let _ = signal_process_group(child.id(), libc::SIGKILL);
                #[cfg(not(unix))]
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("failed to wait for {label}: {error}"));
            }
        }
    }

    #[cfg(unix)]
    if let Err(error) = signal_process_group(child.id(), libc::SIGTERM) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(error);
    }
    #[cfg(not(unix))]
    let _ = child.kill();
    let grace_deadline = Instant::now() + termination_grace;
    while Instant::now() < grace_deadline {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => thread::sleep(Duration::from_millis(20)),
            Err(_) => break,
        }
    }
    #[cfg(unix)]
    let _ = signal_process_group(child.id(), libc::SIGKILL);
    #[cfg(not(unix))]
    let _ = child.kill();
    let _ = child.wait();
    Err(format!(
        "{label} timed out after {} seconds and was terminated",
        timeout.as_secs()
    ))
}

/// Resume a previously suspended Fast Mode VM end-to-end.
///
/// Spawns `lightvm-runner` DETACHED (does not wait), restoring the saved VZ
/// machine state from `<bundle>/metadata/suspend-images/<slug>.bin`, records
/// the runner pid, and marks the VM running.
pub fn resume_backend(store: &VmStore, name: &str) -> Result<RunnerMetadata, String> {
    let (bundle, manifest, _) = store
        .get_vm_with_active_disk(name)
        .map_err(|error| error.to_string())?;
    if CurrentRuntimeEngine::for_manifest(&manifest).uses_qmp() {
        return resume_compatibility_backend(store, name, &bundle, &manifest);
    }

    let state_path = fast_suspend_state_path(&bundle, name);
    if !state_path.exists() {
        return Err(format!(
            "no saved Fast Mode state to resume from at {}; suspend the VM first",
            state_path.display()
        ));
    }

    // Resume is identical to a Fast cold start except it restores the saved VZ
    // machine state instead of booting fresh.
    spawn_fast_backend(
        store,
        name,
        &bundle,
        &manifest,
        Some(&state_path),
        false,
        None,
    )
}

/// Path to the Compatibility Mode suspend marker/metadata for a VM.
///
/// Records that an internal QEMU snapshot tagged [`COMPAT_SUSPEND_SNAPSHOT_TAG`]
/// lives inside the primary qcow2 so resume knows there is state to restore.
pub fn compat_suspend_marker_path(bundle: &Path, name: &str) -> PathBuf {
    bundle
        .join("metadata")
        .join("suspend-images")
        .join(format!("{}-compat.json", bridgevm_config::slug(name)))
}

/// Suspend a Compatibility Mode (QEMU) VM.
///
/// Connects to the running QEMU over QMP, pauses the guest (`stop`), saves a
/// full internal VM snapshot (CPU + RAM + device state) into the primary qcow2
/// via the job-based `snapshot-save` QMP command (tag
/// [`COMPAT_SUSPEND_SNAPSHOT_TAG`]), waits for the job to conclude, then `quit`s
/// QEMU. The recorded child pid (if any) is terminated to guarantee no orphan
/// remains, the suspend marker is recorded, and the VM is marked `suspended`.
pub(crate) fn suspend_compatibility_backend(
    store: &VmStore,
    name: &str,
    bundle: &Path,
) -> Result<RunnerMetadata, String> {
    let socket_path = qmp_socket_path(bundle);
    if !socket_path.exists() {
        return Err(format!(
            "QMP socket unavailable: {}; is the Compatibility Mode VM running?",
            socket_path.display()
        ));
    }

    let metadata = store
        .runner_metadata(name)
        .map_err(|error| error.to_string())?;
    let recorded_pid = metadata
        .as_ref()
        .filter(|metadata| !metadata.dry_run)
        .and_then(|metadata| metadata.pid.map(|pid| (pid, metadata.started_at_unix)));

    // Pause + save full machine state into the qcow2, then quit QEMU.
    suspend_to_snapshot(
        &socket_path,
        Duration::from_secs(COMPAT_SUSPEND_SNAPSHOT_TIMEOUT_SECONDS),
    )
    .map_err(|error| format!("Compatibility Mode suspend (snapshot-save) failed: {error}"))?;
    // The snapshot is committed; quit QEMU so the process releases the disk.
    qmp_quit(&socket_path).map_err(|error| error.to_string())?;

    // Guarantee the QEMU process is gone (QMP quit usually does this, but the
    // recorded pid is the release-gate backstop).
    if let Some((pid, started_at_unix)) = recorded_pid {
        terminate_recorded_process(
            pid,
            started_at_unix,
            Duration::from_secs(STOP_TERMINATION_GRACE_SECONDS),
        )?;
    }

    // Record a suspend marker so resume knows there is internal state to load.
    let marker_path = compat_suspend_marker_path(bundle, name);
    if let Some(parent) = marker_path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    // Reuse the disk path as the "image path" for the suspend-image metadata so
    // `mark_fast_suspend_image_exists` reports the location of the saved state.
    let disk_path = bundle.join("disks").join("root.qcow2");
    store
        .mark_fast_suspend_image_exists(name, &disk_path)
        .map_err(|error| error.to_string())?;
    fs::write(
        &marker_path,
        format!(
            "{{\"snapshot_tag\":\"{}\",\"disk\":\"{}\"}}\n",
            COMPAT_SUSPEND_SNAPSHOT_TAG,
            disk_path.display()
        ),
    )
    .map_err(|error| error.to_string())?;

    // Build a descriptive runner metadata (no live pid; backend is suspended).
    let command = build_compatibility_command(
        &store.get_vm(name).map_err(|error| error.to_string())?.1,
        bundle,
    )
    .map_err(compatibility_qemu_command_error)?;
    let log_path = bundle.join("logs").join("qemu.log");
    let guest_tools = store
        .guest_tools_runner_metadata(name)
        .map_err(|error| error.to_string())?;
    let suspend_metadata = RunnerMetadata {
        engine: "fullvm".to_string(),
        pid: None,
        command: command.render_shell_words(),
        log_path,
        started_at_unix: now_unix(),
        dry_run: false,
        launch_spec_path: None,
        guest_tools: Some(guest_tools),
        disk: None,
        active_disk: None,
        launch_readiness: None,
        runtime_control: None,
    };
    store
        .write_runner_metadata(name, &suspend_metadata)
        .map_err(|error| error.to_string())?;

    // The snapshot is committed and QEMU has been quit/killed -- the VM is
    // definitively Suspended. Force the transition so an unexpected prior state
    // can't leave a killed backend recorded as Running with an orphaned snapshot.
    store
        .force_transition_state(name, VmRuntimeState::Suspended)
        .map_err(|error| error.to_string())?;

    Ok(suspend_metadata)
}
