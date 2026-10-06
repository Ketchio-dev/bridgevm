//! Snapshot names must retain separate catalog, metadata and media identities.

use super::helpers::{manifest, temp_store};
use crate::*;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub(super) fn files_under(directory: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut files = BTreeMap::new();
    for entry in fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files.extend(files_under(&path));
        } else {
            files.insert(path.clone(), fs::read(path).unwrap());
        }
    }
    files
}

#[test]
fn slug_alias_refusal_preserves_disk_snapshot_and_selected_media() {
    let store = temp_store();
    let bundle = store.create_vm(&manifest("dev")).unwrap();
    let primary = store.prepare_primary_disk("dev").unwrap();
    fs::write(&primary.path, b"original backing").unwrap();
    store
        .create_snapshot("dev", "before upgrade", SnapshotKind::Disk)
        .unwrap();
    let disk = store
        .snapshot_disk_metadata("dev", "before upgrade")
        .unwrap()
        .unwrap();
    fs::write(&disk.overlay_path, b"first overlay").unwrap();
    store.create_snapshot_disk("dev", "before upgrade").unwrap();
    let before = files_under(&bundle);

    let result = store.create_snapshot("dev", "before-upgrade", SnapshotKind::Disk);

    assert!(matches!(
        result,
        Err(StorageError::SnapshotAlreadyExists { .. })
    ));
    assert_eq!(files_under(&bundle), before);
    assert_eq!(store.snapshots("dev").unwrap().len(), 1);
    assert_eq!(store.active_disk("dev").unwrap().path, disk.overlay_path);
}

#[test]
fn slug_aliases_are_rejected_across_every_snapshot_kind_before_writes() {
    let kinds = [
        SnapshotKind::Disk,
        SnapshotKind::Suspend,
        SnapshotKind::ApplicationConsistent,
    ];
    for existing_kind in kinds {
        for requested_kind in kinds {
            let store = temp_store();
            let bundle = store.create_vm(&manifest("dev")).unwrap();
            store
                .create_snapshot("dev", "Before Upgrade", existing_kind)
                .unwrap();
            let before = files_under(&bundle);
            for alias in ["before-upgrade", "before___upgrade", " before upgrade "] {
                let result = store.create_snapshot("dev", alias, requested_kind);
                assert!(
                    matches!(result, Err(StorageError::SnapshotAlreadyExists { .. })),
                    "{existing_kind:?}/{requested_kind:?}/{alias}: {result:?}"
                );
                assert_eq!(files_under(&bundle), before);
            }
        }
    }
}

#[test]
fn empty_normalized_snapshot_names_are_rejected_before_writes() {
    for kind in [
        SnapshotKind::Disk,
        SnapshotKind::Suspend,
        SnapshotKind::ApplicationConsistent,
    ] {
        let store = temp_store();
        let bundle = store.create_vm(&manifest("dev")).unwrap();
        let before = files_under(&bundle);
        for name in ["", "  ", "---", "../", "스냅샷", "🧪"] {
            let result = store.create_snapshot("dev", name, kind);
            assert!(
                matches!(result, Err(StorageError::Io(ref error))
                if error.kind() == std::io::ErrorKind::InvalidInput),
                "{kind:?}/{name}: {result:?}"
            );
            assert_eq!(files_under(&bundle), before);
        }
    }
}

