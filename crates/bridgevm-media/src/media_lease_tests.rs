use super::*;
use std::os::fd::AsRawFd;
use std::path::PathBuf;
#[path = "media_lease_lifecycle_tests.rs"]
mod lifecycle;

struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        loop {
            let sequence = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let path =
                std::env::temp_dir().join(format!("bv-{tag}-{}-{sequence}", std::process::id()));
            match fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("create fixture: {error}"),
            }
        }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }

    fn write(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.path(name);
        fs::write(&path, bytes).unwrap();
        path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn exclusive_across_separate_opens_and_released_on_drop() {
    let s = Scratch::new("lease-exclusive");
    let disk = s.write("disk", b"d");
    let vars = s.write("vars", b"v");
    let lease = MediaLease::acquire([disk.as_path(), vars.as_path()]).unwrap();
    assert_eq!(
        MediaLease::acquire([vars.as_path(), disk.as_path()])
            .unwrap_err()
            .kind(),
        io::ErrorKind::WouldBlock
    );
    drop(lease);
    MediaLease::acquire([disk.as_path(), vars.as_path()]).unwrap();
}

#[test]
fn overlapping_pairs_conflict_and_failed_acquisition_releases_resources() {
    let s = Scratch::new("lease-overlap");
    let disk = s.write("disk", b"d");
    let vars = s.write("vars", b"v");
    let other = s.write("other", b"o");
    let _lease = MediaLease::acquire([disk.as_path(), vars.as_path()]).unwrap();
    assert!(MediaLease::acquire([other.as_path(), vars.as_path()]).is_err());
    MediaLease::acquire([other.as_path()]).unwrap();
}

#[test]
fn hardlink_aliases_share_the_inode_lease() {
    let s = Scratch::new("lease-hardlink");
    let disk = s.write("disk", b"d");
    let alias = s.path("alias");
    fs::hard_link(&disk, &alias).unwrap();
    let _lease = MediaLease::acquire([disk.as_path()]).unwrap();
    assert_eq!(
        MediaLease::acquire([alias.as_path()]).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
}

#[test]
fn a_later_created_file_still_conflicts_on_its_path() {
    let s = Scratch::new("lease-missing");
    let disk = s.path("disk");
    let _lease = MediaLease::acquire([disk.as_path()]).unwrap();
    fs::write(&disk, b"created").unwrap();
    assert_eq!(
        MediaLease::acquire([disk.as_path()]).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
}

#[test]
fn lock_descriptors_do_not_survive_exec() {
    let s = Scratch::new("lease-cloexec");
    let disk = s.write("disk", b"d");
    let lease = MediaLease::acquire([disk.as_path()]).unwrap();
    for file in &lease.files {
        // SAFETY: the lease owns this descriptor; F_GETFD takes no extra argument.
        let flags = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_GETFD) };
        assert_ne!(flags & libc::FD_CLOEXEC, 0);
    }
}

#[test]
fn another_process_cannot_acquire_owned_media() {
    let s = Scratch::new("lease-process");
    let disk = s.write("disk", b"d");
    let lease = MediaLease::acquire([disk.as_path()]).unwrap();
    run_child(&disk, true);
    drop(lease);
    run_child(&disk, false);
}

fn run_child(path: &Path, blocked: bool) {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "media_lease::tests::subprocess_contender",
            "--ignored",
        ])
        .env("BV_TEST_LEASE_PATH", path)
        .env("BV_TEST_LEASE_BLOCKED", if blocked { "1" } else { "0" })
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
#[ignore = "invoked only by the cross-process lease test"]
fn subprocess_contender() {
    let path = PathBuf::from(std::env::var_os("BV_TEST_LEASE_PATH").unwrap());
    let result = MediaLease::acquire([path.as_path()]);
    if std::env::var("BV_TEST_LEASE_BLOCKED").unwrap() == "1" {
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::WouldBlock);
    } else {
        result.unwrap();
    }
}
