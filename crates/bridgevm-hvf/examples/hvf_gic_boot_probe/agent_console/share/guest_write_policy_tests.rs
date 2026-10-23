//! Guest-written share files (decisions of 2026-10-23, revised the same day):
//! a new file is created owner-only 0600 before the umask, an overwrite keeps
//! only the replaced file's read/write bits, and the temporary carries a
//! com.apple.quarantine mark before the rename. A write whose mark cannot be
//! set is refused.

use super::*;
use bridgevm_hvf::host_entropy::HostEntropy;
use bridgevm_hvf::smccc_trng::{EntropyError, EntropySource};
use std::os::unix::ffi::OsStrExt;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const QUARANTINE: &CStr = c"com.apple.quarantine";

fn mode(path: &Path) -> u32 {
    std::fs::metadata(path).unwrap().permissions().mode() & 0o7777
}

fn c_path(path: &Path) -> CString {
    CString::new(path.as_os_str().as_bytes()).unwrap()
}

fn xattr(path: &Path, name: &CStr) -> Option<Vec<u8>> {
    let path = c_path(path);
    let mut value = vec![0u8; 256];
    // SAFETY: NUL-terminated path and name; `value` has value.len() writable bytes.
    let len = unsafe {
        libc::getxattr(
            path.as_ptr(),
            name.as_ptr(),
            value.as_mut_ptr().cast(),
            value.len(),
            0,
            libc::XATTR_NOFOLLOW,
        )
    };
    value.truncate(usize::try_from(len).ok()?);
    Some(value)
}

fn set_xattr(path: &Path, name: &CStr, value: &[u8]) {
    let path = c_path(path);
    // SAFETY: NUL-terminated path and name; `value` is value.len() readable bytes.
    let status = unsafe {
        libc::setxattr(
            path.as_ptr(),
            name.as_ptr(),
            value.as_ptr().cast(),
            value.len(),
            0,
            libc::XATTR_NOFOLLOW,
        )
    };
    assert_eq!(status, 0, "{}", io::Error::last_os_error());
}

/// Check the `flags;hex-seconds;agent;UUID` form; return the seconds and UUID.
fn parse_mark(mark: &[u8]) -> (u64, String) {
    let mark = std::str::from_utf8(mark).unwrap();
    let fields: Vec<&str> = mark.split(';').collect();
    assert_eq!(fields.len(), 4, "{mark}");
    assert_eq!((fields[0], fields[2]), ("0083", "BridgeVM"), "{mark}");
    let hex = |s: &str, letters: &[u8]| {
        s.bytes()
            .all(|b| b.is_ascii_digit() || letters.contains(&b))
    };
    assert!(fields[1].len() == 8 && hex(fields[1], b"abcdef"), "{mark}");
    let uuid = fields[3];
    let groups: Vec<usize> = uuid.split('-').map(str::len).collect();
    assert_eq!(groups, [8, 4, 4, 4, 12], "{mark}");
    assert!(hex(&uuid.replace('-', ""), b"ABCDEF"), "{mark}");
    // A random (version 4) UUID with the RFC 9562 variant bits.
    assert_eq!(&uuid[14..15], "4", "{mark}");
    assert!("89AB".contains(&uuid[19..20]), "{mark}");
    (u64::from_str_radix(fields[1], 16).unwrap(), uuid.to_owned())
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[test]
fn new_guest_file_is_created_owner_only_0600() {
    let f = Fixture::new();
    let d = Destination::open(&f.root(), "new.bin", true).unwrap();
    d.write(b"guest").unwrap();
    // 0600 before the umask; the usual umasks (022, 077) leave it as is.
    let created = mode(&f.root().join("new.bin"));
    assert_eq!(created, 0o600, "{created:o}");
}

#[test]
fn overwrite_keeps_only_the_replaced_files_read_write_bits() {
    for requested in [0o755, 0o6755, 0o711, 0o640] {
        let f = Fixture::new();
        let path = f.root().join("a");
        std::fs::write(&path, b"old").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(requested)).unwrap();
        let before = mode(&path);
        let d = Destination::open(&f.root(), "a", true).unwrap();
        d.write(b"new").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"new");
        assert_eq!(mode(&path), before & 0o666, "replaced a {before:o} file");
    }
}

