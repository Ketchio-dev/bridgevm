use super::*;
use crate::{owned_child::OwnedChild, ChildRole, RuntimeControl};
use std::process::{Command, Stdio};

fn panic_text(f: impl FnOnce()) -> String {
    let error = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).unwrap_err();
    error.downcast_ref::<String>().unwrap().clone()
}

#[test]
fn missing_marker_reports_observed_exit_and_stderr_before_cleanup() {
    let fixture = Fixture::new();
    let child = Command::new("/bin/sh")
        .args(["-c", "printf diagnostic-sentinel >&2; exit 17"])
        .stdout(Stdio::null())
        .stderr(fixture.stderr())
        .spawn()
        .unwrap();
    let mut child = OwnedChild::new(child, ChildRole::Helper);
    let message = panic_text(|| fixture.wait_file_with("ready", || child.try_wait()));
    assert!(message.contains("fixture readiness deadline"));
    assert!(message.contains("child exited code=Some(17) signal=None"));
    assert!(message.contains("diagnostic-sentinel"));
    assert!(message.contains("marker=\"ready\" [kind=NotFound"));
    assert_eq!(child.shutdown(&RuntimeControl::default()).code(), Some(17));
    let root = fixture.root.clone();
    drop(fixture);
    assert!(!root.exists());
}

#[test]
fn missing_marker_reports_running_child_without_stopping_it() {
    let fixture = Fixture::new();
    let child = Command::new("/bin/sleep")
        .arg("20")
        .stderr(fixture.stderr())
        .spawn()
        .unwrap();
    let mut child = OwnedChild::new(child, ChildRole::Helper);
    let message = panic_text(|| fixture.wait_file_with("ready", || child.try_wait()));
    assert!(message.contains("child still running at nonblocking observation"));
    assert!(child.try_wait().unwrap().is_none());
    assert_eq!(
        child.shutdown(&RuntimeControl::default()).signal(),
        Some(libc::SIGTERM)
    );
}

#[test]
fn successful_readiness_does_not_sample_child_or_read_stderr() {
    let fixture = Fixture::new();
    std::fs::write(fixture.root.join("ready"), b"ready").unwrap();
    fixture.wait_file_with("ready", || panic!("status callback must be failure-only"));
}

#[test]
fn stderr_is_capped_and_read_errors_are_context() {
    use std::io::Write;
    let fixture = Fixture::new();
    let mut file = fixture.stderr();
    file.write_all(&vec![b'a'; STDERR_LIMIT as usize]).unwrap();
    file.write_all(b"hidden-tail").unwrap();
    let context = fixture.stderr_context();
    assert!(context.contains("truncated=true"));
    assert!(!context.contains("hidden-tail"));
    std::fs::remove_file(fixture.root.join("stderr")).unwrap();
    assert!(fixture.stderr_context().contains("kind=NotFound"));
    std::os::unix::fs::symlink("disk.raw", fixture.root.join("stderr")).unwrap();
    assert!(fixture.stderr_context().starts_with("unavailable"));
}

#[test]
fn diagnostic_fifo_is_refused_without_reading() {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let fixture = Fixture::new();
    let path = CString::new(fixture.root.join("stderr").as_os_str().as_bytes()).unwrap();
    // SAFETY: the path is an owned absent fixture entry; no outside namespace is modified.
    assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
    assert!(fixture.stderr_context().contains("kind=InvalidData"));
}
