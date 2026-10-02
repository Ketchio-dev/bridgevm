use super::*;
use crate::snapshot_pair::creation::{create_snapshot_using, stage::CreateStage};
use crate::snapshot_pair::snapshot_pair_tests::{Scratch, QUOTA};
use crate::snapshot_pair::{
    create_snapshot, managed::LockedPair, restore_snapshot, staging_path, verify_snapshot,
    DISK_NAME, MANIFEST_NAME, VARS_NAME,
};
use std::fs;

#[test]
fn growth_after_source_admission_cannot_publish_a_pair_above_quota() {
    for managed in [false, true] {
        let scratch = Scratch::new(&format!("create-copied-quota-{managed}"));
        let disk = scratch.write("disk", b"disk");
        let vars = scratch.write("vars", b"vars");
        let destination = scratch.path("snapshot");
        let old = create_snapshot(&disk, &vars, &destination, "old", false, QUOTA).unwrap();
        if managed {
            restore_snapshot(&destination, &disk, &vars, false).unwrap();
        }
        let pair = LockedPair::open(&disk, &vars).unwrap();
        let selected = pair.paths().unwrap();
        drop(pair);
        let before = [DISK_NAME, VARS_NAME, MANIFEST_NAME]
            .map(|name| fs::read(destination.join(name)).unwrap());
        for output in [destination.clone(), scratch.path("absent")] {
            fs::write(&selected.1, b"vars").unwrap();
            let mut stages = Vec::new();
            let result = create_snapshot_using(&disk, &vars, &output, "new", false, 8, |stage| {
                stages.push(stage);
                if stage == CreateStage::DiskSynced {
                    fs::write(&selected.1, b"grown vars").unwrap();
                }
            });
            assert!(
                matches!(
                    result,
                    Err(SnapshotError::QuotaExceeded {
                        bytes: 14,
                        quota: 8
                    })
                ),
                "{result:?}"
            );
            assert!(!stages.contains(&CreateStage::ManifestPublished));
            assert!(!stages.contains(&CreateStage::SnapshotPublished));
            assert!(!staging_path(&output).exists());
            if output != destination {
                assert!(!output.exists());
            }
            assert_eq!(verify_snapshot(&destination).unwrap(), old);
            assert_eq!(
                [DISK_NAME, VARS_NAME, MANIFEST_NAME]
                    .map(|name| fs::read(destination.join(name)).unwrap()),
                before
            );
            let pair = LockedPair::open(&disk, &vars).unwrap();
            assert_eq!(pair.paths().unwrap(), selected);
            assert_eq!(fs::read(&selected.0).unwrap(), b"disk");
            assert_eq!(fs::read(&selected.1).unwrap(), b"grown vars");
            assert_eq!(fs::read(&disk).unwrap(), b"disk");
            assert_eq!(
                fs::read(&vars).unwrap(),
                if managed {
                    b"vars".as_slice()
                } else {
                    b"grown vars".as_slice()
                }
            );
        }
    }
}

#[test]
fn copied_growth_exactly_at_quota_remains_valid() {
    let scratch = Scratch::new("create-copied-exact-quota");
    let disk = scratch.write("disk", b"disk");
    let vars = scratch.write("vars", b"vars");
    let destination = scratch.path("snapshot");
    let manifest = create_snapshot_using(&disk, &vars, &destination, "vm", false, 14, |stage| {
        if stage == CreateStage::DiskSynced {
            fs::write(&vars, b"grown vars").unwrap();
        }
    })
    .unwrap();
    assert_eq!((manifest.disk_bytes, manifest.vars_bytes), (4, 10));
    assert_eq!(verify_snapshot(&destination).unwrap(), manifest);
}

#[test]
fn overflowing_pair_sizes_fail_closed_and_exact_full_width_size_is_allowed() {
    let result = admit(u64::MAX, 1, u64::MAX);
    let Err(SnapshotError::Io(error)) = result else {
        panic!("size overflow accepted: {result:?}");
    };
    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    assert_eq!(error.to_string(), "snapshot byte count overflow");
    admit(u64::MAX, 0, u64::MAX).unwrap();
}
