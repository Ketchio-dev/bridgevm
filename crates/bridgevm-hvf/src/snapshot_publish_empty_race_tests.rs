//! Real rename refuses a late entry; publication never falls back to exchange.

use super::*;
use std::os::unix::fs::MetadataExt;

fn unexpected_exchange(_: &Path, _: &Path) -> io::Result<()> {
    panic!("the inspected empty destination must remain on the rename branch")
}

#[test]
fn a_sentinel_arriving_after_the_empty_check_is_preserved() {
    let scratch = Scratch::new("publish-empty-sentinel");
    let new = snapshot(&scratch, "stage", b"new-pair");
    let stage = scratch.path("stage");
    let dest = scratch.path("dest");
    fs::create_dir(&dest).unwrap();
    let inode = fs::symlink_metadata(&dest).unwrap().ino();
    let result = publish_using(
        &stage,
        &dest,
        unexpected_exchange,
        |a, b| {
            fs::write(b.join("keep.txt"), b"caller sentinel")?;
            fs::rename(a, b)
        },
        crate::snapshot_pair::sync_dir,
    );
    let error = result.unwrap_err();
    assert!(matches!(
        error.raw_os_error(),
        Some(libc::ENOTEMPTY) | Some(libc::EEXIST)
    ));
    assert_eq!(fs::symlink_metadata(&dest).unwrap().ino(), inode);
    assert_eq!(fs::read(dest.join("keep.txt")).unwrap(), b"caller sentinel");
    assert_eq!(fs::read_dir(&dest).unwrap().count(), 1);
    assert_eq!(verify_snapshot(&stage).unwrap(), new);
}

#[test]
fn rename_publication_does_not_delete_a_recreated_staging_path() {
    for empty in [true, false] {
        let scratch = Scratch::new(&format!("publish-recreated-stage-empty-{empty}"));
        let new = snapshot(&scratch, "stage", b"new-pair");
        let stage = scratch.path("stage");
        let dest = scratch.path("dest");
        if empty {
            fs::create_dir(&dest).unwrap();
        }
        publish_using(
            &stage,
            &dest,
            unexpected_exchange,
            |a, b| {
                fs::rename(a, b)?;
                fs::create_dir(a)?;
                fs::write(a.join("keep.txt"), b"new staging owner")
            },
            crate::snapshot_pair::sync_dir,
        )
        .unwrap();
        assert_eq!(verify_snapshot(&dest).unwrap(), new);
        assert_eq!(
            fs::read(stage.join("keep.txt")).unwrap(),
            b"new staging owner"
        );
    }
}

#[test]
fn failed_parent_sync_after_empty_rename_retains_the_published_pair() {
    let scratch = Scratch::new("publish-empty-sync-failure");
    let new = snapshot(&scratch, "stage", b"new-pair");
    let stage = scratch.path("stage");
    let dest = scratch.path("dest");
    fs::create_dir(&dest).unwrap();
    let result = publish_using(
        &stage,
        &dest,
        unexpected_exchange,
        |a, b| fs::rename(a, b),
        |_| Err(io::Error::other("injected parent sync failure")),
    );
    assert_eq!(
        result.unwrap_err().to_string(),
        "injected parent sync failure"
    );
    assert_eq!(verify_snapshot(&dest).unwrap(), new);
    assert!(!stage.exists());
}
