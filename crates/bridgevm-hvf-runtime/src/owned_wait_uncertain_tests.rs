//! A separate test process contains the intentional external-wait fault.

use super::*;
use crate::{owned_control::test_support::Fixture, prepare};
use bridgevm_hvf::media::lock::MediaLease;
use std::process::{Command, Stdio};

#[test]
fn ambiguous_wait_keeps_owner_and_both_leases_until_external_fixture_teardown() {
    let fixture = Fixture::new();
    let mut owner = Command::new(std::env::current_exe().unwrap())
        .args([
            "--ignored",
            "--exact",
            "owned_child::uncertain_tests::uncertain_child_process",
        ])
        .env("BRIDGEVM_TEST_UNCERTAIN_ROOT", &fixture.root)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let check = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        fixture.wait_file("unconfirmed");
        assert!(owner.try_wait().unwrap().is_none());
        for name in ["disk.raw", "vars.fd"] {
            assert!(MediaLease::acquire(&fixture.root.join(name), "competitor").is_err());
        }
        assert!(!fixture.root.join("tpm-teardown-reached").exists());
        assert_eq!(
            std::fs::read_to_string(fixture.root.join("unconfirmed")).unwrap(),
            "child wait failed"
        );
    }));
    // The exact fixture owner is the only remaining process; its child was
    // deliberately reaped before entering the uncertain ownership path.
    owner.kill().unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while owner.try_wait().unwrap().is_none() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(owner.try_wait().unwrap().is_some());
    for name in ["disk.raw", "vars.fd"] {
        assert!(MediaLease::acquire(&fixture.root.join(name), "fixture gone").is_ok());
    }
    if let Err(panic) = check {
        std::panic::resume_unwind(panic);
    }
}

#[test]
#[ignore = "spawned only by the bounded ownership fixture above"]
fn uncertain_child_process() {
    let root = std::path::PathBuf::from(std::env::var_os("BRIDGEVM_TEST_UNCERTAIN_ROOT").unwrap());
    let manifest =
        crate::LaunchManifest::parse(
            &format!(
        "{{\"version\":1,\"disk\":\"{}\",\"uefi_vars\":\"{}\",\"ram_mib\":1024,\"vcpus\":1}}",
        root.join("disk.raw").display(), root.join("vars.fd").display()),
            false,
        )
        .unwrap();
    let _prepared = prepare(manifest, "uncertain owned fixture").unwrap();
    let process = Command::new("/bin/sh")
        .args(["-c", "exit 17"])
        .spawn()
        .unwrap();
    let mut child = OwnedChild::new(process, ChildRole::Helper);
    let mut status = 0;
    // SAFETY: this test alone spawned/owns this exact Child. This intentional
    // wait violates the observer contract, exposing ECHILD without PID lookup.
    assert_eq!(
        unsafe { libc::waitpid(child.id() as libc::pid_t, &mut status, 0) },
        child.id() as i32
    );
    let report = |value: CleanupUnconfirmed| {
        std::fs::write(root.join("unconfirmed"), value.reason).unwrap();
    };
    let control = RuntimeControl::new(&|| true, &report);
    let _ = child.wait(&control);
    std::fs::write(root.join("tpm-teardown-reached"), b"incorrect").unwrap();
    panic!("an unreaped identity must never return to TPM teardown");
}
