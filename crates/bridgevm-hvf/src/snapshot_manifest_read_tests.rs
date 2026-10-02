use super::MANIFEST_LIMIT;
use crate::snapshot_pair::snapshot_pair_tests::{Scratch, QUOTA};
use crate::snapshot_pair::{
    create_snapshot, managed, restore_snapshot, verify_snapshot, SnapshotError, MANIFEST_NAME,
};
use std::fs::{self, OpenOptions};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{symlink, OpenOptionsExt};
use std::sync::mpsc;
use std::time::Duration;

#[test]
fn a_regular_manifest_at_the_limit_verifies_but_larger_files_do_not() {
    let scratch = Scratch::new("manifest-read-size");
    let disk = scratch.write("disk", b"disk");
    let vars = scratch.write("vars", b"vars");
    let snapshot = scratch.path("snapshot");
    let manifest = create_snapshot(&disk, &vars, &snapshot, "vm", false, QUOTA).unwrap();
    let path = snapshot.join(MANIFEST_NAME);
    let mut text = manifest.to_json();
    text.extend(std::iter::repeat_n(
        ' ',
        MANIFEST_LIMIT as usize - text.len(),
    ));
    fs::write(&path, &text).unwrap();
    assert_eq!(verify_snapshot(&snapshot).unwrap(), manifest);
    text.push(' ');
    fs::write(&path, text).unwrap();
    assert!(matches!(
        verify_snapshot(&snapshot),
        Err(SnapshotError::BadManifest(_))
    ));
    OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .set_len(1 << 30)
        .unwrap();
    assert!(matches!(
        verify_snapshot(&snapshot),
        Err(SnapshotError::BadManifest(_))
    ));
}

#[test]
fn unsafe_manifest_files_cannot_change_the_selected_generation_or_original_media() {
    for case in ["symlink", "fifo", "directory", "oversized"] {
        let scratch = Scratch::new(&format!("manifest-read-{case}"));
        let disk = scratch.write("disk", b"snapshot disk");
        let vars = scratch.write("vars", b"snapshot vars");
        let snapshot = scratch.path("snapshot");
        let manifest = create_snapshot(&disk, &vars, &snapshot, "vm", false, QUOTA).unwrap();
        fs::write(&disk, b"original disk remains").unwrap();
        fs::write(&vars, b"original vars remain").unwrap();
        restore_snapshot(&snapshot, &disk, &vars, false).unwrap();
        let pair = managed::LockedPair::open(&disk, &vars).unwrap();
        let selected = pair.paths().unwrap();
        fs::write(&selected.0, b"selected live disk").unwrap();
        fs::write(&selected.1, b"selected live vars").unwrap();
        drop(pair);
        let path = snapshot.join(MANIFEST_NAME);
        fs::remove_file(&path).unwrap();
        match case {
            "symlink" => {
                let target = scratch.write("outside", manifest.to_json().as_bytes());
                symlink(target, &path).unwrap();
            }
            "fifo" => {
                let path = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
                // SAFETY: the CString owns a NUL-terminated scratch path for the entire call.
                assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
            }
            "directory" => fs::create_dir(&path).unwrap(),
            "oversized" => {
                fs::write(&path, manifest.to_json()).unwrap();
                OpenOptions::new()
                    .write(true)
                    .open(&path)
                    .unwrap()
                    .set_len(MANIFEST_LIMIT + 1)
                    .unwrap();
            }
            _ => unreachable!(),
        }
        // Keep a FIFO writer open so a regressed blocking reader can be released after the bounded wait.
        let anchor = (case == "fifo").then(|| {
            OpenOptions::new()
                .read(true)
                .write(true)
                .custom_flags(libc::O_NONBLOCK)
                .open(&path)
                .unwrap()
        });
        let (send, receive) = mpsc::channel();
        let (thread_disk, thread_vars, thread_snapshot) =
            (disk.clone(), vars.clone(), snapshot.clone());
        let worker = std::thread::spawn(move || {
            send.send((
                verify_snapshot(&thread_snapshot),
                restore_snapshot(&thread_snapshot, &thread_disk, &thread_vars, false),
                create_snapshot(
                    &thread_disk,
                    &thread_vars,
                    &thread_snapshot,
                    "new",
                    false,
                    QUOTA,
                ),
            ))
            .unwrap();
        });
        let (verified, restored, replacement) = receive
            .recv_timeout(Duration::from_secs(3))
            .unwrap_or_else(|error| panic!("{case}: manifest refusal blocked: {error}"));
        drop(anchor);
        worker.join().unwrap();
        assert!(
            matches!(verified, Err(SnapshotError::BadManifest(_))),
            "{case}: verify accepted unsafe manifest"
        );
        assert!(
            matches!(restored, Err(SnapshotError::BadManifest(_))),
            "{case}: restore accepted unsafe manifest"
        );
        assert!(
            matches!(replacement, Err(SnapshotError::Io(_))),
            "{case}: replacement accepted unsafe manifest"
        );
        let pair = managed::LockedPair::open(&disk, &vars).unwrap();
        assert_eq!(
            pair.paths().unwrap(),
            selected,
            "{case}: selected generation changed"
        );
        assert_eq!(fs::read(&selected.0).unwrap(), b"selected live disk");
        assert_eq!(fs::read(&selected.1).unwrap(), b"selected live vars");
        assert_eq!(fs::read(&disk).unwrap(), b"original disk remains");
        assert_eq!(fs::read(&vars).unwrap(), b"original vars remain");
    }
}