#[test]
fn guest_content_over_an_executable_with_an_execute_acl_cannot_run() {
    let f = Fixture::new();
    let path = f.root().join("tool");
    std::fs::write(&path, b"#!/bin/sh\nexit 0\n").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    let acl = std::process::Command::new("/bin/chmod")
        .args(["+a", "everyone allow execute"])
        .arg(&path)
        .status()
        .unwrap();
    assert!(acl.success());
    let d = Destination::open(&f.root(), "tool", true).unwrap();
    d.write(b"#!/bin/sh\nexit 0\n").unwrap();
    // exec(2) needs an execute mode bit; the copied ACL entry is not enough.
    let run = std::process::Command::new(&path).status();
    let refused = run.as_ref().map_err(io::Error::kind).err();
    assert_eq!(refused, Some(io::ErrorKind::PermissionDenied), "{run:?}");
}

#[test]
fn published_file_carries_a_fresh_quarantine_mark() {
    let f = Fixture::new();
    let start = now();
    let d = Destination::open(&f.root(), "a.txt", true).unwrap();
    d.write(b"one").unwrap();
    let mark = xattr(&f.root().join("a.txt"), QUARANTINE).expect("unmarked guest file");
    let (seconds, first) = parse_mark(&mark);
    assert!((start..=now()).contains(&seconds), "{seconds:x}");
    let d = Destination::open(&f.root(), "b.txt", true).unwrap();
    d.write(b"two").unwrap();
    let mark = xattr(&f.root().join("b.txt"), QUARANTINE).expect("unmarked guest file");
    assert_ne!(parse_mark(&mark).1, first, "quarantine UUID repeated");
}

#[test]
fn overwrite_replaces_an_approved_mark_and_keeps_other_attributes() {
    let f = Fixture::new();
    let path = f.root().join("a");
    std::fs::write(&path, b"old").unwrap();
    set_xattr(&path, c"org.bridgevm.test", b"kept");
    let approved = b"01c1;00000000;Safari;00000000-0000-4000-8000-000000000000";
    set_xattr(&path, QUARANTINE, approved);
    let d = Destination::open(&f.root(), "a", true).unwrap();
    d.write(b"new").unwrap();
    let kept = xattr(&path, c"org.bridgevm.test");
    assert_eq!(kept.as_deref(), Some(&b"kept"[..]));
    parse_mark(&xattr(&path, QUARANTINE).expect("unmarked guest file"));
}

#[test]
fn leaf_swap_cannot_move_the_mark_onto_the_link_target() {
    let f = Fixture::new();
    std::fs::write(f.root().join("a"), b"old").unwrap();
    std::fs::write(f.path.join("outside/a"), b"sentinel").unwrap();
    let d = Destination::open(&f.root(), "a", true).unwrap();
    d.write_with(|file| {
        std::fs::remove_file(f.root().join("a"))?;
        symlink(f.path.join("outside/a"), f.root().join("a"))?;
        file.write_all(b"guest")
    })
    .unwrap();
    parse_mark(&xattr(&f.root().join("a"), QUARANTINE).expect("unmarked guest file"));
    assert_eq!(xattr(&f.path.join("outside/a"), QUARANTINE), None);
}

#[test]
fn a_new_file_whose_mark_cannot_be_set_is_not_published() {
    let f = Fixture::new();
    let d = Destination::open(&f.root(), "a", true).unwrap();
    // A read-only temporary makes the real fsetxattr fail (EACCES).
    let result = d.write_with(|file| {
        file.write_all(b"guest")?;
        file.set_permissions(std::fs::Permissions::from_mode(0o444))
    });
    assert!(result.is_err(), "unmarked guest file published");
    let left: Vec<_> = std::fs::read_dir(f.root()).unwrap().collect();
    assert!(left.is_empty(), "temporary or file left behind: {left:?}");
}

