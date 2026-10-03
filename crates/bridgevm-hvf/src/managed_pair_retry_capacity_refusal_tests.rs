//! Reclaiming stale copies must preserve every existing refusal boundary.

use super::*;
use std::os::unix::fs::{symlink, MetadataExt, PermissionsExt};

fn refuse_copy(_: &Path, _: &Path) -> io::Result<u64> {
    panic!("refused admission reached copying")
}

fn refuse_publish(_: &Path, _: &Path) -> io::Result<()> {
    panic!("refused admission reached publication")
}

#[test]
fn invalid_or_inside_managed_source_preserves_crash_debris() {
    for inside in [false, true] {
        let tag = format!("managed-retry-source-{inside}");
        let (_scratch, disk, vars, snapshot) = fixture(&tag);
        crash_restore("before-publish", BEFORE_EXIT, &disk, &vars, &snapshot);
        let mut pair = LockedPair::open(&disk, &vars).unwrap();
        let staged = pair.root.join("staging");
        let identity = fs::metadata(&staged).unwrap().ino();
        let source = if inside {
            staged.clone()
        } else {
            fs::write(snapshot.join("disk.raw"), b"bad-disk").unwrap();
            snapshot
        };
        assert!(pair
            .restore_copying(
                &source,
                |_| panic!("invalid source sampled space"),
                refuse_copy,
                refuse_publish
            )
            .is_err());
        assert_eq!(fs::metadata(&staged).unwrap().ino(), identity);
        assert_eq!(fs::read(staged.join("disk.raw")).unwrap(), b"new-disk");
        assert_eq!(
            contents(&pair),
            (b"old-disk".to_vec(), b"old-vars".to_vec())
        );
    }
}

