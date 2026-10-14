//! Export creation collisions never reopen caller entries or completed winners.

use super::helpers::{manifest, temp_store};
use crate::*;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::{symlink, MetadataExt};
use std::sync::{Arc, Barrier};

struct Fixture(VmStore);
impl Fixture {
    fn new() -> Self {
        let store = temp_store();
        store.ensure().unwrap();
        Self(store)
    }

    fn source(&self, name: &str, payload: &[u8]) {
        let mut manifest = manifest(name);
        manifest.storage.primary.path = "disks/root.raw".into();
        manifest.storage.primary.format = "raw".into();
        let source = self.0.create_vm(&manifest).unwrap();
        fs::write(source.join("disks/root.raw"), payload).unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(self.0.root());
    }
}

#[test]
fn tar_creation_refuses_entries_appearing_after_admission() {
    let fixture = Fixture::new();
    for dangling in [false, true] {
        let output = fixture.0.root().join(format!("occupied-{dangling}.tar"));
        let target = fixture.0.root().join("absent-target");
        crate::export_destination::ensure_absent(&output).unwrap();
        if dangling {
            symlink(&target, &output).unwrap();
        } else {
            fs::write(&output, b"caller archive").unwrap();
        }
        let original = fs::symlink_metadata(&output).unwrap();
        assert!(matches!(
            crate::export_destination::create_tar(&output),
            Err(StorageError::ExportAlreadyExists(_))
        ));
        let current = fs::symlink_metadata(&output).unwrap();
        assert_eq!(
            (current.dev(), current.ino()),
            (original.dev(), original.ino())
        );
        if dangling {
            assert_eq!(fs::read_link(&output).unwrap(), target);
            assert!(!target.exists());
        } else {
            assert_eq!(fs::read(&output).unwrap(), b"caller archive");
        }
    }
}

#[test]
fn public_export_preserves_partial_output_on_retry() {
    let fixture = Fixture::new();
    fixture.source("source", b"source raw bytes");
    let output = fixture.0.root().join("partial.tar");
    let mut file = crate::export_destination::create_tar(&output).unwrap();
    file.write_all(b"partial archive bytes").unwrap();
    drop(file);
    let original = fs::metadata(&output).unwrap();
    assert!(matches!(
        fixture.0.export_vm("source", &output),
        Err(StorageError::ExportAlreadyExists(_))
    ));
    let current = fs::metadata(&output).unwrap();
    assert_eq!(
        (current.dev(), current.ino()),
        (original.dev(), original.ino())
    );
    assert_eq!(fs::read(output).unwrap(), b"partial archive bytes");
}

#[test]
fn public_export_propagates_destination_metadata_error() {
    let fixture = Fixture::new();
    fixture.source("source", b"source raw bytes");
    let parent = fixture.0.root().join("regular-file-parent");
    fs::write(&parent, b"caller parent").unwrap();
    let output = parent.join("output.tar");
    let expected = fs::symlink_metadata(&output).unwrap_err().kind();
    assert_ne!(expected, std::io::ErrorKind::NotFound);
    let StorageError::Io(error) = fixture.0.export_vm("source", output).unwrap_err() else {
        panic!("metadata error must propagate as I/O error");
    };
    assert_eq!(error.kind(), expected);
    assert_eq!(fs::read(parent).unwrap(), b"caller parent");
}

#[test]
fn concurrent_public_tar_exports_preserve_exactly_one_winner() {
    let fixture = Fixture::new();
    let barrier = Arc::new(Barrier::new(4));
    let output = fixture.0.root().join("winner.tar");
    let mut workers = Vec::new();
    for index in 0..4 {
        let name = format!("source-{index}");
        fixture.source(&name, name.as_bytes());
        let store = VmStore::new(fixture.0.root().to_path_buf());
        let barrier = Arc::clone(&barrier);
        let output = output.clone();
        workers.push(std::thread::spawn(move || {
            barrier.wait();
            (name.clone(), store.export_vm(&name, output))
        }));
    }
    let mut winners = Vec::new();
    for worker in workers {
        match worker.join().unwrap() {
            (name, Ok(_)) => winners.push(name),
            (_, Err(StorageError::ExportAlreadyExists(_))) => {}
            (_, Err(error)) => panic!("unexpected export error: {error}"),
        }
    }
    assert_eq!(winners.len(), 1);
    let mut archive = tar::Archive::new(fs::File::open(output).unwrap());
    let mut disk = None;
    for entry in archive.entries().unwrap() {
        let mut entry = entry.unwrap();
        if entry.path().unwrap().ends_with("disks/root.raw") {
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).unwrap();
            assert!(disk.replace(bytes).is_none());
        }
    }
    assert_eq!(disk.unwrap(), winners[0].as_bytes());
    for index in 0..4 {
        let name = format!("source-{index}");
        let disk = fixture.0.active_disk(&name).unwrap().path;
        assert_eq!(fs::read(disk).unwrap(), name.as_bytes());
    }
}
