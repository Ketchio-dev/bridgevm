//! Proves an explicitly configured Fast restore stays daemon-supervised.

use super::helpers::ready_fast_manifest;
use super::helpers::temp_store;
use super::helpers::write_executable;
use super::wait::wait_up_to_ten_seconds;
use crate::*;
use bridgevm_api::fast_suspend_state_path;
use bridgevm_api::BridgeVmResponse;
use bridgevm_storage::VmRuntimeState;
use std::fs;

#[test]
fn supervised_fast_restore_spawns_and_tracks_restored_child_without_global_env() {
    let store = temp_store();
    let manifest = ready_fast_manifest("fast-linux");
    store.create_vm(&manifest).unwrap();
    let bundle = store.bundle_path("fast-linux");
    fs::create_dir_all(bundle.join("boot")).unwrap();
    fs::create_dir_all(bundle.join("disks")).unwrap();
    fs::write(bundle.join("boot").join("vmlinuz"), b"kernel").unwrap();
    fs::write(bundle.join("disks").join("root.raw"), b"disk").unwrap();
    let state_path = fast_suspend_state_path(&bundle, "fast-linux");
    fs::create_dir_all(state_path.parent().unwrap()).unwrap();
    fs::write(&state_path, b"saved-state").unwrap();
    store
        .force_transition_state("fast-linux", VmRuntimeState::Suspended)
        .unwrap();

    let argv_log = store.root().join("supervised-resume-argv.txt");
    let lightvm_runner = store.root().join("fake-lightvm-runner");
    let apple_vz_runner = store.root().join("fake-AppleVzRunner");
    write_executable(
        &lightvm_runner,
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\nexec /bin/sleep 30\n",
            argv_log.display()
        ),
    );
    write_executable(&apple_vz_runner, "#!/bin/sh\nexit 0\n");

    let mut state = DaemonState::new(store.clone());
    let response = state
        .spawn_fast_backend_with_restore(
            "fast-linux",
            bundle,
            ready_fast_manifest("fast-linux"),
            FastModeSpawnConfig {
                lightvm_runner,
                apple_vz_runner,
                stop_after_seconds: None,
                force_stop_grace_seconds: None,
                verify_apple_vz_runner_entitlement: false,
            },
            Some(state_path.clone()),
        )
        .unwrap();
    let BridgeVmResponse::RunnerStatus {
        metadata: Some(metadata),
        ..
    } = response
    else {
        panic!("expected supervised Fast restore metadata");
    };

    let pid = metadata.pid.expect("supervised Fast restore pid");
    assert_eq!(state.children.get("fast-linux").unwrap().child.id(), pid);
    assert_eq!(
        store
            .runner_metadata("fast-linux")
            .unwrap()
            .and_then(|metadata| metadata.pid),
        Some(pid)
    );
    assert!(metadata.command.windows(2).any(|words| {
        words[0] == "--apple-vz-restore-state" && words[1] == state_path.display().to_string()
    }));
    assert!(wait_up_to_ten_seconds(|| argv_log.exists()));

    state.cleanup_owned_backend("fast-linux", false).unwrap();
    assert_eq!(
        store.state("fast-linux").unwrap().state,
        VmRuntimeState::Stopped
    );
    assert_eq!(store.runner_metadata("fast-linux").unwrap(), None);
    fs::remove_dir_all(store.root()).unwrap();
}
