//! Which refusals of the guest write policy hold until the guest file changes
//! (follow-up of 2026-10-23), so the share does not fetch that file again, and
//! which failures are retried. The cases an APFS share cannot produce are
//! injected, as in the parent module.

use super::*;
use crate::agent_console::share::guest_write_refusal::is_persistent;

#[test]
fn mode_and_mark_refusals_hold_until_the_file_changes() {
    // Execute bits that stay off msdos and exfat.
    let mut host = FakeHost::new("apfs", FAT_MODE);
    let error = host.seal(&mut HostEntropy::new()).unwrap_err();
    assert!(is_persistent(&error), "{error}");
    // A mark the share will not hold: no extended attributes, a deny ACL, a flag.
    for errno in [libc::ENOTSUP, libc::EACCES, libc::EPERM] {
        let mut host = FakeHost::new("apfs", CLEAN_MODE);
        host.set_errno = Some(errno);
        let error = host.seal(&mut HostEntropy::new()).unwrap_err();
        assert!(is_persistent(&error), "errno {errno}: {error}");
    }
    // An msdos mark that does not read back.
    let mut host = FakeHost::new("msdos", FAT_MODE);
    host.readback = Readback::Errno(libc::ENOATTR);
    let error = host.seal(&mut HostEntropy::new()).unwrap_err();
    assert!(is_persistent(&error), "{error}");
}

#[test]
fn other_failures_are_retried() {
    for errno in [libc::ENOSPC, libc::EDQUOT, libc::EIO] {
        let mut host = FakeHost::new("apfs", CLEAN_MODE);
        host.set_errno = Some(errno);
        let error = host.seal(&mut HostEntropy::new()).unwrap_err();
        assert!(!is_persistent(&error), "errno {errno}: {error}");
    }
    let mut host = FakeHost::new("apfs", CLEAN_MODE);
    let error = host.seal(&mut NoEntropy).unwrap_err();
    assert!(!is_persistent(&error), "{error}");
    // An error from outside the policy, such as a share without write access.
    assert!(!is_persistent(&io::Error::from_raw_os_error(libc::EACCES)));
}

#[test]
fn a_real_deny_acl_refusal_keeps_its_message_and_holds() {
    let f = Fixture::new();
    let path = f.root().join("a");
    std::fs::write(&path, b"old").unwrap();
    let acl = std::process::Command::new("/bin/chmod")
        .args(["+a", "everyone deny writeextattr"])
        .arg(&path)
        .status()
        .unwrap();
    assert!(acl.success());
    let d = Destination::open(&f.root(), "a", true).unwrap();
    let error = d.write(b"new").unwrap_err();
    assert!(is_persistent(&error), "{error}");
    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    assert_eq!(
        error.to_string(),
        "quarantine not set: Permission denied (os error 13)"
    );
}
