use crate::snapshot_pair::creation::{create_snapshot_using, stage::CreateStage};
use crate::snapshot_pair::{
    create_snapshot, snapshot_pair_tests::Scratch, verify_snapshot, SnapshotError, SnapshotManifest,
};
use std::fs;
use std::path::Path;

fn assert_refused(result: Result<SnapshotManifest, SnapshotError>) {
    match result {
        Err(SnapshotError::Io(error)) => assert!(
            error.to_string().contains("left intact"),
            "refusal does not say the output was kept: {error}"
        ),
        other => panic!("expected an intact-output refusal, got {other:?}"),
    }
}

fn arrive_after_staging(dest: &Path, make_directory: bool) -> impl FnMut(CreateStage) + '_ {
    move |stage| {
        if stage == CreateStage::StagingDirectorySynced {
            if make_directory {
                fs::create_dir(dest).unwrap();
            }
            fs::write(dest.join("keep.txt"), b"arrived during copy").unwrap();
        }
    }
}

#[test]
fn a_previous_snapshot_that_gains_an_entry_during_copy_is_not_swapped() {
    let s = Scratch::new("recheck-previous-snapshot");
    let disk = s.write("source-disk", b"old disk");
    let vars = s.write("source-vars", b"old vars");
    let dest = s.path("snapshot");
    let old = create_snapshot(&disk, &vars, &dest, "old", false, 1024).unwrap();
    fs::write(&disk, b"new disk").unwrap();
    let result = create_snapshot_using(
        &disk,
        &vars,
        &dest,
        "new",
        false,
        1024,
        arrive_after_staging(&dest, false),
    );
    assert_eq!(
        fs::read(dest.join("keep.txt")).ok().as_deref(),
        Some(&b"arrived during copy"[..]),
        "publication deleted data that arrived after admission"
    );
    assert_refused(result);
    assert_eq!(verify_snapshot(&dest).unwrap(), old);
}

#[test]
fn an_unrelated_directory_created_during_copy_is_not_swapped() {
    let s = Scratch::new("recheck-new-directory");
    let disk = s.write("source-disk", b"new disk");
    let vars = s.write("source-vars", b"new vars");
    let dest = s.path("snapshot");
    let result = create_snapshot_using(
        &disk,
        &vars,
        &dest,
        "new",
        false,
        1024,
        arrive_after_staging(&dest, true),
    );
    assert_eq!(
        fs::read(dest.join("keep.txt")).ok().as_deref(),
        Some(&b"arrived during copy"[..]),
        "publication deleted a directory that appeared after admission"
    );
    assert_eq!(fs::read_dir(&dest).unwrap().count(), 1);
    assert_refused(result);
}
