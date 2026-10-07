//! Fast suspend and resume must preserve the daemon's explicit Apple VZ launch opt-in.

use super::fast_opt_in_fixture::FastOptInFixture;
use super::helpers::EnvVarGuard;
use super::wait::wait_up_to_ten_seconds;
use crate::*;
use bridgevm_api::stop_backend;
use bridgevm_storage::VmRuntimeState;
use std::env;
use std::sync::Mutex;

static FAST_LIFECYCLE_ENV_LOCK: Mutex<()> = Mutex::new(());

fn capture_fast_lifecycle_env() -> EnvVarGuard {
    EnvVarGuard::capture(&[
        "BRIDGEVM_APPLE_VZ_ALLOW_REAL_START",
        "BRIDGEVM_APPLE_VZ_RUNNER",
        "BRIDGEVM_LIGHTVM_RUNNER",
        "BRIDGEVM_APPLE_VZ_STOP_AFTER_SECONDS",
        "BRIDGEVM_APPLE_VZ_FORCE_STOP_GRACE_SECONDS",
    ])
}

fn disable_real_start() {
    env::remove_var("BRIDGEVM_APPLE_VZ_ALLOW_REAL_START");
    env::remove_var("BRIDGEVM_APPLE_VZ_STOP_AFTER_SECONDS");
    env::remove_var("BRIDGEVM_APPLE_VZ_FORCE_STOP_GRACE_SECONDS");
}

fn assert_opt_in_error(error: anyhow::Error, action: &str, refusal: &str) {
    let message = format!("{error:#}");
    assert!(
        message.contains(&format!(
            "Fast Mode {action} requires explicit real-start opt-in"
        )) && message.contains("BRIDGEVM_APPLE_VZ_ALLOW_REAL_START=1")
            && message.contains(refusal),
        "{message}"
    );
}

#[test]
fn daemon_fast_resume_refuses_detached_fallback_without_real_start_opt_in() {
    let _lock = FAST_LIFECYCLE_ENV_LOCK.lock().unwrap();
    let _env = capture_fast_lifecycle_env();
    disable_real_start();
    let fixture = FastOptInFixture::new(VmRuntimeState::Suspended, true);
    let invocation = fixture.store.root().join("detached-resume-invoked");
    fixture.set_runner_env(&format!(
        "#!/bin/sh\nprintf invoked > '{}'\nexec /bin/sleep 30\n",
        invocation.display()
    ));

    let mut state = DaemonState::new(fixture.store.clone());
    let result = state.resume_backend_supervised("fast-linux");
    if result.is_ok() {
        assert!(wait_up_to_ten_seconds(|| invocation.exists()));
        stop_backend(&fixture.store, "fast-linux").unwrap();
    }
    assert_opt_in_error(
        result.expect_err("resume must not bypass explicit real-start opt-in"),
        "resume",
        "refusing to launch a detached backend",
    );
    assert!(!invocation.exists());
    assert!(state.children.is_empty());
    assert_eq!(
        fixture.store.state("fast-linux").unwrap().state,
        VmRuntimeState::Suspended
    );
    assert_eq!(fixture.store.runner_metadata("fast-linux").unwrap(), None);
    assert!(fixture.state_path.exists());
    fixture.cleanup();
}

#[test]
fn daemon_fast_suspend_refuses_real_start_without_explicit_opt_in() {
    let _lock = FAST_LIFECYCLE_ENV_LOCK.lock().unwrap();
    let _env = capture_fast_lifecycle_env();
    disable_real_start();
    let fixture = FastOptInFixture::new(VmRuntimeState::Running, false);
    let invocation = fixture.store.root().join("suspend-runner-invoked");
    fixture.set_runner_env(&format!(
        "#!/bin/sh\nprintf invoked > '{}'\nwhile [ \"$#\" -gt 0 ]; do\n\
         if [ \"$1\" = \"--apple-vz-save-state\" ]; then shift; mkdir -p \"$(dirname \"$1\")\"; \
         printf saved > \"$1\"; fi\nshift\ndone\n",
        invocation.display()
    ));

    let mut state = DaemonState::new(fixture.store.clone());
    let error = state
        .suspend_backend_supervised("fast-linux")
        .expect_err("suspend must not bypass explicit real-start opt-in");
    assert_opt_in_error(
        error,
        "suspend",
        "refusing to start a VM while handling suspend",
    );
    assert!(!invocation.exists());
    assert!(state.children.is_empty());
    assert_eq!(
        fixture.store.state("fast-linux").unwrap().state,
        VmRuntimeState::Running
    );
    assert_eq!(fixture.store.runner_metadata("fast-linux").unwrap(), None);
    assert!(!fixture.state_path.exists());
    fixture.cleanup();
}
