//! Fast resume must preserve the daemon's explicit Apple VZ launch opt-in.

use super::helpers::ready_fast_manifest;
use super::helpers::temp_store;
use super::helpers::write_executable;
use super::helpers::EnvVarGuard;
use super::wait::wait_up_to_ten_seconds;
use crate::*;
use bridgevm_api::fast_suspend_state_path;
use bridgevm_api::stop_backend;
use bridgevm_storage::VmRuntimeState;
use std::env;
use std::fs;
use std::sync::Mutex;

static FAST_RESUME_ENV_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn daemon_fast_resume_refuses_detached_fallback_without_real_start_opt_in() {
    let _lock = FAST_RESUME_ENV_LOCK.lock().unwrap();
    let _env = EnvVarGuard::capture(&[
        "BRIDGEVM_APPLE_VZ_ALLOW_REAL_START",
        "BRIDGEVM_APPLE_VZ_RUNNER",
        "BRIDGEVM_LIGHTVM_RUNNER",
    ]);
    env::remove_var("BRIDGEVM_APPLE_VZ_ALLOW_REAL_START");

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

    let invocation = store.root().join("detached-resume-invoked");
    let lightvm_runner = store.root().join("fake-lightvm-runner");
    let apple_vz_runner = store.root().join("fake-AppleVzRunner");
    write_executable(
        &lightvm_runner,
        &format!(
            "#!/bin/sh\nprintf invoked > '{}'\nexec /bin/sleep 30\n",
            invocation.display()
        ),
    );
    write_executable(&apple_vz_runner, "#!/bin/sh\nexit 0\n");
    env::set_var("BRIDGEVM_LIGHTVM_RUNNER", &lightvm_runner);
    env::set_var("BRIDGEVM_APPLE_VZ_RUNNER", &apple_vz_runner);

    let mut state = DaemonState::new(store.clone());
    let result = state.resume_backend_supervised("fast-linux");
    if result.is_ok() {
        assert!(wait_up_to_ten_seconds(|| invocation.exists()));
        stop_backend(&store, "fast-linux").unwrap();
    }
    let error = result.expect_err("resume must not bypass explicit real-start opt-in");
    let message = format!("{error:#}");

    assert!(
        message.contains("requires explicit real-start opt-in")
            && message.contains("BRIDGEVM_APPLE_VZ_ALLOW_REAL_START=1")
            && message.contains("refusing to launch a detached backend"),
        "{message}"
    );
    assert!(!invocation.exists());
    assert!(!state.children.contains_key("fast-linux"));
    assert_eq!(
        store.state("fast-linux").unwrap().state,
        VmRuntimeState::Suspended
    );
    assert_eq!(store.runner_metadata("fast-linux").unwrap(), None);
    assert!(state_path.exists());

    fs::remove_dir_all(store.root()).unwrap();
}
