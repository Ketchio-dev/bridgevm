use super::*;
use crate::owned_control::test_support::Fixture;
use std::os::unix::process::ExitStatusExt;
use std::process::{Command, Stdio};

#[test]
fn normal_exit_is_observed_and_repeated_shutdown_does_not_signal() {
    let child = Command::new("/bin/sh")
        .args(["-c", "exit 17"])
        .spawn()
        .unwrap();
    let mut child = OwnedChild::new(child, ChildRole::Helper);
    let exit = child.wait(&RuntimeControl::default()).unwrap();
    assert_eq!(exit.status.code(), Some(17));
    assert!(!exit.cancelled);
    assert_eq!(child.shutdown(&RuntimeControl::default()).code(), Some(17));
}

#[test]
fn cancellation_terminates_only_the_retained_child() {
    let child = Command::new("/bin/sleep").arg("20").spawn().unwrap();
    let mut child = OwnedChild::new(child, ChildRole::Helper);
    let start = Instant::now();
    let exit = child
        .wait(&RuntimeControl::new(&|| true, &|_| {
            panic!("unexpected uncertainty")
        }))
        .unwrap();
    assert!(exit.cancelled);
    assert_eq!(exit.status.signal(), Some(libc::SIGTERM));
    assert!(start.elapsed() < Duration::from_secs(2));
}

#[test]
fn ignored_term_escalates_after_two_seconds_and_reaps() {
    let fixture = Fixture::new();
    let script = fixture.script(
        "ignore.sh",
        "trap '' TERM; printf ready > \"$1\"; exec /bin/sleep 20",
    );
    let child = Command::new(script)
        .arg(fixture.root.join("ready"))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut child = OwnedChild::new(child, ChildRole::Helper);
    fixture.wait_file("ready");
    let start = Instant::now();
    let exit = child
        .wait(&RuntimeControl::new(&|| true, &|_| {
            panic!("unexpected uncertainty")
        }))
        .unwrap();
    assert_eq!(exit.status.signal(), Some(libc::SIGKILL));
    assert!(start.elapsed() >= Duration::from_secs(2));
    assert!(start.elapsed() < Duration::from_secs(4));
    assert!(child.try_wait().unwrap().is_some());
}
