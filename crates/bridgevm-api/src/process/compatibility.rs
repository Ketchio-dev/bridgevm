//! Split out of process.rs by responsibility.

use crate::*;

/// Build the QEMU command used to resume a suspended Compatibility Mode VM:
/// the normal compatibility command plus `-loadvm <tag>` so QEMU restores the
/// internal VM snapshot saved during suspend.
///
/// Shared by the daemon-less resume path ([`resume_backend`]) and the daemon's
/// supervised resume so both spawn an identical process.
pub fn build_compatibility_resume_command(
    manifest: &VmManifest,
    bundle: &Path,
) -> Result<QemuCommand, String> {
    let mut command =
        build_compatibility_command(manifest, bundle).map_err(compatibility_qemu_command_error)?;
    command.args.push("-loadvm".to_string());
    command.args.push(COMPAT_SUSPEND_SNAPSHOT_TAG.to_string());
    Ok(command)
}

/// Confirm that a spawned QEMU process survived Compatibility Mode `-loadvm`.
///
/// QEMU can abort quickly while restoring an internal snapshot. The caller must
/// only consume the suspend marker after this returns `Ok(())`.
pub fn verify_compatibility_resume_loaded(
    child: &mut Child,
    bundle: &Path,
    log_path: &Path,
) -> Result<(), String> {
    // QEMU `-loadvm` can fail fast while restoring the snapshot — notably,
    // restoring an HVF-accelerated arm64 guest aborts in cpu_pre_load
    // (cpreg_vmstate_indexes). Confirm the process actually survived loading the
    // snapshot before declaring the VM running; otherwise report honestly and
    // leave the suspend marker + qcow2 snapshot intact so nothing is lost.
    // Poll over a readiness window so a -loadvm abort is caught WHENEVER it exits
    // (not only at a single fixed 2s), since we must not consume the irreplaceable
    // suspend marker unless the VM truly came back.
    let resume_deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(Some(status)) = child.try_wait() {
            return Err(format!(
                "Compatibility Mode resume failed: QEMU exited ({status}) while restoring the saved snapshot. Restoring a QEMU snapshot is not supported for HVF-accelerated arm64 guests on this host; the suspend snapshot is preserved. See {}.",
                log_path.display()
            ));
        }
        if Instant::now() >= resume_deadline {
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }
    // The process survived the window. If QMP is reachable and reports a terminal
    // status, the restore didn't actually come up -> fail and preserve the
    // snapshot (kill the half-up QEMU so it can't orphan). If QMP isn't reachable
    // we rely on the survived-the-window signal rather than risk a false failure.
    if let Ok(status) = query_status(&qmp_socket_path(bundle)) {
        if status.is_terminal() {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "Compatibility Mode resume: QEMU reported terminal status '{}' after restoring the snapshot; the suspend snapshot is preserved. See {}.",
                status.status,
                log_path.display()
            ));
        }
    }

    Ok(())
}

/// Resume a suspended Compatibility Mode (QEMU) VM.
///
/// Relaunches QEMU detached with `-loadvm <tag>` appended to the built command
/// so it restores the internal VM snapshot saved during suspend, records the
/// new child pid, and marks the VM `running`.
pub(crate) fn resume_compatibility_backend(
    store: &VmStore,
    name: &str,
    bundle: &Path,
    manifest: &VmManifest,
) -> Result<RunnerMetadata, String> {
    let marker_path = compat_suspend_marker_path(bundle, name);
    if !marker_path.exists() {
        return Err(format!(
            "no saved Compatibility Mode state to resume from at {}; suspend the VM first",
            marker_path.display()
        ));
    }

    let (disk, active_disk) = store
        .prepare_active_disk(name)
        .map_err(|error| error.to_string())?;
    if !disk.exists {
        return Err(missing_disk_message(&disk));
    }

    let mut manifest = manifest.clone();
    apply_active_disk_to_manifest(&mut manifest, &active_disk);
    let mut command = build_compatibility_resume_command(&manifest, bundle)?;
    // Pin a free VNC display so a resumed Compat VM doesn't collide on 5900.
    assign_free_vnc_display(&mut command, &[])?;

    let log_path = bundle.join("logs").join("qemu.log");
    let guest_tools = store
        .guest_tools_runner_metadata(name)
        .map_err(|error| error.to_string())?;

    fs::create_dir_all(bundle.join("logs")).map_err(|error| error.to_string())?;
    let stdout = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|error| error.to_string())?;
    let stderr = stdout.try_clone().map_err(|error| error.to_string())?;
    let mut child = Command::new(&command.program)
        .args(&command.args)
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .spawn()
        .map_err(|error| format!("failed to spawn {}: {error}", command.program))?;

    verify_compatibility_resume_loaded(&mut child, bundle, &log_path)?;

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
        runtime_control: None,
    };
    store
        .write_runner_metadata(name, &metadata)
        .map_err(|error| error.to_string())?;

    // Resume succeeded (process survived snapshot load); consume the marker so a
    // subsequent stop->run doesn't try to resume stale state.
    let _ = fs::remove_file(&marker_path);

    store
        .transition_state(name, VmRuntimeState::Running)
        .map_err(|error| error.to_string())?;

    Ok(metadata)
}

