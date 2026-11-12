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
#[cfg(target_os = "macos")]
fn nearly_full_volume_can_restore_by_cloning() {
    let s = Scratch::new("clone-nospace");
    let disk = s.write("disk", b"disk");
    let vars = s.write("vars", b"vars");
    let snapshot = s.path("snap");
    create_snapshot(&disk, &vars, &snapshot, "vm", false, QUOTA).unwrap();
    let mut pair = managed::LockedPair::open(&disk, &vars).unwrap();
    let mut sampled = false;
    pair.restore_copying(
        &snapshot,
        |src, dst| {
            copy::copy_and_sync_using(src, dst, free_space::clone_file, |_| {
                sampled = true;
                Some(0)
            })
        },
        snapshot_publish::publish,
    )
    .unwrap();
    assert!(!sampled, "successful clone must not sample free space");
    let (disk, vars) = pair.paths().unwrap();
    assert_eq!(fs::read(disk).unwrap(), b"disk");
    assert_eq!(fs::read(vars).unwrap(), b"vars");
}

#[test]
fn failed_clone_with_short_space_refuses_before_copy_and_preserves_pair() {
    for selected in [false, true] {
        let s = Scratch::new(&format!("fallback-nospace-{selected}"));
        let disk = s.write("disk", b"disk");
        let vars = s.write("vars", b"vars");
        let snapshot = s.path("snap");
        create_snapshot(&disk, &vars, &snapshot, "vm", false, QUOTA).unwrap();
        let mut pair = managed::LockedPair::open(&disk, &vars).unwrap();
        if selected {
            pair.restore(&snapshot).unwrap();
        }
        let paths = pair.paths().unwrap();
        fs::write(&paths.0, b"live disk").unwrap();
        fs::write(&paths.1, b"live vars").unwrap();
        let mut destination = None;
        let result = pair.restore_copying(
            &snapshot,
            |src, dst| {
                destination = Some(dst.to_path_buf());
                let result = copy::copy_and_sync_using(
                    src,
                    dst,
                    |_, _| None,
                    |parent| {
                        assert_eq!(parent, dst.parent().unwrap());
                        Some(0)
                    },
                );
                assert!(
                    !dst.exists(),
                    "byte copy must not start when space is short"
                );
                result
            },
            |_, _| panic!("short-space copy reached publication"),
        );
        assert_eq!(
            result.unwrap_err().to_string(),
            "No space left on device; free space on your Mac and try again."
        );
        assert!(
            !destination.unwrap().parent().unwrap().exists(),
            "staging not cleared"
        );
        assert_eq!(pair.paths().unwrap(), paths);
        assert_eq!(fs::read(paths.0).unwrap(), b"live disk");
        assert_eq!(fs::read(paths.1).unwrap(), b"live vars");
    }
}

#[test]
fn copy_out_of_space_has_one_plain_sentence() {
    let error = SnapshotError::from(std::io::Error::from_raw_os_error(libc::ENOSPC));
    assert_eq!(
        error.to_string(),
        "No space left on device; free space on your Mac and try again."
    );
}
