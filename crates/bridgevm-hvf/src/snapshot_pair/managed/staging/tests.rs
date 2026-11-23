use super::*;
use crate::snapshot_pair::create_snapshot;
use crate::snapshot_pair::files::copy_and_sync;
use crate::snapshot_pair::tests::Scratch;
use std::os::unix::fs::symlink;

#[test]
fn unfamiliar_entries_preserve_all_staging_contents() {
    for kind in ["foreign-file", "nested-directory", "member-symlink"] {
        let scratch = Scratch::new(&format!("managed-stage-shape-{kind}"));
        let staged = scratch.path("staging");
        private_directory(&staged, true).unwrap();
        fs::write(staged.join("disk.raw"), b"known partial copy").unwrap();
        let foreign = scratch.write("outside", b"foreign bytes");
        let name = match kind {
            "foreign-file" => {
                fs::write(staged.join("keep.txt"), b"keep").unwrap();
                "keep.txt"
            }
            "nested-directory" => {
                fs::create_dir(staged.join("vars.fd")).unwrap();
                "vars.fd"
            }
            _ => {
                symlink(&foreign, staged.join("vars.fd")).unwrap();
                "vars.fd"
            }
        };
        let before = fs::symlink_metadata(staged.join(name)).unwrap().file_type();
        let error = clear_contents(&staged).unwrap_err();
        assert!(error.to_string().contains("it was left intact"));
        assert_eq!(
            fs::read(staged.join("disk.raw")).unwrap(),
            b"known partial copy"
        );
        assert_eq!(
            fs::symlink_metadata(staged.join(name)).unwrap().file_type(),
            before
        );
        assert_eq!(fs::read(&foreign).unwrap(), b"foreign bytes");
    }
}

#[test]
fn failed_media_ownership_transfer_removes_only_staging_and_allows_retry() {
    for already_selected in [false, true] {
        let scratch = Scratch::new(&format!("restore-staging-contention-{already_selected}"));
        let disk = scratch.write("disk", b"original disk");
        let vars = scratch.write("vars", b"original vars");
        let source_disk = scratch.write("source-disk", b"snapshot disk");
        let source_vars = scratch.write("source-vars", b"snapshot vars");
        let snapshot = scratch.path("snapshot");
        create_snapshot(&source_disk, &source_vars, &snapshot, "vm").unwrap();
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
            copy_and_sync,
            crate::snapshot_pair::publish::publish,
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

// Publication failure must never clear the old generation after an exchange.

fn contents(directory: &Path) -> [Vec<u8>; 3] {
    ["disk.raw", "vars.fd", "manifest.json"].map(|name| fs::read(directory.join(name)).unwrap())
}

#[test]
fn error_after_exchange_keeps_previous_generation_at_staging() {
    let scratch = Scratch::new("restore-exchange-error-identity");
    let disk = scratch.write("disk", b"old disk");
    let vars = scratch.write("vars", b"old vars");
    let snapshot = scratch.path("snapshot");
    create_snapshot(&disk, &vars, &snapshot, "vm").unwrap();
    let mut pair = LockedPair::open(&disk, &vars).unwrap();
    pair.restore(&snapshot).unwrap();
    let current = pair.paths().unwrap().0.parent().unwrap().to_path_buf();
    fs::write(current.join("disk.raw"), b"guest writes").unwrap();
    let before = contents(&current);
    let staged = current.with_file_name("staging");
    let result = pair.restore_using(&snapshot, |new, old| {
        // Model the successful exchange boundary before directory sync fails.
        let parked = scratch.path("parked");
        fs::rename(old, &parked)?;
        fs::rename(new, old)?;
        fs::rename(parked, new)?;
        Err(io::Error::other("injected post-exchange sync failure"))
    });
    assert!(result.is_err());
    assert_eq!(contents(&staged), before);
    assert_eq!(contents(&current), contents(&snapshot));
    // A crash during old-generation cleanup can leave just a subset of
    // the recorded files. Retry on the same lease that still owns them.
    fs::remove_file(staged.join("disk.raw")).unwrap();
    pair.restore(&snapshot).unwrap();
    assert!(!staged.exists());
    assert_eq!(contents(&current), contents(&snapshot));
}
