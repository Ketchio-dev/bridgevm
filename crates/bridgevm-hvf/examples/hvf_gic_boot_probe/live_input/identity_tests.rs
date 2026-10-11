use super::{InputControlFile, LiveInputCommand, LiveInputController, COMPACT_AFTER_BYTES};
use std::ffi::CString;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::fd::AsRawFd;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{symlink, MetadataExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "bridgevm-input-identity-{}-{nonce}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn controller(path: &Path) -> LiveInputController {
    LiveInputController {
        source: Some(InputControlFile::from_path(path.to_owned())),
        offset: 0,
        lines: Default::default(),
        pending: Default::default(),
        accepted_pointer_moves: 0,
        next_poll: Instant::now(),
    }
}

fn unrelated_contents() -> Vec<u8> {
    let mut contents = vec![b'x'; COMPACT_AFTER_BYTES as usize + 1];
    contents.push(b'\n');
    contents
}

fn assert_alias_is_not_consumed(link: impl FnOnce(&Path, &Path)) {
    let fixture = Fixture::new();
    let victim = fixture.path("unrelated");
    let input_path = fixture.path("input.ctl");
    let contents = unrelated_contents();
    fs::write(&victim, &contents).unwrap();
    link(&victim, &input_path);
    let mut input = controller(&input_path);

    input.read_new_commands();
    input.read_new_commands();

    assert_eq!(
        fs::metadata(&victim).unwrap().len(),
        contents.len() as u64,
        "input consumer must not truncate an unrelated aliased file"
    );
    assert_eq!(fs::read(&victim).unwrap(), contents);
    assert_eq!(input.offset, 0, "aliased bytes must not be consumed");
    assert!(input.pending.is_empty());
}

#[test]
fn symlink_input_does_not_truncate_unrelated_file() {
    assert_alias_is_not_consumed(|from, to| symlink(from, to).unwrap());
}

#[test]
fn hardlink_input_does_not_truncate_unrelated_file() {
    assert_alias_is_not_consumed(|from, to| fs::hard_link(from, to).unwrap());
}

#[test]
fn cached_descriptor_refuses_replacement_and_does_not_reopen_it() {
    let fixture = Fixture::new();
    let path = fixture.path("input.ctl");
    let original = fixture.path("original");
    let contents = unrelated_contents();
    fs::write(&path, &contents).unwrap();
    let mut input = controller(&path);
    input.read_new_commands();
    assert_eq!(input.offset, COMPACT_AFTER_BYTES);
    fs::rename(&path, &original).unwrap();
    fs::write(&path, &contents).unwrap();

    for _ in 0..3 {
        input.read_new_commands();
    }
    assert_eq!(input.offset, COMPACT_AFTER_BYTES);
    assert!(input.source.as_mut().unwrap().length().is_none());
    assert_eq!(fs::read(&original).unwrap(), contents);
    assert_eq!(fs::read(&path).unwrap(), contents);
    // Identity rejection must release the shared protocol lock too.
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&original)
        .unwrap();
    assert_eq!(
        unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
        0
    );
}

#[test]
fn cached_descriptor_rejects_new_aliases_before_compaction() {
    for hardlink in [false, true] {
        let fixture = Fixture::new();
        let path = fixture.path("input.ctl");
        let alias = fixture.path("alias");
        let contents = unrelated_contents();
        fs::write(&path, &contents).unwrap();
        let mut input = controller(&path);
        input.read_new_commands();
        assert_eq!(input.offset, COMPACT_AFTER_BYTES);
        if hardlink {
            fs::hard_link(&path, &alias).unwrap();
        } else {
            fs::rename(&path, &alias).unwrap();
            symlink(&alias, &path).unwrap();
        }

        input.read_new_commands();
        assert_eq!(input.offset, COMPACT_AFTER_BYTES);
        assert_eq!(fs::read(&path).unwrap(), contents);
        assert_eq!(fs::read(&alias).unwrap(), contents);
    }
}

#[test]
fn owned_single_link_file_compacts_in_place_and_accepts_later_appends() {
    let fixture = Fixture::new();
    let path = fixture.path("input.ctl");
    fs::write(&path, unrelated_contents()).unwrap();
    let before = fs::metadata(&path).unwrap();
    let mut input = controller(&path);
    input.read_new_commands();
    let fd = input.source.as_mut().unwrap().file().unwrap().as_raw_fd();
    let descriptor_flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
    let status_flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    assert!(descriptor_flags >= 0 && status_flags >= 0);
    assert_ne!(descriptor_flags & libc::FD_CLOEXEC, 0);
    assert_ne!(status_flags & libc::O_NONBLOCK, 0);
    input.read_new_commands();
    let after = fs::metadata(&path).unwrap();
    assert_eq!((before.dev(), before.ino()), (after.dev(), after.ino()));
    assert_eq!(after.len(), 0);
    assert_eq!(input.offset, 0);

    OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(b"KEY enter\n")
        .unwrap();
    input.read_new_commands();
    assert!(matches!(input.pending.front(), Some(LiveInputCommand::Key(key)) if key == "enter"));
    assert_eq!(
        input.source.as_mut().unwrap().file().unwrap().as_raw_fd(),
        fd
    );
}

#[test]
fn fifo_and_directory_are_rejected_without_waiting_for_a_writer() {
    let fixture = Fixture::new();
    let path = fixture.path("input.ctl");
    let name = CString::new(path.as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
    for path in [&path, &fixture.0] {
        let mut input = controller(path);
        input.read_new_commands();
        assert_eq!(input.offset, 0);
        assert!(input.pending.is_empty());
        assert!(input.source.as_mut().unwrap().file().is_none());
    }
}
