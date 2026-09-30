use super::*;
use crate::snapshot_pair::copy::{stream_and_sync, COPY_CHUNK};
use crate::snapshot_pair::{snapshot_pair_tests::Scratch, verify_snapshot};
use libc::ENOSPC;

fn fixture(tag: &str) -> (Scratch, PathBuf, PathBuf, PathBuf, SnapshotManifest) {
    let s = Scratch::new(tag);
    let disk = s.write("disk", b"old-disk");
    let vars = s.write("vars", b"old-vars");
    let dest = s.path("snapshot");
    let old = create_snapshot(&disk, &vars, &dest, "old", false, 1024).unwrap();
    fs::write(&disk, b"new-disk").unwrap();
    (s, disk, vars, dest, old)
}

/// The vars source vanishing once the disk is staged fails the second copy
/// while staging already holds a complete disk copy.
#[test]
fn a_copy_error_after_staging_is_claimed_removes_staging() {
    let (_s, disk, vars, dest, old) = fixture("create-copy-error");
    let result = create_snapshot_using(&disk, &vars, &dest, "new", false, 1024, |stage| {
        if stage == CreateStage::DiskSynced {
            fs::remove_file(&vars).unwrap();
        }
    });
    match result {
        Err(SnapshotError::Io(error)) => assert_eq!(error.kind(), io::ErrorKind::NotFound),
        other => panic!("expected the vars copy error, got {other:?}"),
    }
    assert!(
        !staging_path(&dest).exists(),
        "failed copy stranded staging"
    );
    assert_eq!(verify_snapshot(&dest).unwrap(), old);
    fs::write(&vars, b"new-vars").unwrap();
    let retried = create_snapshot(&disk, &vars, &dest, "retry", false, 1024).unwrap();
    assert_eq!(verify_snapshot(&dest).unwrap(), retried);
}

/// Clearing after a failure follows debris rules: foreign content is kept.
#[test]
fn a_copy_error_keeps_staging_that_gained_foreign_content() {
    let (_s, disk, vars, dest, old) = fixture("create-copy-error-foreign");
    let staging = staging_path(&dest);
    let result = create_snapshot_using(&disk, &vars, &dest, "new", false, 1024, |stage| {
        if stage == CreateStage::DiskSynced {
            fs::write(staging.join("keep.txt"), b"arrived during copy").unwrap();
            fs::remove_file(&vars).unwrap();
        }
    });
    assert!(matches!(result, Err(SnapshotError::Io(_))), "{result:?}");
    assert_eq!(
        fs::read(staging.join("keep.txt")).ok().as_deref(),
        Some(&b"arrived during copy"[..]),
        "failure cleanup deleted foreign staging content"
    );
    assert_eq!(fs::read(staging.join(DISK_NAME)).unwrap(), b"new-disk");
    assert_eq!(verify_snapshot(&dest).unwrap(), old);
}

/// Export to another volume streams; a full volume fails it after one chunk
/// with a partial disk copy in staging.
#[test]
fn a_full_volume_during_a_streaming_copy_removes_the_partial_copy() {
    let (s, _, vars, dest, old) = fixture("create-copy-full-volume");
    let disk = s.write("disk", &vec![7; COPY_CHUNK + 4096]);
    let mut partial = None;
    let full = |src: &Path, dst: &Path| {
        let copied = stream_and_sync(src, dst, |_| Err(io::Error::from_raw_os_error(ENOSPC)));
        partial = fs::metadata(dst).ok().map(|metadata| metadata.len());
        copied
    };
    let result = create_stopped(&disk, &vars, &dest, "new", u64::MAX, full, |_| {});
    assert_eq!(
        partial,
        Some(COPY_CHUNK as u64),
        "copy did not stop mid-disk"
    );
    match result {
        Err(SnapshotError::Io(error)) => assert_eq!(error.raw_os_error(), Some(ENOSPC)),
        other => panic!("expected the full-volume error, got {other:?}"),
    }
    assert!(
        !staging_path(&dest).exists(),
        "failed copy stranded staging"
    );
    assert_eq!(verify_snapshot(&dest).unwrap(), old);
    let stream = |src: &Path, dst: &Path| stream_and_sync(src, dst, |_| Ok(()));
    let retried = create_stopped(&disk, &vars, &dest, "retry", u64::MAX, stream, |_| {});
    assert_eq!(verify_snapshot(&dest).unwrap(), retried.unwrap());
}
