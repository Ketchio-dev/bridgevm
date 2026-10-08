//! Failure-path fixture cleanup must reap the directly owned process.

use super::guest_command_fixture::GuestCommandFixture;
use std::process::{Command, Stdio};

#[test]
fn guest_command_fixture_reaps_child_on_unwind() {
    let mut pid = None;
    let mut root = None;
    let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let fixture = GuestCommandFixture::new();
        pid = Some(fixture.state.children["legacy"].child.id());
        root = Some(fixture.state.store.root().to_owned());
        panic!("synthetic assertion failure after connected fixture");
    }));
    assert!(caught.is_err());
    // This is a non-signalling existence query, not cleanup of a cached PID.
    let status = Command::new("/bin/kill")
        .args(["-0", &pid.expect("fixture child").to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert!(!status.success(), "fixture child survived unwind");
    assert!(!root.expect("fixture root").exists());
}