pub(crate) fn lifecycle_plan(
    store: &VmStore,
    name: &str,
    action: LifecycleAction,
) -> Result<LifecyclePlanRecord, String> {
    let (bundle, manifest) = store.get_vm(name).map_err(|error| error.to_string())?;
    let current_state = store.state(name).map_err(|error| error.to_string())?.state;
    let qmp_supervisor = store
        .qmp_supervisor_metadata(name)
        .map_err(|error| error.to_string())?;
    let target_state = action.target_state();
    let valid_transition = lifecycle_transition_is_valid(current_state, action);
    let mut blockers = Vec::new();
    let mut notes = vec!["metadata-only lifecycle plan; no backend command was sent".to_string()];

    if !valid_transition {
        blockers.push(format!(
            "invalid-lifecycle-transition:{current_state}->{target_state}"
        ));
    }

    let runtime_engine = CurrentRuntimeEngine::for_manifest(&manifest);
    let (backend, qmp_command, socket_path, socket_available) = match runtime_engine {
        CurrentRuntimeEngine::QemuCompatibility => {
            let socket_path = qmp_socket_path(&bundle);
            let socket_available = socket_path.exists();
            if !socket_available {
                blockers.push(format!("qmp-socket-unavailable:{}", socket_path.display()));
            }
            notes.push("Compatibility Mode lifecycle control maps to QMP stop/cont".to_string());
            (
                runtime_engine.lifecycle_backend_label().to_string(),
                Some(action.qmp_command().to_string()),
                Some(socket_path),
                socket_available,
            )
        }
        CurrentRuntimeEngine::AppleVz => {
            if let Err(error) = require_apple_vz_runner() {
                blockers.push(format!("apple-vz-runner-unavailable:{error}"));
            }
            notes.push(
                "Fast Mode suspend/resume is wired through the runner via Apple VZ \
                 saveMachineState/restoreMachineState (not QMP); a real suspend/resume \
                 requires a signed AppleVzRunner (BRIDGEVM_APPLE_VZ_RUNNER)"
                    .to_string(),
            );
            (
                runtime_engine.lifecycle_backend_label().to_string(),
                None,
                None,
                false,
            )
        }
    };

    Ok(LifecyclePlanRecord {
        vm: name.to_string(),
        action,
        current_state,
        target_state,
        backend,
        metadata_only: true,
        executable: blockers.is_empty(),
        qmp_command,
        socket_path,
        socket_available,
        qmp_supervisor,
        blockers,
        notes,
    })
}

pub(crate) fn lifecycle_transition_is_valid(from: VmRuntimeState, action: LifecycleAction) -> bool {
    matches!(
        (from, action),
        (VmRuntimeState::Running, LifecycleAction::Suspend)
            | (VmRuntimeState::Suspended, LifecycleAction::Resume)
    )
}

pub(crate) fn execute_qmp_control<F>(
    store: &VmStore,
    name: &str,
    command: &str,
    execute: F,
) -> Result<QmpCommandRecord, String>
where
    F: FnOnce(&Path) -> Result<(), QemuError>,
{
    let (bundle, _) = store.get_vm(name).map_err(|error| error.to_string())?;
    let socket_path = qmp_socket_path(&bundle);
    if !socket_path.exists() {
        return Err(format!("QMP socket unavailable: {}", socket_path.display()));
    }

    execute(&socket_path).map_err(|error| error.to_string())?;
    Ok(QmpCommandRecord {
        vm: name.to_string(),
        socket_path,
        command: command.to_string(),
    })
}

pub(crate) fn records(store: &VmStore) -> Result<Vec<VmRecord>, bridgevm_storage::StorageError> {
    store
        .list_vms()?
        .into_iter()
        .map(|(_, manifest)| record_for(store, &manifest.name))
        .collect()
}

pub(crate) fn record_for(
    store: &VmStore,
    name: &str,
) -> Result<VmRecord, bridgevm_storage::StorageError> {
    let (path, manifest) = store.get_vm(name)?;
    let state = store.state(name)?.state.to_string();
    Ok(VmRecord {
        name: manifest.name,
        mode: manifest.mode.to_string(),
        guest_os: manifest.guest.os,
        guest_arch: manifest.guest.arch,
        state,
        path,
        qmp_supervisor: store.qmp_supervisor_metadata(name)?,
    })
}

pub(crate) fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
#[path = "../process_tests/mod.rs"]
mod tests;
