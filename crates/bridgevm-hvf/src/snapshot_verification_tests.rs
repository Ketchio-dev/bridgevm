use super::{Scratch, QUOTA};
use crate::snapshot_pair::*;
use std::fs;

#[test]
fn incorrect_declared_sizes_do_not_verify_or_replace_the_live_pair() {
    for (field, bytes) in [
        ("disk_bytes", 0),
        ("disk_bytes", 99),
        ("vars_bytes", 0),
        ("vars_bytes", 99),
    ] {
        let s = Scratch::new(&format!("declared-{field}-{bytes}"));
        let disk = s.write("disk", b"disk");
        let vars = s.write("vars", b"vars");
        let snapshot = s.path("snapshot");
        let manifest = create_snapshot(&disk, &vars, &snapshot, "vm", false, QUOTA).unwrap();
        let changed = manifest
            .to_json()
            .replace(&format!("\"{field}\": 4"), &format!("\"{field}\": {bytes}"));
        assert_ne!(changed, manifest.to_json());
        fs::write(snapshot.join(MANIFEST_NAME), changed).unwrap();
        fs::write(&disk, b"live disk").unwrap();
        fs::write(&vars, b"live vars").unwrap();

        assert!(
            matches!(
                verify_snapshot(&snapshot),
                Err(SnapshotError::BadManifest(_))
            ),
            "{field}={bytes} was accepted"
        );
        assert!(matches!(
            restore_snapshot(&snapshot, &disk, &vars, false),
            Err(SnapshotError::BadManifest(_))
        ));
        let pair = managed::LockedPair::open(&disk, &vars).unwrap();
        assert_eq!(
            pair.paths().unwrap(),
            (disk.canonicalize().unwrap(), vars.canonicalize().unwrap())
        );
        assert_eq!(fs::read(disk).unwrap(), b"live disk");
        assert_eq!(fs::read(vars).unwrap(), b"live vars");
    }
}

#[test]
fn a_full_width_format_version_cannot_wrap_to_the_supported_version() {
    let manifest = SnapshotManifest {
        format_version: SNAPSHOT_FORMAT_VERSION,
        vm_id: "vm".into(),
        disk_bytes: 4,
        disk_sha256: "a".repeat(64),
        vars_bytes: 4,
        vars_sha256: "b".repeat(64),
    };
    for version in [u64::from(u32::MAX) + 2, u64::MAX] {
        let text = manifest.to_json().replace(
            "\"format_version\": 1",
            &format!("\"format_version\": {version}"),
        );
        assert!(
            matches!(
                SnapshotManifest::from_json(&text),
                Err(SnapshotError::BadManifest(_))
            ),
            "unsupported version {version} was accepted"
        );
    }
}

#[test]
fn truthful_snapshot_sizes_allow_restore_at_or_above_available_capacity() {
    for available in [Some(8), Some(9), None] {
        let s = Scratch::new(&format!("capacity-{available:?}"));
        let disk = s.write("disk", b"disk");
        let vars = s.write("vars", b"vars");
        let snapshot = s.path("snapshot");
        create_snapshot(&disk, &vars, &snapshot, "vm", false, QUOTA).unwrap();
        fs::write(&disk, b"changed disk").unwrap();
        fs::write(&vars, b"changed vars").unwrap();
        let mut pair = managed::LockedPair::open(&disk, &vars).unwrap();
        pair.restore_with_capacity(
            &snapshot,
            |_| available,
            crate::snapshot_pair::snapshot_publish::publish,
        )
        .unwrap();
        let (selected_disk, selected_vars) = pair.paths().unwrap();
        assert_eq!(fs::read(selected_disk).unwrap(), b"disk");
        assert_eq!(fs::read(selected_vars).unwrap(), b"vars");
    }
}

#[test]
fn a_restore_that_cannot_fit_its_staging_copy_is_refused_up_front() {
    // Found by a live run, not by reading: restoring a 64 GiB pair needs the
    // volume to hold the live pair, the snapshot AND a full second copy at the
    // same time. It failed partway through with ENOSPC and left a
    // multi-gigabyte temp file behind, which made the next attempt worse.
    let s = Scratch::new("nospace");
    let disk = s.write("disk", b"live disk");
    let vars = s.write("vars", b"live vars");
    create_snapshot(&disk, &vars, &s.path("snap"), "vm", false, QUOTA).unwrap();

    // Use truthful snapshot sizes and inject the capacity of a nearly full
    // volume, so the guard is tested independently of this machine's free space.
    let mut pair = managed::LockedPair::open(&disk, &vars).unwrap();
    let result = pair.restore_with_capacity(
        &s.path("snap"),
        |_| Some(17),
        |_, _| panic!("insufficient capacity must refuse before publication"),
    );
    match result {
        Err(SnapshotError::InsufficientSpace { needed, available }) => {
            assert_eq!((needed, available), (18, 17));
        }
        other => panic!("expected InsufficientSpace, got {other:?}"),
    }
    // And the live pair is untouched, as with any other refusal.
    assert_eq!(
        pair.paths().unwrap(),
        (disk.canonicalize().unwrap(), vars.canonicalize().unwrap())
    );
    assert_eq!(fs::read(&disk).unwrap(), b"live disk");
    assert_eq!(fs::read(&vars).unwrap(), b"live vars");
}
