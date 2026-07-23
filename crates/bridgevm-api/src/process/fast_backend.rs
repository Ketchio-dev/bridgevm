//! Split out of process.rs by responsibility.

use crate::*;

/// Boot a Fast Mode VM from cold (fresh boot, no saved-state restore).
///
/// Public entry point shared by the daemon-less CLI run path. Requires
/// `BRIDGEVM_APPLE_VZ_RUNNER` to be set (see [`apple_vz_runner_configured`]);
/// callers gate on that and fall back to dry-run planning when it is unset.
pub fn cold_start_fast_backend(store: &VmStore, name: &str) -> Result<RunnerMetadata, String> {
    let (bundle, mut manifest, _) = store
        .get_vm_with_active_disk(name)
        .map_err(|error| error.to_string())?;
    if manifest.mode != VmMode::Fast {
        return Err("cold-start launch is only implemented for Fast Mode VMs".to_string());
    }
    apply_power_aware_fast_resources(&mut manifest);
    spawn_fast_backend(store, name, &bundle, &manifest, None, false, None)
}

/// Expand `auto` Fast Mode memory/cpu using the host power state at launch time,
/// so a lightweight Apple VZ VM conserves resources on battery. Explicit per-VM
/// values are preserved (see [`bridgevm_resource_manager::resolve_memory`]). Only
/// applied to fresh launches — resume must reuse the saved-state config, and
/// preview/dry-run paths stay deterministic. Shared by the daemon-less CLI path
/// (here) and the daemon's own Fast cold-start so both adapt to battery.
pub fn apply_power_aware_fast_resources(manifest: &mut VmManifest) {
    use bridgevm_resource_manager::{
        decide_from_manifest_profile_with_power, read_on_battery, resolve_memory, resolve_vcpu,
    };
    let decision =
        decide_from_manifest_profile_with_power(&manifest.resources.profile, read_on_battery());
    manifest.resources.memory = resolve_memory(&manifest.resources.memory, &decision);
    manifest.resources.cpu = resolve_vcpu(&manifest.resources.cpu, &decision);
}

pub fn reapply_runtime_resources(
    store: &VmStore,
    name: &str,
    visibility: RuntimeResourceVisibility,
) -> Result<RuntimeResourcePolicyMetadata, String> {
    let (_, manifest) = store.get_vm(name).map_err(|error| error.to_string())?;
    if manifest.mode != VmMode::Fast {
        return Err("runtime resource reapply is only implemented for Fast Mode VMs".to_string());
    }

    let state = store.state(name).map_err(|error| error.to_string())?;
    if state.state != VmRuntimeState::Running {
        return Err(format!(
            "runtime resource reapply requires a running VM; current state is {}",
            state.state
        ));
    }

    let runner = store
        .runner_metadata(name)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "runtime resource reapply requires runner metadata".to_string())?;
    if runner.dry_run {
        return Err(
            "runtime resource reapply requires a real backend, not dry-run metadata".to_string(),
        );
    }

    let mut policy =
        build_runtime_resource_policy_metadata(name, &manifest, visibility, state.state);
    store
        .write_runtime_resource_policy_metadata(name, &policy)
        .map_err(|error| error.to_string())?;
    if runtime_control_policy_acknowledged(&runner) {
        policy.runtime_control_acknowledged = true;
        store
            .write_runtime_resource_policy_metadata(name, &policy)
            .map_err(|error| error.to_string())?;
    }
    Ok(policy)
}

pub(crate) fn runtime_control_policy_acknowledged(runner: &RunnerMetadata) -> bool {
    let Some(control) = &runner.runtime_control else {
        return false;
    };
    if !control.commands.iter().any(|command| command == "policy") {
        return false;
    }

    match send_runtime_control_command(&control.socket_path, "policy") {
        Ok(response) => response
            .get("ok")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        Err(_) => false,
    }
}

pub(crate) fn build_runtime_resource_policy_metadata(
    name: &str,
    manifest: &VmManifest,
    visibility: RuntimeResourceVisibility,
    state: VmRuntimeState,
) -> RuntimeResourcePolicyMetadata {
    use bridgevm_resource_manager::{
        decide_for_runtime, read_on_battery, resolve_memory, resolve_vcpu, ResourceProfile,
    };

    let on_battery = read_on_battery();
    let foreground = matches!(visibility, RuntimeResourceVisibility::Foreground);
    let decision = decide_for_runtime(
        ResourceProfile::parse(&manifest.resources.profile),
        foreground,
        on_battery,
    );
    RuntimeResourcePolicyMetadata {
        vm: name.to_string(),
        mode: manifest.mode.to_string(),
        profile: manifest.resources.profile.clone(),
        visibility,
        state,
        on_battery,
        memory: resolve_memory(&manifest.resources.memory, &decision),
        cpu: resolve_vcpu(&manifest.resources.cpu, &decision),
        display_fps_cap: decision.display_fps_cap,
        rationale: decision.rationale,
        live_applied: false,
        runtime_control_acknowledged: false,
        live_apply_blockers: vec![RuntimeResourcePolicyBlocker {
            code: "runtime-control-unavailable".to_string(),
            message: "Live Apple VZ CPU/RAM hot-apply is not implemented yet; the policy is available to display pacing and runtime policy IPC consumers.".to_string(),
        }],
        updated_at_unix: now_unix(),
    }
}

/// Boot a Fast Mode VM with an embedded graphical display: spawns the windowed
/// AppleVzRunner (via lightvm-runner `--apple-vz-display`) that hosts the VM in a
/// `VZVirtualMachineView` window. Requires `BRIDGEVM_APPLE_VZ_RUNNER` and a GUI
/// session. Unlike cold-start, the display path has no suspend/resume (a VZ
/// graphics device disables save/restore).
pub fn display_fast_backend(store: &VmStore, name: &str) -> Result<RunnerMetadata, String> {
    display_fast_backend_with_size(store, name, None)
}