#[test]
fn replaced_root_symlink_refuses_before_cleanup_or_capacity() {
    let (scratch, disk, vars, snapshot) = fixture("managed-retry-root-link");
    let mut pair = LockedPair::open(&disk, &vars).unwrap();
    init::initialize(&pair.root, |_| {}).unwrap();
    let moved = scratch.path("original-root");
    fs::rename(&pair.root, &moved).unwrap();
    let unrelated = scratch.path("unrelated");
    private_directory(&unrelated, true).unwrap();
    let staged = unrelated.join("staging");
    private_directory(&staged, true).unwrap();
    fs::write(staged.join("keep"), b"unrelated bytes").unwrap();
    let inode = fs::metadata(staged.join("keep")).unwrap().ino();
    symlink(&unrelated, &pair.root).unwrap();
    assert!(pair
        .restore_copying(
            &snapshot,
            |_| panic!("replaced root sampled space"),
            refuse_copy,
            refuse_publish
        )
        .is_err());
    assert!(fs::symlink_metadata(&pair.root)
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(fs::metadata(staged.join("keep")).unwrap().ino(), inode);
    assert_eq!(fs::read(staged.join("keep")).unwrap(), b"unrelated bytes");
    assert!(moved.join("original").is_file());
    assert_eq!(fs::read(&disk).unwrap(), b"old-disk");
    assert_eq!(fs::read(&vars).unwrap(), b"old-vars");
}

#[test]
fn unowned_staging_refuses_before_capacity_and_preserves_selection() {
    for selected in [false, true] {
        for kind in ["symlink", "file", "permissive"] {
            let tag = format!("managed-retry-stage-{selected}-{kind}");
            let (scratch, disk, vars, snapshot) = fixture(&tag);
            let mut pair = LockedPair::open(&disk, &vars).unwrap();
            if selected {
                pair.restore(&snapshot).unwrap();
            } else {
                init::initialize(&pair.root, |_| {}).unwrap();
            }
            let old_paths = pair.paths().unwrap();
            let old = contents(&pair);
            let unrelated = scratch.path("unrelated");
            private_directory(&unrelated, true).unwrap();
            fs::write(unrelated.join("keep"), b"unrelated bytes").unwrap();
            let staged = pair.root.join("staging");
            match kind {
                "symlink" => symlink(&unrelated, &staged).unwrap(),
                "file" => fs::write(&staged, b"keep file").unwrap(),
                _ => {
                    fs::create_dir(&staged).unwrap();
                    fs::set_permissions(&staged, fs::Permissions::from_mode(0o755)).unwrap();
                    fs::write(staged.join("keep"), b"keep directory").unwrap();
                }
            }
            let metadata = fs::symlink_metadata(&staged).unwrap();
            assert!(pair
                .restore_copying(
                    &snapshot,
                    |_| panic!("unowned stage sampled space"),
                    refuse_copy,
                    refuse_publish
                )
                .is_err());
            let after = fs::symlink_metadata(&staged).unwrap();
            assert_eq!(
                (after.dev(), after.ino(), after.mode()),
                (metadata.dev(), metadata.ino(), metadata.mode())
            );
            assert_eq!(
                fs::read(unrelated.join("keep")).unwrap(),
                b"unrelated bytes"
            );
            if kind == "file" {
                assert_eq!(fs::read(&staged).unwrap(), b"keep file");
            } else if kind == "permissive" {
                assert_eq!(fs::read(staged.join("keep")).unwrap(), b"keep directory");
            }
            assert_eq!(pair.paths().unwrap(), old_paths);
            assert_eq!(contents(&pair), old);
            assert_eq!(fs::read(&disk).unwrap(), b"old-disk");
            assert_eq!(fs::read(&vars).unwrap(), b"old-vars");
        }
    }
}

#[test]
fn insufficient_post_reclaim_capacity_preserves_original_or_current_pair() {
    for selected in [false, true] {
        let tag = format!("managed-retry-still-full-{selected}");
        let (_scratch, disk, vars, snapshot) = fixture(&tag);
        if selected {
            LockedPair::open(&disk, &vars)
                .unwrap()
                .restore(&snapshot)
                .unwrap();
        }
        crash_restore("before-publish", BEFORE_EXIT, &disk, &vars, &snapshot);
        let mut pair = LockedPair::open(&disk, &vars).unwrap();
        let paths = pair.paths().unwrap();
        let old = contents(&pair);
        let staged = pair.root.join("staging");
        let result = pair.restore_copying(
            &snapshot,
            |_| {
                assert!(!staged.exists(), "stale copy still consumes capacity");
                Some(15)
            },
            refuse_copy,
            refuse_publish,
        );
        assert!(matches!(
            result,
            Err(SnapshotError::InsufficientSpace {
                needed: 16,
                available: 15
            })
        ));
        assert_eq!(pair.paths().unwrap(), paths);
        assert_eq!(contents(&pair), old);
        assert_eq!(fs::read(&disk).unwrap(), b"old-disk");
        assert_eq!(fs::read(&vars).unwrap(), b"old-vars");
        assert!(!staged.exists());
    }
}

#[test]
fn initial_capacity_refusal_does_not_create_managed_storage() {
    let (_scratch, disk, vars, snapshot) = fixture("managed-retry-new-full");
    let mut pair = LockedPair::open(&disk, &vars).unwrap();
    let root = pair.root.clone();
    assert!(!root.exists());
    let result = pair.restore_copying(&snapshot, |_| Some(0), refuse_copy, refuse_publish);
    assert!(matches!(
        result,
        Err(SnapshotError::InsufficientSpace {
            needed: 16,
            available: 0
        })
    ));
    assert!(!root.exists());
    assert_eq!(
        contents(&pair),
        (b"old-disk".to_vec(), b"old-vars".to_vec())
    );
}

#[test]
fn unknown_capacity_still_allows_retry_after_reclaim() {
    let (_scratch, disk, vars, snapshot) = fixture("managed-retry-unknown-space");
    crash_restore("before-publish", BEFORE_EXIT, &disk, &vars, &snapshot);
    let mut pair = LockedPair::open(&disk, &vars).unwrap();
    let staged = pair.root.join("staging");
    pair.restore_with_capacity(
        &snapshot,
        |_| {
            assert!(!staged.exists());
            None
        },
        snapshot_publish::publish,
    )
    .unwrap();
    assert_eq!(
        contents(&pair),
        (b"new-disk".to_vec(), b"new-vars".to_vec())
    );
}
