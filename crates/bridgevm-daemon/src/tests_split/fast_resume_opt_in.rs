//! Fast resume must preserve the daemon's explicit Apple VZ launch opt-in.

use super::helpers::ready_fast_manifest;
use super::helpers::temp_store;
use super::helpers::write_executable;
use super::helpers::EnvVarGuard;
use super::wait::wait_up_to_ten_seconds;
use crate::*;
use bridgevm_api::fast_suspend_state_path;
use bridgevm_api::stop_backend;
use bridgevm_api::BridgeVmResponse;
use bridgevm_storage::VmRuntimeState;
use bridgevm_storage::VmStore;
use std::env;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;

static FAST_RESUME_ENV_LOCK: Mutex<()> = Mutex::new(());

struct SuspendedFastFixture {
    store: VmStore,
    state_path: PathBuf,
}

fn suspended_fast_fixture() -> SuspendedFastFixture {
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

    SuspendedFastFixture { store, state_path }
}

fn capture_fast_resume_env() -> EnvVarGuard {
    EnvVarGuard::capture(&[
        "BRIDGEVM_APPLE_VZ_ALLOW_REAL_START",
        "BRIDGEVM_APPLE_VZ_RUNNER",
        "BRIDGEVM_LIGHTVM_RUNNER",
        "BRIDGEVM_APPLE_VZ_STOP_AFTER_SECONDS",
        "BRIDGEVM_APPLE_VZ_FORCE_STOP_GRACE_SECONDS",
    ])
}

#[test]
fn daemon_fast_resume_refuses_detached_fallback_without_real_start_opt_in() {
    let _lock = FAST_RESUME_ENV_LOCK.lock().unwrap();
    let _env = capture_fast_resume_env();
    env::remove_var("BRIDGEVM_APPLE_VZ_ALLOW_REAL_START");

    let fixture = suspended_fast_fixture();
    let invocation = fixture.store.root().join("detached-resume-invoked");
    let lightvm_runner = fixture.store.root().join("fake-lightvm-runner");
    let apple_vz_runner = fixture.store.root().join("fake-AppleVzRunner");
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

    let mut state = DaemonState::new(fixture.store.clone());
    let result = state.resume_backend_supervised("fast-linux");
    if result.is_ok() {
        assert!(wait_up_to_ten_seconds(|| invocation.exists()));
        stop_backend(&fixture.store, "fast-linux").unwrap();
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
        fixture.store.state("fast-linux").unwrap().state,
        VmRuntimeState::Suspended
    );
    assert_eq!(fixture.store.runner_metadata("fast-linux").unwrap(), None);
    assert!(fixture.state_path.exists());

    fs::remove_dir_all(fixture.store.root()).unwrap();
}

#[cfg(target_os = "macos")]
fn write_signed_apple_vz_runner(path: &Path) {
    fs::copy("/usr/bin/true", path).unwrap();
    let entitlements = path.with_extension("entitlements.plist");
    fs::write(
        &entitlements,
        b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
          <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \
          \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
          <plist version=\"1.0\"><dict>\
          <key>com.apple.security.virtualization</key><true/>\
          </dict></plist>\n",
    )
    .unwrap();
    let output = Command::new("/usr/bin/codesign")
        .args(["--force", "--sign", "-", "--entitlements"])
        .arg(&entitlements)
        .arg(path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "codesign failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(target_os = "macos")]
#[test]
fn daemon_fast_resume_with_opt_in_spawns_and_tracks_restored_child() {
    let _lock = FAST_RESUME_ENV_LOCK.lock().unwrap();
    let _env = capture_fast_resume_env();
    env::set_var("BRIDGEVM_APPLE_VZ_ALLOW_REAL_START", "1");
    env::remove_var("BRIDGEVM_APPLE_VZ_STOP_AFTER_SECONDS");
    env::remove_var("BRIDGEVM_APPLE_VZ_FORCE_STOP_GRACE_SECONDS");

    let fixture = suspended_fast_fixture();
    let argv_log = fixture.store.root().join("supervised-resume-argv.txt");
    let lightvm_runner = fixture.store.root().join("fake-lightvm-runner");
    let apple_vz_runner = fixture.store.root().join("fake-AppleVzRunner");
    write_executable(
        &lightvm_runner,
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\nexec /bin/sleep 30\n",
            argv_log.display()
        ),
    );
    write_signed_apple_vz_runner(&apple_vz_runner);
    env::set_var("BRIDGEVM_LIGHTVM_RUNNER", &lightvm_runner);
    env::set_var("BRIDGEVM_APPLE_VZ_RUNNER", &apple_vz_runner);

    let mut state = DaemonState::new(fixture.store.clone());
    let response = state.resume_backend_supervised("fast-linux").unwrap();
    let BridgeVmResponse::RunnerStatus {
        metadata: Some(metadata),
        ..
    } = response
    else {
        panic!("expected supervised Fast resume metadata");
    };

    let pid = metadata.pid.expect("supervised Fast resume pid");
    assert_eq!(state.children.get("fast-linux").unwrap().child.id(), pid);
    assert_eq!(
        fixture.store.state("fast-linux").unwrap().state,
        VmRuntimeState::Running
    );
    assert_eq!(
        fixture
            .store
            .runner_metadata("fast-linux")
            .unwrap()
            .and_then(|metadata| metadata.pid),
        Some(pid)
    );
    assert!(metadata
        .command
        .contains(&"--apple-vz-allow-real-start".to_string()));
    assert!(metadata.command.windows(2).any(|words| {
        words[0] == "--apple-vz-restore-state"
            && words[1] == fixture.state_path.display().to_string()
    }));
    assert!(wait_up_to_ten_seconds(|| argv_log.exists()));

    state.cleanup_owned_backend("fast-linux", false).unwrap();
    assert!(!state.children.contains_key("fast-linux"));
    assert_eq!(
        fixture.store.state("fast-linux").unwrap().state,
        VmRuntimeState::Stopped
    );
    assert_eq!(fixture.store.runner_metadata("fast-linux").unwrap(), None);
    fs::remove_dir_all(fixture.store.root()).unwrap();
}
