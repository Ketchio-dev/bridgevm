use super::*;
use bridgevm_media::media_lease::MediaLease;
use std::sync::atomic::{AtomicU64, Ordering};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "bv-copy-ownership-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join("source")).unwrap();
        fs::write(root.join("source/disk.raw"), b"disk").unwrap();
        Self(root)
    }
    fn source(&self) -> PathBuf {
        self.0.join("source")
    }
    fn disk(&self) -> PathBuf {
        self.source().join("disk.raw")
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn held_media_refuses_copy_before_output_creation() {
    let fixture = Fixture::new();
    let disk = fixture.disk();
    let owner = MediaLease::acquire([disk.as_path()]).unwrap();
    let output = fixture.0.join("copy");
    assert!(crate::copy_dir_all(&fixture.source(), &output).is_err());
    assert!(!output.exists());
    drop(owner);
    crate::copy_dir_all(&fixture.source(), &output).unwrap();
    assert_eq!(fs::read(output.join("disk.raw")).unwrap(), b"disk");
}

#[test]
fn copy_ownership_refuses_media_owner_and_hardlink_alias() {
    let fixture = Fixture::new();
    let disk = fixture.disk();
    let alias = fixture.0.join("alias");
    fs::hard_link(&disk, &alias).unwrap();
    let owner = acquire(&fixture.source()).unwrap();
    for path in [&disk, &alias] {
        assert_eq!(
            MediaLease::acquire([path.as_path()]).unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }
    drop(owner);
    MediaLease::acquire([disk.as_path()]).unwrap();
}
