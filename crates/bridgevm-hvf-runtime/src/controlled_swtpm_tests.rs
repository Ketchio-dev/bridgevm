use super::*;
use crate::{owned_control::test_support::Fixture, prepare};

#[test]
fn invalid_key_is_refused_before_state_directory_or_process() {
    let fixture = Fixture::new();
    let state = fixture.root.join("state");
    let marker = fixture.root.join("unexpected-child");
    let helper = fixture.script("never.sh", &format!("touch '{}'", marker.display()));
    let error = start_swtpm_controlled(
        &VtpmConfig {
            state_dir: state.clone(),
            swtpm_bin: helper,
            state_key: Some(vec![0; 33]),
        },
        &RuntimeControl::default(),
    )
    .err()
    .unwrap();
    assert!(error.to_string().contains("exactly 32 bytes"));
    assert!(!state.exists());
    assert!(!marker.exists());
}

#[test]
fn cancellation_during_readiness_reaps_child_before_returning_and_keeps_lease() {
    let fixture = Fixture::new();
    let marker = fixture.root.join("started");
    let helper = fixture.script(
        "not-ready.sh",
        &format!("printf ready > '{}'; exec /bin/sleep 20", marker.display()),
    );
    let prepared = prepare(fixture.manifest(), "startup owner").unwrap();
    let requested = || marker.exists();
    let control = RuntimeControl::new(&requested, &|_| panic!("unexpected uncertainty"));
    let before = Instant::now();
    let error = start_swtpm_controlled(
        &VtpmConfig {
            state_dir: fixture.root.join("state"),
            swtpm_bin: helper,
            state_key: None,
        },
        &control,
    )
    .err()
    .unwrap();
    assert!(marker.exists());
    assert!(error.to_string().contains("cancelled"));
    assert!(before.elapsed() < Duration::from_secs(2));
    assert!(control.is_cancelled());
    assert!(prepare(fixture.manifest(), "competitor").is_err());
    drop(prepared);
    assert!(prepare(fixture.manifest(), "released").is_ok());
}