#[test]
fn an_overwrite_whose_mark_cannot_be_set_keeps_the_old_file() {
    let f = Fixture::new();
    let path = f.root().join("a");
    std::fs::write(&path, b"old").unwrap();
    // The replaced file's ACL, copied onto the temporary, forbids the mark.
    let acl = std::process::Command::new("/bin/chmod")
        .args(["+a", "everyone deny writeextattr"])
        .arg(&path)
        .status()
        .unwrap();
    assert!(acl.success());
    let d = Destination::open(&f.root(), "a", true).unwrap();
    assert!(d.write(b"new").is_err(), "unmarked guest file published");
    assert_eq!(std::fs::read(&path).unwrap(), b"old");
    assert_eq!(std::fs::read_dir(f.root()).unwrap().count(), 1);
}

/// Fills every byte with one value.
struct Constant(u8);

impl EntropySource for Constant {
    fn fill(&mut self, out: &mut [u8]) -> Result<(), EntropyError> {
        out.fill(self.0);
        Ok(())
    }
}

/// A host CSPRNG that cannot produce bytes.
struct NoEntropy;

impl EntropySource for NoEntropy {
    fn fill(&mut self, _: &mut [u8]) -> Result<(), EntropyError> {
        Err(EntropyError::Unavailable)
    }
}

#[test]
fn mark_is_flags_hex_seconds_agent_and_a_version_4_uuid() {
    let at = UNIX_EPOCH + Duration::from_secs(0x6a1b_2c3d);
    let mark = policy::quarantine_mark(at, &mut Constant(0xff)).unwrap();
    assert_eq!(
        mark,
        "0083;6a1b2c3d;BridgeVM;FFFFFFFF-FFFF-4FFF-BFFF-FFFFFFFFFFFF"
    );
    let mark = policy::quarantine_mark(at, &mut Constant(0)).unwrap();
    assert_eq!(
        mark,
        "0083;6a1b2c3d;BridgeVM;00000000-0000-4000-8000-000000000000"
    );
}

#[test]
fn csprng_failure_refuses_the_write_without_a_fallback_uuid() {
    let f = Fixture::new();
    let file = File::create(f.root().join("t")).unwrap();
    let mut marked = false;
    let result = policy::seal_with(None, &file, &mut NoEntropy, |_, _, _| {
        marked = true;
        Ok(())
    });
    assert!(result.is_err());
    assert!(!marked, "a mark was set without a CSPRNG UUID");
}

#[test]
fn a_temporary_that_keeps_execute_bits_is_refused() {
    // FAT and exFAT report every file rwx------ whatever chmod asks for.
    let f = Fixture::new();
    let mut options = OpenOptions::new();
    options.write(true).create_new(true).mode(0o700);
    let file = options.open(f.root().join("t")).unwrap();
    let mut marked = false;
    let result = policy::seal_with(None, &file, &mut HostEntropy::new(), |_, _, _| {
        marked = true;
        Ok(())
    });
    assert!(result.is_err(), "guest file with execute bits sealed");
    assert!(!marked, "guest file with execute bits marked");
}

#[test]
fn share_filesystem_without_extended_attributes_refuses_the_write() {
    let f = Fixture::new();
    let file = File::create(f.root().join("t")).unwrap();
    let enotsup = |_: &File, _: &CStr, _: &[u8]| Err(io::Error::from_raw_os_error(libc::ENOTSUP));
    let error = policy::seal_with(None, &file, &mut HostEntropy::new(), enotsup).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::Unsupported);
    assert!(error.to_string().contains("extended attributes"), "{error}");
}
