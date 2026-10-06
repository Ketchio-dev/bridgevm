use super::*;
use crate::snapshot_pair::creation::{create_snapshot_using, stage::CreateStage};
use crate::snapshot_pair::snapshot_pair_tests::{Scratch, QUOTA};
use crate::snapshot_pair::{create_snapshot, staging_path, verify_snapshot, MANIFEST_NAME};

fn available_vm_id_bytes() -> usize {
    MANIFEST_LIMIT as usize
        - SnapshotManifest {
            format_version: SNAPSHOT_FORMAT_VERSION,
            vm_id: String::new(),
            disk_bytes: 4,
            disk_sha256: "0".repeat(64),
            vars_bytes: 4,
            vars_sha256: "0".repeat(64),
        }
        .to_json()
        .len()
}

#[test]
fn oversized_plain_and_escaped_vm_ids_refuse_before_any_staging_or_replacement() {
    for (name, vm_id) in [
        ("ascii", "x".repeat(MANIFEST_LIMIT as usize)),
        ("controls", "\n".repeat(MANIFEST_LIMIT as usize / 6 + 1)),
    ] {
        let scratch = Scratch::new(&format!("create-manifest-limit-{name}"));
        let disk = scratch.write("disk", b"disk");
        let vars = scratch.write("vars", b"vars");
        let snapshot = scratch.path("snapshot");
        let old = create_snapshot(&disk, &vars, &snapshot, "old", false, QUOTA).unwrap();
        let old_text = fs::read(snapshot.join(MANIFEST_NAME)).unwrap();
        for destination in [snapshot.clone(), scratch.path("new-parent/absent")] {
            let result =
                create_snapshot_using(&disk, &vars, &destination, &vm_id, false, QUOTA, |stage| {
                    panic!("metadata refusal came after {stage:?}")
                });
            let Err(SnapshotError::Io(error)) = result else {
                panic!("accepted unreadable {name} manifest: {result:?}");
            };
            assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
            assert!(!staging_path(&destination).exists());
        }
        assert!(!scratch.path("new-parent").exists());
        assert_eq!(verify_snapshot(&snapshot).unwrap(), old);
        assert_eq!(fs::read(snapshot.join(MANIFEST_NAME)).unwrap(), old_text);
        assert_eq!(fs::read(&disk).unwrap(), b"disk");
        assert_eq!(fs::read(&vars).unwrap(), b"vars");
        assert!(matches!(
            create_snapshot(&disk, &vars, &snapshot, &vm_id, false, 7),
            Err(SnapshotError::QuotaExceeded { bytes: 8, quota: 7 })
        ));
    }
}

#[test]
fn exactly_bounded_plain_and_escaped_manifests_round_trip_after_creation() {
    let available = available_vm_id_bytes();
    let controls = "\0".repeat(available / 6) + &"x".repeat(available % 6);
    for (name, vm_id) in [("ascii", "x".repeat(available)), ("controls", controls)] {
        let scratch = Scratch::new(&format!("create-manifest-boundary-{name}"));
        let disk = scratch.write("disk", b"disk");
        let vars = scratch.write("vars", b"vars");
        let snapshot = scratch.path("snapshot");
        let manifest = create_snapshot(&disk, &vars, &snapshot, &vm_id, false, QUOTA).unwrap();
        assert_eq!(
            fs::metadata(snapshot.join(MANIFEST_NAME)).unwrap().len(),
            MANIFEST_LIMIT
        );
        assert_eq!(verify_snapshot(&snapshot).unwrap(), manifest);
        assert_eq!(manifest.vm_id, vm_id);
    }
}

#[test]
fn final_serialized_size_is_rechecked_and_failed_fill_is_removed() {
    let scratch = Scratch::new("create-manifest-final-limit");
    let disk = scratch.write("disk", b"disk");
    let vars = scratch.write("vars", b"vars");
    let snapshot = scratch.path("snapshot");
    let old = create_snapshot(&disk, &vars, &snapshot, "old", false, QUOTA).unwrap();
    let vm_id = "x".repeat(available_vm_id_bytes());
    let result = create_snapshot_using(&disk, &vars, &snapshot, &vm_id, false, QUOTA, |stage| {
        if stage == CreateStage::DiskSynced {
            // A changed digit count grows the final serialized metadata by one
            // byte without changing the preflighted vm_id.
            fs::write(&vars, b"ten bytes!").unwrap();
        }
    });
    let Err(SnapshotError::Io(error)) = result else {
        panic!("accepted unreadable final metadata: {result:?}");
    };
    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    assert!(!staging_path(&snapshot).exists());
    assert_eq!(verify_snapshot(&snapshot).unwrap(), old);
}
