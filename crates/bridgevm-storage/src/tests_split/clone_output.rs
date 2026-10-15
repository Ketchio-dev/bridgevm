//! Clone reservations preserve caller entries and clean only newly created output.

use super::clone_fixture::raw_manifest as manifest;
use super::helpers::temp_store;
use crate::*;
use bridgevm_media::media_lease::MediaLease;
use std::fs;
use std::os::unix::fs::{symlink, MetadataExt};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Barrier};

struct Fixture {
    store: VmStore,
    source: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let store = temp_store();
        let source = store.create_vm(&manifest("source")).unwrap();
        let primary = store.prepare_primary_disk("source").unwrap();
        fs::write(primary.path, b"synthetic clone source").unwrap();
        Self { store, source }
    }

    fn output(&self) -> PathBuf {
        self.store.bundle_path("copy")
    }

    fn snapshot(&self) -> Vec<(PathBuf, Vec<u8>)> {
        fn visit(path: &Path, files: &mut Vec<(PathBuf, Vec<u8>)>) {
            for entry in fs::read_dir(path).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    visit(&path, files);
                } else {
                    files.push((path.clone(), fs::read(path).unwrap()));
                }
            }
        }
        let mut files = Vec::new();
        visit(&self.source, &mut files);
        files.sort_by(|a, b| a.0.cmp(&b.0));
        files
    }

    fn unchanged(&self, before: &[(PathBuf, Vec<u8>)]) {
        assert!(self.snapshot() == before, "clone changed source files");
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(self.store.root());
    }
}

#[test]
fn clone_preserves_occupied_regular_file() {
    let fixture = Fixture::new();
    let before = fixture.snapshot();
    fs::write(fixture.output(), b"caller entry").unwrap();
    let original = fs::symlink_metadata(fixture.output()).unwrap();
    let error = fixture.store.clone_vm("source", "copy", false).unwrap_err();
    assert!(matches!(error, StorageError::AlreadyExists(_)));
    let current = fs::symlink_metadata(fixture.output()).unwrap();
    assert_eq!(
        (current.dev(), current.ino()),
        (original.dev(), original.ino())
    );
    assert_eq!(fs::read(fixture.output()).unwrap(), b"caller entry");
    fixture.unchanged(&before);
}

#[test]
fn clone_preserves_occupied_symlink_and_its_live_target() {
    let fixture = Fixture::new();
    let before = fixture.snapshot();
    symlink(&fixture.source, fixture.output()).unwrap();
    let original = fs::symlink_metadata(fixture.output()).unwrap();
    let error = fixture.store.clone_vm("source", "copy", false).unwrap_err();
    assert!(matches!(error, StorageError::AlreadyExists(_)));
    let current = fs::symlink_metadata(fixture.output()).unwrap();
    assert_eq!(
        (current.dev(), current.ino()),
        (original.dev(), original.ino())
    );
    assert_eq!(fs::read_link(fixture.output()).unwrap(), fixture.source);
    fixture.unchanged(&before);
}

#[test]
fn clone_lease_refusal_cleans_reservation_and_allows_retry() {
    let fixture = Fixture::new();
    let before = fixture.snapshot();
    let disk = fixture.store.active_disk("source").unwrap().path;
    let owner = MediaLease::acquire([disk.as_path()]).unwrap();
    assert!(fixture.store.clone_vm("source", "copy", false).is_err());
    assert!(!fixture.output().exists());
    fixture.unchanged(&before);
    drop(owner);
    fixture.store.clone_vm("source", "copy", false).unwrap();
    assert_eq!(fixture.store.get_vm("copy").unwrap().1.name, "copy");
    fixture.unchanged(&before);
}

#[test]
fn clone_metadata_failure_cleans_populated_output() {
    let fixture = Fixture::new();
    fs::write(
        fixture.source.join("metadata/active-disk.json"),
        b"invalid json",
    )
    .unwrap();
    let before = fixture.snapshot();
    assert!(matches!(
        fixture.store.clone_vm("source", "copy", false),
        Err(StorageError::Json(_))
    ));
    assert!(!fixture.output().exists());
    fixture.unchanged(&before);
}

#[test]
fn clone_contenders_cannot_delete_the_winning_output() {
    let fixture = Fixture::new();
    let before = fixture.snapshot();
    let barrier = Arc::new(Barrier::new(4));
    let mut threads = Vec::new();
    for _ in 0..4 {
        let store = VmStore::new(fixture.store.root().to_path_buf());
        let barrier = Arc::clone(&barrier);
        threads.push(std::thread::spawn(move || {
            barrier.wait();
            store.clone_vm("source", "copy", false)
        }));
    }
    let mut winners = 0;
    for thread in threads {
        match thread.join().unwrap() {
            Ok(_) => winners += 1,
            Err(StorageError::AlreadyExists(_)) => {}
            Err(error) => panic!("unexpected clone error: {error}"),
        }
    }
    assert_eq!(winners, 1);
    assert_eq!(fixture.store.get_vm("copy").unwrap().1.name, "copy");
    let disk = fixture.store.active_disk("copy").unwrap().path;
    assert_eq!(fs::read(disk).unwrap(), b"synthetic clone source");
    fixture.unchanged(&before);
}
