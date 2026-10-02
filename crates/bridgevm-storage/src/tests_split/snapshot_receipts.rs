//! Clone rebasing and exact receipt preservation by metadata-only import.

use super::helpers::{manifest, temp_store};
use super::snapshot_names::files_under;
use crate::*;
use std::fs;
use std::os::unix::process::ExitStatusExt;
use std::process::Output;

fn create_present_overlay(store: &VmStore, name: &str) {
    let disk = store.snapshot_disk_metadata("dev", name).unwrap().unwrap();
    fs::write(&disk.overlay_path, b"synthetic overlay").unwrap();
    store.create_snapshot_disk("dev", name).unwrap();
}

fn store_with_receipt() -> VmStore {
    let store = temp_store();
    store.create_vm(&manifest("dev")).unwrap();
    let primary = store.prepare_primary_disk("dev").unwrap();
    fs::write(&primary.path, b"synthetic backing").unwrap();
    store
        .create_snapshot("dev", "base", SnapshotKind::Disk)
        .unwrap();
    create_present_overlay(&store, "base");
    store
}

fn make_legacy_receipt(store: &VmStore) -> std::path::PathBuf {
    let bundle = store.bundle_path("dev");
    let current = snapshot_disk_create_metadata_path(&bundle, "base");
    let legacy = bundle.join("metadata/snapshot-disks/base-create.json");
    fs::rename(current, &legacy).unwrap();
    legacy
}

#[test]
fn legacy_receipt_occupancy_refuses_new_snapshot_without_overwriting() {
    let store = store_with_receipt();
    make_legacy_receipt(&store);
    let bundle = store.bundle_path("dev");
    let before = files_under(&bundle);

    let result = store.create_snapshot("dev", "base-create", SnapshotKind::Disk);

    assert!(matches!(result, Err(StorageError::Io(ref error))
        if error.kind() == std::io::ErrorKind::AlreadyExists));
    assert_eq!(files_under(&bundle), before);
    assert_eq!(store.snapshots("dev").unwrap().len(), 1);
}

#[test]
fn current_receipts_preserve_suffix_names_in_both_creation_orders() {
    for names in [["base", "base-create"], ["base-create", "base"]] {
        let store = temp_store();
        let bundle = store.create_vm(&manifest("dev")).unwrap();
        let primary = store.prepare_primary_disk("dev").unwrap();
        fs::write(&primary.path, b"backing").unwrap();
        for name in names {
            store
                .create_snapshot("dev", name, SnapshotKind::Disk)
                .unwrap();
            create_present_overlay(&store, name);
        }
        for name in names {
            let other = if name == "base" {
                "base-create"
            } else {
                "base"
            };
            let other_path = snapshot_disk_metadata_path(&bundle, other);
            let before = fs::read(&other_path).unwrap();
            store.create_snapshot_disk("dev", name).unwrap();
            assert_eq!(fs::read(other_path).unwrap(), before);
        }
        assert_eq!(store.snapshot_chain("dev").unwrap().disks.len(), 2);
        let cloned = store.clone_vm("dev", "copy", false).unwrap().output;
        for name in names {
            let metadata = store.snapshot_disk_metadata("copy", name).unwrap().unwrap();
            assert!(metadata.overlay_path.starts_with(&cloned));
            let receipt: SnapshotDiskCreateMetadata =
                read_json_required(&snapshot_disk_create_metadata_path(&cloned, name)).unwrap();
            assert!(receipt.disk.overlay_path.starts_with(&cloned));
            assert!(receipt.disk.backing_path.starts_with(&cloned));
        }
        let exported = store.root().join("export.vmbridge");
        store.export_vm("dev", &exported).unwrap();
        let target = temp_store();
        let imported = target
            .import_vm(&exported, Some("imported"))
            .unwrap()
            .output;
        assert_eq!(target.snapshot_chain("imported").unwrap().disks.len(), 2);
        for name in names {
            let source_receipt = snapshot_disk_create_metadata_path(&bundle, name);
            let imported_receipt = snapshot_disk_create_metadata_path(&imported, name);
            let receipt: SnapshotDiskCreateMetadata =
                read_json_required(&imported_receipt).unwrap();
            assert_eq!(receipt.snapshot, name);
            assert_eq!(
                fs::read(imported_receipt).unwrap(),
                fs::read(source_receipt).unwrap()
            );
            assert_eq!(
                target
                    .snapshot_disk_metadata("imported", name)
                    .unwrap()
                    .unwrap()
                    .snapshot,
                name
            );
        }
    }
}

#[test]
fn legacy_receipts_rebase_in_clone_and_import_preserves_original_metadata() {
    let store = store_with_receipt();
    let legacy = make_legacy_receipt(&store);
    let original = fs::read(&legacy).unwrap();
    assert_eq!(store.snapshot_chain("dev").unwrap().disks.len(), 1);
    let cloned = store.clone_vm("dev", "copy", false).unwrap().output;
    let receipt: SnapshotDiskCreateMetadata =
        read_json_required(&cloned.join("metadata/snapshot-disks/base-create.json")).unwrap();
    assert!(receipt.disk.overlay_path.starts_with(cloned));
    assert_eq!(fs::read(&legacy).unwrap(), original);
    let exported = store.root().join("export.vmbridge");
    store.export_vm("dev", &exported).unwrap();
    let target = temp_store();
    let imported = target
        .import_vm(&exported, Some("imported"))
        .unwrap()
        .output;
    let imported_receipt = imported.join("metadata/snapshot-disks/base-create.json");
    let receipt: SnapshotDiskCreateMetadata = read_json_required(&imported_receipt).unwrap();
    assert_eq!(receipt.snapshot, "base");
    assert_eq!(fs::read(imported_receipt).unwrap(), original);
    assert_eq!(
        target.snapshot_disk_metadata("imported", "base").unwrap(),
        store.snapshot_disk_metadata("dev", "base").unwrap()
    );
    assert_eq!(fs::read(legacy).unwrap(), original);
}

#[test]
fn linked_clone_drops_current_and_legacy_snapshot_receipts() {
    let store = store_with_receipt();
    let current = snapshot_disk_create_metadata_path(&store.bundle_path("dev"), "base");
    let legacy = store
        .bundle_path("dev")
        .join("metadata/snapshot-disks/base-create.json");
    fs::copy(&current, &legacy).unwrap();
    let cloned = store
        .clone_vm_with("dev", "linked", true, |_program, args| {
            fs::write(&args[7], b"synthetic linked overlay")?;
            Ok(Output {
                status: std::process::ExitStatus::from_raw(0),
                stdout: Vec::new(),
                stderr: Vec::new(),
            })
        })
        .unwrap()
        .output;
    assert!(!cloned.join("metadata/snapshot-disks").exists());
    assert!(current.exists());
    assert!(legacy.exists());
}

#[test]
fn malformed_disk_records_and_receipts_fail_clone_without_publishing() {
    for relative in [
        "metadata/snapshot-disks/invalid.json",
        "metadata/snapshot-disks/creates/invalid.json",
    ] {
        let store = store_with_receipt();
        fs::write(store.bundle_path("dev").join(relative), b"{}").unwrap();
        assert!(matches!(
            store.clone_vm("dev", "copy", false),
            Err(StorageError::Json(_))
        ));
        assert!(!store.bundle_path("copy").exists());
    }
}
