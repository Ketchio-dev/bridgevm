//! Harness-mode GET under the guest write policy (follow-up of 2026-10-23). The
//! file reaches BRIDGEVM_VIRTIO_CONSOLE_GET_DIR as a share write does: created
//! owner-only 0600, no execute bit on an overwrite, marked before the rename,
//! or not written at all. The levels of that directory a GET creates are
//! owner-only 0700 and marked, as directories for guest paths are; an existing
//! level is used as it is.

use super::*;
use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::PermissionsExt;

fn mode(path: &Path) -> u32 {
    std::fs::metadata(path).unwrap().permissions().mode() & 0o7777
}

/// Whether `path` carries a BridgeVM com.apple.quarantine mark.
fn marked(path: &Path) -> bool {
    let path = CString::new(path.as_os_str().as_bytes()).unwrap();
    let mut value = [0u8; 128];
    // SAFETY: NUL-terminated path and name; `value` has value.len() writable bytes.
    let len = unsafe {
        libc::getxattr(
            path.as_ptr(),
            c"com.apple.quarantine".as_ptr(),
            value.as_mut_ptr().cast(),
            value.len(),
            0,
            libc::XATTR_NOFOLLOW,
        )
    };
    let Ok(len) = usize::try_from(len) else {
        return false;
    };
    let mark = String::from_utf8_lossy(&value[..len]);
    mark.starts_with("0083;") && mark.contains(";BridgeVM;")
}

fn deny(dir: &Path, entry: &str) {
    let status = std::process::Command::new("/bin/chmod")
        .args(["+a", entry])
        .arg(dir)
        .status()
        .unwrap();
    assert!(status.success());
}

#[test]
fn a_get_creates_the_missing_levels_of_its_directory_owner_only_and_marked() {
    let get_dir = GetDir::scratch("dir-levels");
    let dir = get_dir.root().join("fresh/nested");
    std::env::set_var(GET_DIR_ENV, &dir);
    let mut h = harness();
    let progress = get(&mut h, "C:\\guest\\a.bin", b"guest");
    assert!(matches!(progress, ReplyProgress::Complete));
    for level in [get_dir.root().join("fresh"), dir.clone()] {
        let created = mode(&level);
        assert_eq!(created, 0o700, "a GET directory level is {created:o}");
        assert!(marked(&level), "an unmarked GET directory level");
    }
    assert_eq!(std::fs::read(dir.join("a.bin")).unwrap(), b"guest");
    assert_eq!(mode(&dir.join("a.bin")), 0o600);
    assert!(marked(&dir.join("a.bin")));
}

#[test]
fn a_get_writes_its_file_under_the_share_file_policy() {
    let get_dir = GetDir::scratch("file-policy");
    let before = mode(&get_dir.dir());
    let tool = get_dir.dir().join("tool");
    std::fs::write(&tool, b"#!/bin/sh\n").unwrap();
    std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut h = harness();
    for path in ["C:\\guest\\new.bin", "C:\\guest\\tool"] {
        assert!(matches!(
            get(&mut h, path, b"guest"),
            ReplyProgress::Complete
        ));
    }
    let new = get_dir.dir().join("new.bin");
    assert_eq!((mode(&new), marked(&new)), (0o600, true));
    // An overwrite keeps only the replaced file's read/write bits.
    assert_eq!((mode(&tool), marked(&tool)), (0o644, true));
    // The existing GET directory is used as it is.
    assert_eq!(mode(&get_dir.dir()), before);
    assert!(!marked(&get_dir.dir()));
}

#[test]
fn a_get_directory_level_whose_mark_cannot_be_set_is_removed() {
    let get_dir = GetDir::scratch("level-refused");
    deny(
        get_dir.root(),
        "everyone deny writeextattr,directory_inherit",
    );
    std::env::set_var(GET_DIR_ENV, get_dir.root().join("fresh"));
    let mut h = harness();
    assert!(matches!(
        get(&mut h, "C:\\guest\\a.bin", b"guest"),
        ReplyProgress::Complete
    ));
    let kept = get_dir.root().join("fresh").exists();
    assert!(!kept, "a GET directory level was kept without its mark");
}

#[test]
fn a_get_whose_mark_cannot_be_set_writes_nothing() {
    let get_dir = GetDir::scratch("mark-refused");
    deny(&get_dir.dir(), "everyone deny writeextattr,file_inherit");
    let mut h = harness();
    assert!(matches!(
        get(&mut h, "C:\\guest\\a.bin", b"guest"),
        ReplyProgress::Complete
    ));
    let left = std::fs::read_dir(get_dir.dir()).unwrap().count();
    assert_eq!(left, 0, "an unmarked GET file or its temporary was left");
}