#[test]
fn distinct_snapshot_names_keep_disk_metadata_separate_from_creation_receipts() {
    let store = temp_store();
    let bundle = store.create_vm(&manifest("dev")).unwrap();
    let primary = store.prepare_primary_disk("dev").unwrap();
    fs::write(&primary.path, b"original backing").unwrap();
    store
        .create_snapshot("dev", "base", SnapshotKind::Disk)
        .unwrap();
    store
        .create_snapshot("dev", "base-create", SnapshotKind::Disk)
        .unwrap();
    let second_path = snapshot_disk_metadata_path(&bundle, "base-create");
    let second_before = fs::read(&second_path).unwrap();
    let first = store
        .snapshot_disk_metadata("dev", "base")
        .unwrap()
        .unwrap();
    fs::write(&first.overlay_path, b"first overlay").unwrap();

    store.create_snapshot_disk("dev", "base").unwrap();

    assert_eq!(fs::read(&second_path).unwrap(), second_before);
    assert_eq!(
        store
            .snapshot_disk_metadata("dev", "base-create")
            .unwrap()
            .unwrap()
            .snapshot,
        "base-create"
    );
    let receipt = snapshot_disk_create_metadata_path(&bundle, "base");
    let created: SnapshotDiskCreateMetadata = read_json_required(&receipt).unwrap();
    assert_eq!(created.snapshot, "base");
    assert_ne!(receipt, second_path);
    assert_eq!(store.snapshots("dev").unwrap().len(), 2);
}

#[test]
fn snapshot_metadata_aliases_do_not_select_or_activate_another_name() {
    let store = temp_store();
    let bundle = store.create_vm(&manifest("dev")).unwrap();
    let primary = store.prepare_primary_disk("dev").unwrap();
    fs::write(&primary.path, b"backing").unwrap();
    store
        .create_snapshot("dev", "before upgrade", SnapshotKind::Disk)
        .unwrap();
    let disk = store
        .snapshot_disk_metadata("dev", "before upgrade")
        .unwrap()
        .unwrap();
    fs::write(&disk.overlay_path, b"overlay").unwrap();
    store
        .create_snapshot("dev", "paused state", SnapshotKind::Suspend)
        .unwrap();
    store
        .create_snapshot("dev", "app ready", SnapshotKind::ApplicationConsistent)
        .unwrap();
    let before = files_under(&bundle);

    assert!(store
        .snapshot_disk_metadata("dev", "before-upgrade")
        .unwrap()
        .is_none());
    assert!(store
        .snapshot_suspend_image_metadata("dev", "paused-state")
        .unwrap()
        .is_none());
    assert!(store
        .application_consistent_snapshot_preflight_metadata("dev", "app-ready")
        .unwrap()
        .is_none());
    assert!(matches!(
        store.create_snapshot_disk("dev", "before-upgrade"),
        Err(StorageError::SnapshotDiskMetadataNotFound { .. })
    ));
    assert_eq!(files_under(&bundle), before);
    assert_eq!(store.snapshots("dev").unwrap().len(), 3);
}

#[test]
fn creation_suffix_snapshot_stays_visible_and_clones_as_disk_metadata() {
    let store = temp_store();
    store.create_vm(&manifest("dev")).unwrap();
    store
        .create_snapshot("dev", "base", SnapshotKind::Disk)
        .unwrap();
    store
        .create_snapshot("dev", "base-create", SnapshotKind::Disk)
        .unwrap();
    let names = store
        .snapshot_chain("dev")
        .unwrap()
        .disks
        .into_iter()
        .map(|disk| disk.snapshot)
        .collect::<Vec<_>>();
    assert_eq!(names, ["base", "base-create"]);
    let clone = store.clone_vm("dev", "copy", false).unwrap();
    let disk = store
        .snapshot_disk_metadata("copy", "base-create")
        .unwrap()
        .unwrap();
    assert!(disk.overlay_path.starts_with(clone.output));
}

#[test]
fn concurrent_snapshot_aliases_admit_only_one_catalog_entry() {
    let store = temp_store();
    store.create_vm(&manifest("dev")).unwrap();
    let first_store = store.clone();
    let second_store = store.clone();
    let first = std::thread::spawn(move || {
        first_store.create_snapshot("dev", "Before Upgrade", SnapshotKind::Disk)
    });
    let second = std::thread::spawn(move || {
        second_store.create_snapshot("dev", "before-upgrade", SnapshotKind::Suspend)
    });
    let results = [first.join().unwrap(), second.join().unwrap()];
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(store.snapshots("dev").unwrap().len(), 1);
}
