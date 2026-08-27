//! Regression for daemon-owned restart process replacement.

use super::helpers::compatibility_manifest;
use super::helpers::temp_store;
use super::helpers::write_executable;
use super::helpers::EnvVarGuard;
use super::wait::wait_up_to_ten_seconds;
use crate::*;
use bridgevm_api::BridgeVmResponse;
use bridgevm_storage::VmRuntimeState;
use std::env;
use std::fs;
use std::process::Command;
use std::sync::Mutex;

static PATH_ENV_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn daemon_owned_restart_spawns_and_tracks_replacement_child() {
    let _lock = PATH_ENV_LOCK.lock().unwrap();
    let _env = EnvVarGuard::capture(&["PATH", "BRIDGEVM_COMPAT_EXTRA_QEMU_ARGS"]);
    env::remove_var("BRIDGEVM_COMPAT_EXTRA_QEMU_ARGS");

    let store = temp_store();
    let manifest = compatibility_manifest("legacy");
    store.create_vm(&manifest).unwrap();
    let bundle = store.bundle_path("legacy");
    let disk = bundle.join(&manifest.storage.primary.path);
    fs::create_dir_all(disk.parent().unwrap()).unwrap();
    fs::write(&disk, b"disk").unwrap();

    let fake_bin = store.root().join("bin");
    let launch_log = store.root().join("restart-launches.txt");
    fs::create_dir_all(&fake_bin).unwrap();
    write_executable(
        &fake_bin.join("qemu-system-x86_64"),
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$$\" >> '{}'\nexec /bin/sleep 5\n",
            launch_log.display()
        ),
    );
    let saved_path = env::var_os("PATH").unwrap_or_default();
    let mut search_path = vec![fake_bin];
    search_path.extend(env::split_paths(&saved_path));
    env::set_var("PATH", env::join_paths(search_path).unwrap());

    let old_child = Command::new("/bin/sleep").arg("30").spawn().unwrap();
    let old_pid = old_child.id();
    store
        .transition_state("legacy", VmRuntimeState::Running)
        .unwrap();
    let mut state = DaemonState::new(store.clone());
    state.children.insert(
        "legacy".to_string(),
        SupervisedBackend::new(old_child),
    );

    let response = state.restart_owned_backend("legacy").unwrap();
    let BridgeVmResponse::RunnerStatus {
        metadata: Some(metadata),
        ..
    } = response
    else {
        panic!("owned restart must return replacement runner metadata");
    };
    let new_pid = metadata.pid.expect("replacement pid");
    assert_ne!(new_pid, old_pid);
    assert_eq!(
        state.children.get("legacy").unwrap().child.id(),
        new_pid
    );
    assert_eq!(
        store.state("legacy").unwrap().state,
        VmRuntimeState::Running
    );
    assert_eq!(
        store
            .runner_metadata("legacy")
            .unwrap()
            .and_then(|metadata| metadata.pid),
        Some(new_pid)
    );
    assert!(wait_up_to_ten_seconds(|| launch_log.exists()));
    assert_eq!(fs::read_to_string(&launch_log).unwrap().lines().count(), 1);

    let mut replacement = state.children.remove("legacy").unwrap();
    let _ = replacement.child.kill();
    let _ = replacement.child.wait();
    store
        .force_transition_state("legacy", VmRuntimeState::Stopped)
        .unwrap();
    store.clear_runner_metadata("legacy").unwrap();
    fs::remove_dir_all(store.root()).unwrap();
}
