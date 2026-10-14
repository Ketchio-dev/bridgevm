//! Owned local directory fixtures for the snapshot restore admission tests.
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

pub(super) struct Fixture(pub PathBuf);

impl Fixture {
    pub(super) fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        Self::at(std::env::temp_dir().join(format!(
            "bridgevm-restore-media-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )))
    }

    fn at(path: PathBuf) -> Self {
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn collision_preserves_existing_directory_and_marker() {
    use std::os::unix::fs::MetadataExt;
    let parent = Fixture::new();
    let occupied = parent.0.join("preexisting");
    fs::create_dir(&occupied).unwrap();
    let marker = occupied.join("marker");
    fs::write(&marker, b"preexisting synthetic marker").unwrap();
    let before = fs::metadata(&occupied).unwrap();
    let attempt = std::panic::catch_unwind(|| Fixture::at(occupied.clone()));
    assert!(attempt.is_err(), "exclusive creation must refuse collision");
    assert_eq!(
        fs::read(&marker).ok().as_deref(),
        Some(b"preexisting synthetic marker".as_slice())
    );
    let after = fs::metadata(&occupied).unwrap();
    assert_eq!((before.dev(), before.ino()), (after.dev(), after.ino()));
}

#[test]
fn successful_creation_removes_only_owned_directory() {
    let parent = Fixture::new();
    let marker = parent.0.join("sibling-marker");
    fs::write(&marker, b"preserved sibling").unwrap();
    let path = parent.0.join("owned");
    {
        let owned = Fixture::at(path.clone());
        fs::write(owned.0.join("own-marker"), b"owned synthetic marker").unwrap();
    }
    assert!(!path.exists());
    assert_eq!(fs::read(marker).unwrap(), b"preserved sibling");
}
