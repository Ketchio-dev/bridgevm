use super::*;
use crate::snapshot_pair::snapshot_pair_tests::{Scratch, QUOTA};
use crate::snapshot_pair::{
    create_snapshot, managed::LockedPair, restore_snapshot, verify_snapshot, DISK_NAME, VARS_NAME,
};
use std::fs::{self, OpenOptions};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{symlink, OpenOptionsExt};
use std::sync::mpsc;
use std::time::Duration;

#[test]
fn unsafe_disk_and_vars_members_refuse_without_blocking_or_changing_live_media() {
    for name in [DISK_NAME, VARS_NAME] {
        for kind in ["fifo", "symlink", "directory"] {
            let scratch = Scratch::new(&format!("verify-member-{name}-{kind}"));
            let disk = scratch.write("disk", b"snapshot disk");
            let vars = scratch.write("vars", b"snapshot vars");
            let snapshot = scratch.path("snapshot");
            create_snapshot(&disk, &vars, &snapshot, "vm", false, QUOTA).unwrap();
            fs::write(&disk, b"original live disk").unwrap();
            fs::write(&vars, b"original live vars").unwrap();
            restore_snapshot(&snapshot, &disk, &vars, false).unwrap();
            let pair = LockedPair::open(&disk, &vars).unwrap();
            let selected = pair.paths().unwrap();
            fs::write(&selected.0, b"selected live disk").unwrap();
            fs::write(&selected.1, b"selected live vars").unwrap();
            drop(pair);
            let path = snapshot.join(name);
            let bytes = fs::read(&path).unwrap();
            fs::remove_file(&path).unwrap();
            match kind {
                "fifo" => {
                    let path = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
                    // SAFETY: this owned CString is a valid scratch filename for the entire syscall.
                    assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
                }
                "symlink" => {
                    let outside = scratch.write("outside", &bytes);
                    symlink(outside, &path).unwrap();
                }
                "directory" => fs::create_dir(&path).unwrap(),
                _ => unreachable!(),
            }
            // A bounded receive detects a regressed FIFO reader; the writer is
            // held until refusal and released if the test unwinds.
            let anchor = (kind == "fifo").then(|| {
                OpenOptions::new()
                    .read(true)
                    .write(true)
                    .custom_flags(libc::O_NONBLOCK)
                    .open(&path)
                    .unwrap()
            });
            let (send, receive) = mpsc::channel();
            let (worker_disk, worker_vars, worker_snapshot) =
                (disk.clone(), vars.clone(), snapshot.clone());
            let worker = std::thread::spawn(move || {
                let verified = verify_snapshot(&worker_snapshot);
                let mut pair = LockedPair::open(&worker_disk, &worker_vars).unwrap();
                let restored = pair.restore_copying(
                    &worker_snapshot,
                    |_| None,
                    |_, _| panic!("unsafe source reached restore copy"),
                    |_, _| panic!("unsafe source reached publication"),
                );
                send.send((verified, restored)).unwrap();
            });
            let (verified, restored) = receive
                .recv_timeout(Duration::from_secs(3))
                .unwrap_or_else(|error| panic!("{name}/{kind}: verification blocked: {error}"));
            drop(anchor);
            worker.join().unwrap();
            assert!(
                matches!(verified, Err(SnapshotError::Io(_))),
                "{name}/{kind}: unsafe verify result {verified:?}"
            );
            assert!(
                matches!(restored, Err(SnapshotError::Io(_))),
                "{name}/{kind}: unsafe restore result {restored:?}"
            );
            let pair = LockedPair::open(&disk, &vars).unwrap();
            assert_eq!(pair.paths().unwrap(), selected);
            assert!(!selected
                .0
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .join("staging")
                .exists());
            assert_eq!(fs::read(&selected.0).unwrap(), b"selected live disk");
            assert_eq!(fs::read(&selected.1).unwrap(), b"selected live vars");
            assert_eq!(fs::read(&disk).unwrap(), b"original live disk");
            assert_eq!(fs::read(&vars).unwrap(), b"original live vars");
            if kind == "symlink" {
                assert_eq!(fs::read(scratch.path("outside")).unwrap(), bytes);
            }
        }
    }
}

#[test]
fn mismatched_sparse_member_size_is_refused_before_hashing_or_staging() {
    for name in [DISK_NAME, VARS_NAME] {
        let scratch = Scratch::new(&format!("verify-member-size-{name}"));
        let disk = scratch.write("disk", b"disk");
        let vars = scratch.write("vars", b"vars");
        let snapshot = scratch.path("snapshot");
        create_snapshot(&disk, &vars, &snapshot, "vm", false, QUOTA).unwrap();
        // One TiB is sparse; reading it to discover a declared-size mismatch
        // would take far beyond the bounded refusal checkpoint.
        OpenOptions::new()
            .write(true)
            .open(snapshot.join(name))
            .unwrap()
            .set_len(1 << 40)
            .unwrap();
        let (send, receive) = mpsc::channel();
        let mut pair = LockedPair::open(&disk, &vars).unwrap();
        let before = pair.paths().unwrap();
        let worker = std::thread::spawn(move || {
            let result = pair.restore_copying(
                &snapshot,
                |_| None,
                |_, _| panic!("wrong-size source reached copy"),
                |_, _| panic!("wrong-size source reached publication"),
            );
            send.send((result, pair.paths().unwrap())).unwrap();
        });
        let (result, after) = receive
            .recv_timeout(Duration::from_secs(3))
            .unwrap_or_else(|error| panic!("{name}: size refusal hashed the sparse file: {error}"));
        worker.join().unwrap();
        assert!(
            matches!(result, Err(SnapshotError::BadManifest(_))),
            "{name}: {result:?}"
        );
        assert_eq!(after, before);
        assert_eq!(fs::read(&disk).unwrap(), b"disk");
        assert_eq!(fs::read(&vars).unwrap(), b"vars");
    }
}

#[test]
fn selected_digest_keeps_accepting_logical_aliases_after_canonical_selection() {
    let scratch = Scratch::new("selected-digest-logical-link");
    let disk = scratch.write("disk", b"disk");
    let vars = scratch.write("vars", b"vars");
    let alias = scratch.path("disk-alias");
    symlink(&disk, &alias).unwrap();
    let digest = crate::snapshot_pair::selected::selected_digest(&alias, &vars).unwrap();
    assert_eq!((digest.disk_bytes, digest.vars_bytes), (4, 4));
    assert!(fs::symlink_metadata(alias).unwrap().is_symlink());
}
