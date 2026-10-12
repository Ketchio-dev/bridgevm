//! Publication that fails inside a real create, through publish_with's swap.

use crate::snapshot_pair::copy::copy_and_sync;
use crate::snapshot_pair::creation::stopped::create_stopped;
use crate::snapshot_pair::snapshot_pair_tests::{Scratch, QUOTA};
use crate::snapshot_pair::snapshot_publish::publish_with;
use crate::snapshot_pair::*;
use std::path::{Path, PathBuf};
use std::{fs, io};

fn unsupported() -> io::Error {
    // exFAT has no directory swap.
    io::Error::from_raw_os_error(libc::ENOTSUP)
}

fn create_with(
    s: &Scratch,
    vm_id: &str,
    publish: impl FnOnce(&Path, &Path) -> io::Result<()>,
) -> Result<SnapshotManifest, SnapshotError> {
    let (disk, vars, dest) = (s.path("disk"), s.path("vars"), s.path("snapshot"));
    create_stopped(
        &disk,
        &vars,
        &dest,
        vm_id,
        QUOTA,
        copy_and_sync,
        publish,
        |_| {},
    )
}

fn assert_unsupported(result: Result<SnapshotManifest, SnapshotError>) {
    match result {
        Err(SnapshotError::Io(error)) => assert_eq!(error.raw_os_error(), Some(libc::ENOTSUP)),
        other => panic!("expected the swap's own error, got {other:?}"),
    }
}

/// An old snapshot at the destination, so publication must swap; the sources
/// then change so a published replacement would be visible.
fn fixture(tag: &str) -> (Scratch, PathBuf, PathBuf, SnapshotManifest, Vec<u8>) {
    let s = Scratch::new(tag);
    let (disk, vars, dest) = (
        s.write("disk", b"old"),
        s.write("vars", b"v"),
        s.path("snapshot"),
    );
    let old = create_snapshot(&disk, &vars, &dest, "old", false, QUOTA).unwrap();
    fs::write(&disk, b"new").unwrap();
    let manifest = fs::read(dest.join(MANIFEST_NAME)).unwrap();
    let staging = staging_path(&dest);
    (s, dest, staging, old, manifest)
}

fn assert_old_kept(dest: &Path, old: &SnapshotManifest, manifest: &[u8]) {
    assert_eq!(&verify_snapshot(dest).unwrap(), old);
    assert_eq!(fs::read(dest.join(MANIFEST_NAME)).unwrap(), manifest);
    assert_eq!(fs::read(dest.join(DISK_NAME)).unwrap(), b"old");
}

#[test]
fn a_failed_swap_clears_this_attempts_staging_and_a_retry_meets_the_same_error() {
    let (s, dest, staging, old, manifest) = fixture("publication-unsupported");
    for vm_id in ["new", "retry"] {
        let failing = |staged: &Path, current: &Path| {
            assert_eq!(verify_snapshot(staged).unwrap().vm_id, vm_id);
            publish_with(staged, current, |_, _| Err(unsupported()))
        };
        assert_unsupported(create_with(&s, vm_id, failing));
        assert!(
            !staging.exists(),
            "{vm_id}: unpublished staging was stranded"
        );
        assert_old_kept(&dest, &old, &manifest);
    }
    let created = create_snapshot(&s.path("disk"), &s.path("vars"), &dest, "vm", false, QUOTA);
    assert_eq!(verify_snapshot(&dest).unwrap(), created.unwrap());
    assert_eq!(fs::read(dest.join(DISK_NAME)).unwrap(), b"new");
}

#[test]
fn staging_replaced_before_publication_fails_is_not_cleared() {
    let (s, dest, staging, old, manifest) = fixture("publication-replaced-staging");
    let moved = s.path("moved");
    let replace = |staged: &Path, _: &Path| {
        fs::rename(staged, &moved)?;
        // Reclaimable debris by name, so only identity protects it.
        fs::create_dir(staged)?;
        fs::write(staged.join(DISK_NAME), b"foreign")?;
        Err(unsupported())
    };
    assert_unsupported(create_with(&s, "new", replace));
    assert_eq!(fs::read(staging.join(DISK_NAME)).unwrap(), b"foreign");
    assert_eq!(fs::read_dir(&staging).unwrap().count(), 1);
    assert_eq!(verify_snapshot(&moved).unwrap().vm_id, "new");
    assert_old_kept(&dest, &old, &manifest);
}

/// A swap that took effect leaves the replaced snapshot at the staging name;
/// a later failure, such as the parent sync, must not delete it.
#[test]
fn a_failure_after_the_swap_keeps_the_replaced_snapshot() {
    let (s, dest, staging, old, _) = fixture("publication-after-swap");
    let swapped_then_failed = |staged: &Path, current: &Path| {
        publish_with(staged, current, |left, right| {
            let aside = s.path("aside");
            fs::rename(right, &aside)?;
            fs::rename(left, right)?;
            fs::rename(&aside, left)?;
            Err(io::Error::other("injected failure after the swap"))
        })
    };
    assert!(create_with(&s, "new", swapped_then_failed).is_err());
    assert_eq!(verify_snapshot(&staging).unwrap(), old);
    assert_eq!(verify_snapshot(&dest).unwrap().vm_id, "new");
}
