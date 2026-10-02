use super::*;
use crate::snapshot_pair::managed::{private_directory, LockedPair};
use crate::snapshot_pair::snapshot_pair_tests::Scratch;
use crate::snapshot_pair::{copy_and_sync, create_snapshot, snapshot_publish, SnapshotError};

#[test]
fn failed_media_ownership_transfer_removes_only_staging_and_allows_retry() {
    for already_selected in [false, true] {
        let scratch = Scratch::new(&format!("restore-staging-contention-{already_selected}"));
        let disk = scratch.write("disk", b"original disk");
        let vars = scratch.write("vars", b"original vars");
        let source_disk = scratch.write("source-disk", b"snapshot disk");
        let source_vars = scratch.write("source-vars", b"snapshot vars");
        let snapshot = scratch.path("snapshot");
        create_snapshot(&source_disk, &source_vars, &snapshot, "vm", false, 1024).unwrap();
        let mut pair = LockedPair::open(&disk, &vars).unwrap();
        if already_selected {
            pair.restore(&snapshot).unwrap();
            let selected = pair.paths().unwrap();
            fs::write(selected.0, b"live selected disk").unwrap();
            fs::write(selected.1, b"live selected vars").unwrap();
        }
        let selected_before = pair.paths().unwrap();
        let before = (
            fs::read(&selected_before.0).unwrap(),
            fs::read(&selected_before.1).unwrap(),
        );
        let mut competitor = None;
        let mut retained_reader = None;
        let result = pair.restore_copying(
            &snapshot,
            |_| None,
            |source, destination| {
                let copied = copy_and_sync(source, destination)?;
                if destination.file_name().unwrap() == "manifest.json" {
                    let staging = destination.parent().unwrap();
                    competitor = Some(MediaLease::acquire([
                        staging.join("disk.raw").as_path(),
                        staging.join("vars.fd").as_path(),
                    ])?);
                    retained_reader = Some(fs::File::open(staging.join("disk.raw"))?);
                }
                Ok(copied)
            },
            |_, _| panic!("ownership refusal reached publication"),
        );
        let Err(SnapshotError::Io(error)) = result else {
            panic!("missing contention: {result:?}")
        };
        assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
        assert!(
            !pair.root.join("staging").exists(),
            "failed ownership left staged disk behind"
        );
        assert_eq!(pair.paths().unwrap(), selected_before);
        assert_eq!(
            (
                fs::read(&selected_before.0).unwrap(),
                fs::read(&selected_before.1).unwrap()
            ),
            before
        );
        assert_eq!(fs::read(&disk).unwrap(), b"original disk");
        assert_eq!(fs::read(&vars).unwrap(), b"original vars");
        let mut reader = retained_reader.unwrap();
        let mut contents = Vec::new();
        std::io::Read::read_to_end(&mut reader, &mut contents).unwrap();
        assert_eq!(
            contents, b"snapshot disk",
            "cleanup changed a held reader's inode"
        );
        drop(reader);
        drop(competitor);
        pair.restore_copying(
            &snapshot,
            |_| None,
            copy_and_sync,
            snapshot_publish::publish,
        )
        .unwrap();
        let selected = pair.paths().unwrap();
        assert_eq!(fs::read(selected.0).unwrap(), b"snapshot disk");
        assert_eq!(fs::read(selected.1).unwrap(), b"snapshot vars");
    }
}

#[test]
fn cleanup_keeps_a_replacement_directory_or_a_published_generation() {
    for published in [false, true] {
        let scratch = Scratch::new(&format!("restore-staging-replacement-{published}"));
        let staged = scratch.path("staging");
        private_directory(&staged, true).unwrap();
        fs::write(staged.join("disk.raw"), b"owned staging").unwrap();
        let owned = StageOwner::capture(&staged).unwrap();
        let moved = scratch.path(if published { "current" } else { "moved" });
        fs::rename(&staged, &moved).unwrap();
        private_directory(&staged, true).unwrap();
        fs::write(staged.join("keep"), b"replacement data").unwrap();
        owned.clear();
        assert_eq!(fs::read(staged.join("keep")).unwrap(), b"replacement data");
        assert_eq!(fs::read(moved.join("disk.raw")).unwrap(), b"owned staging");
    }
}
