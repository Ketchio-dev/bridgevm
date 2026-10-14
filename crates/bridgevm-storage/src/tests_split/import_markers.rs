//! Every active deletion marker shape refuses before publishing a registration.

use super::helpers::{manifest, temp_store};
use crate::*;
use std::fs;
use std::os::unix::fs::{symlink, MetadataExt};
use std::path::PathBuf;
use std::process::Command;

struct Fixture(VmStore, VmStore);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(self.0.root());
        let _ = fs::remove_dir_all(self.1.root());
    }
}

#[test]
fn import_refuses_marker_shapes_with_and_without_rename() {
    for rename in [None, Some("copy")] {
        for shape in [
            "valid",
            "malformed",
            "null",
            "empty",
            "directory",
            "symlink",
            "fifo",
        ] {
            let fixture = Fixture(temp_store(), temp_store());
            let mut manifest = manifest("source");
            manifest.storage.primary.path = "disks/root.raw".into();
            manifest.storage.primary.format = "raw".into();
            let source = fixture.0.create_vm(&manifest).unwrap();
            fs::write(source.join("disks/root.raw"), b"preserved source").unwrap();
            let marker = deletion_metadata_path(&source);
            let target = source.join("untouched-link-target");
            match shape {
                "valid" => {
                    fixture.0.delete_vm_metadata_only("source").unwrap();
                }
                "malformed" => fs::write(&marker, b"{invalid").unwrap(),
                "null" => fs::write(&marker, b"null").unwrap(),
                "empty" => fs::write(&marker, b"").unwrap(),
                "directory" => fs::create_dir(&marker).unwrap(),
                "symlink" => symlink(&target, &marker).unwrap(),
                "fifo" => assert!(Command::new("/usr/bin/mkfifo")
                    .arg(&marker)
                    .status()
                    .unwrap()
                    .success()),
                _ => unreachable!(),
            }
            let original = fs::symlink_metadata(&marker).unwrap();
            let marker_bytes = original.is_file().then(|| fs::read(&marker).unwrap());
            let error = fixture.1.import_vm(&source, rename).unwrap_err();
            assert!(
                error.to_string().contains("metadata/deletion.json"),
                "{shape}: {error}"
            );
            assert!(fixture.1.list_vms().unwrap().is_empty());
            assert_eq!(fs::read_dir(fixture.1.vms_dir()).unwrap().count(), 0);
            let current = fs::symlink_metadata(&marker).unwrap();
            assert_eq!(
                (current.dev(), current.ino()),
                (original.dev(), original.ino())
            );
            if let Some(bytes) = marker_bytes {
                assert_eq!(fs::read(&marker).unwrap(), bytes);
            }
            if shape == "symlink" {
                assert_eq!(fs::read_link(&marker).unwrap(), target);
                assert!(!target.exists());
            }
            assert_eq!(
                fs::read(source.join("disks/root.raw")).unwrap(),
                b"preserved source"
            );
        }
    }
}

#[test]
fn copied_stage_marker_is_rejected_after_source_admission() {
    let fixture = Fixture(temp_store(), temp_store());
    let source = fixture.0.create_vm(&manifest("source")).unwrap();
    crate::import_admission::source(&source).unwrap();
    fixture.1.ensure().unwrap();
    let guard = crate::import_staging::Staging::create(&fixture.1.vms_dir()).unwrap();
    let stage: PathBuf = guard.bundle.clone();
    copy_dir_owned(&source, &stage).unwrap();
    fs::write(deletion_metadata_path(&stage), b"{}").unwrap();
    assert!(crate::import_admission::no_deletion_marker(&stage).is_err());
    drop(guard);
    assert_eq!(fs::read_dir(fixture.1.vms_dir()).unwrap().count(), 0);
    assert!(fixture.0.get_vm("source").is_ok());
}

#[test]
fn non_directory_metadata_error_is_not_treated_as_an_absent_marker() {
    let fixture = Fixture(temp_store(), temp_store());
    let source = fixture.0.create_vm(&manifest("source")).unwrap();
    fs::rename(source.join("metadata"), source.join("metadata-preserved")).unwrap();
    fs::write(source.join("metadata"), b"preserved non-directory metadata").unwrap();
    let before = fs::read(source.join("metadata")).unwrap();
    assert!(matches!(
        fixture.1.import_vm(&source, Some("copy")),
        Err(StorageError::Io(_))
    ));
    assert_eq!(fs::read(source.join("metadata")).unwrap(), before);
    assert_eq!(fs::read_dir(fixture.1.vms_dir()).unwrap().count(), 0);
}
