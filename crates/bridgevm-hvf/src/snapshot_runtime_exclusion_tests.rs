//! Powered-off create/restore versus the owned runtime's image writer leases.

use super::*;
use crate::media::lock::{MediaLease, MediaLockError};
use crate::snapshot_pair::{restore_snapshot, runtime_exclusion, size::selected_size};
use std::fs;

const IN_USE: &str = "media is in use by a running VM";

/// What a runtime's `prepare` would be told for `image` right now.
fn runtime_refusal(image: &Path) -> Option<String> {
    match MediaLease::acquire(image, "hvf-runner test runtime") {
        Ok(_) => None,
        Err(MediaLockError::Held { holder, .. }) => Some(holder),
        Err(error) => panic!("unexpected lease error: {error}"),
    }
}

/// A runtime that ran on `image` before leaves its (unlocked) lease file.
fn ran_before(image: &Path) {
    drop(MediaLease::acquire(image, "earlier runtime").unwrap());
}

fn refused(result: Result<SnapshotManifest, SnapshotError>, holder: &str) {
    match result {
        Err(SnapshotError::Io(error)) => {
            let message = error.to_string();
            assert_eq!(error.kind(), io::ErrorKind::WouldBlock, "{message}");
            assert!(
                message.contains(IN_USE) && message.contains(holder),
                "{message}"
            );
        }
        other => panic!("powered-off operation admitted under a runtime: {other:?}"),
    }
}

#[test]
fn either_runtime_writer_lease_refuses_create_and_restore_before_any_change() {
    let scratch = Scratch::new("runtime-exclusion-refusal");
    let disk = scratch.write("disk", b"before-disk");
    let vars = scratch.write("vars", b"before-vars");
    let snapshot = scratch.path("snapshot");
    create_snapshot(&disk, &vars, &snapshot, "vm", false, 1024).unwrap();
    fs::write(&disk, b"later-disk").unwrap();
    for image in [&disk, &vars] {
        let runtime = MediaLease::acquire(image, "hvf-runner held").unwrap();
        let output = scratch.path("output");
        refused(
            create_snapshot(&disk, &vars, &output, "vm", false, 1024),
            "hvf-runner held",
        );
        assert!(!output.exists() && !scratch.path(".output.staging").exists());
        let size = selected_size(&disk, &vars).expect_err("create sizing admitted");
        assert!(size.to_string().contains(IN_USE), "{size}");
        refused(
            restore_snapshot(&snapshot, &disk, &vars, false),
            "hvf-runner held",
        );
        let (selected, _) = managed::LockedPair::open(&disk, &vars)
            .unwrap()
            .paths()
            .unwrap();
        assert_eq!(fs::read(selected).unwrap(), b"later-disk");
        drop(runtime);
    }
    restore_snapshot(&snapshot, &disk, &vars, false).unwrap();
}

#[test]
fn a_runtime_cannot_be_prepared_while_a_powered_off_create_runs() {
    let scratch = Scratch::new("runtime-exclusion-create");
    let disk = scratch.write("disk", b"disk");
    let vars = scratch.write("vars", b"vars");
    ran_before(&disk);
    ran_before(&vars);
    let mut during = Vec::new();
    create_snapshot_using(
        &disk,
        &vars,
        &scratch.path("snap"),
        "vm",
        false,
        1024,
        |stage| {
            if stage == CreateStage::DiskSynced {
                during = vec![runtime_refusal(&disk), runtime_refusal(&vars)];
            }
        },
    )
    .unwrap();
    assert_eq!(during.len(), 2);
    for holder in during {
        let holder = holder.expect("runtime admitted during a powered-off create");
        assert!(
            holder.starts_with("powered-off snapshot create (pid"),
            "{holder}"
        );
    }
    assert_eq!(
        (runtime_refusal(&disk), runtime_refusal(&vars)),
        (None, None)
    );
}

#[test]
fn a_runtime_cannot_be_prepared_while_a_powered_off_restore_owns_the_pair() {
    let scratch = Scratch::new("runtime-exclusion-restore");
    let disk = scratch.write("disk", b"disk");
    let vars = scratch.write("vars", b"vars");
    let snapshot = scratch.path("snap");
    create_snapshot(&disk, &vars, &snapshot, "vm", false, 1024).unwrap();
    ran_before(&disk);
    ran_before(&vars);
    let mut owner = runtime_exclusion::open(&disk, &vars, "restore").unwrap();
    for image in [&disk, &vars] {
        let holder = runtime_refusal(image).expect("runtime admitted during a restore");
        assert!(
            holder.starts_with("powered-off snapshot restore (pid"),
            "{holder}"
        );
    }
    owner.pair.restore(&snapshot).unwrap();
    drop(owner);
    assert_eq!(
        (runtime_refusal(&disk), runtime_refusal(&vars)),
        (None, None)
    );
}

#[test]
fn never_run_media_gets_no_lease_file_and_a_generation_still_cannot_start() {
    let scratch = Scratch::new("runtime-exclusion-never-run");
    let disk = scratch.write("disk", b"disk");
    let vars = scratch.write("vars", b"vars");
    let mut generation = None;
    create_snapshot_using(
        &disk,
        &vars,
        &scratch.path("snap"),
        "vm",
        false,
        1024,
        |stage| {
            if stage == CreateStage::DiskSynced {
                generation = Some(managed::LockedPair::open(&disk, &vars).map(|_| ()));
            }
        },
    )
    .unwrap();
    let refusal = generation
        .unwrap()
        .expect_err("a generation opened a pair being copied");
    assert_eq!(refusal.kind(), io::ErrorKind::WouldBlock);
    let snapshot = scratch.path("snap");
    restore_snapshot(&snapshot, &disk, &vars, false).unwrap();
    for entry in fs::read_dir(&scratch.0).unwrap() {
        let name = entry.unwrap().file_name();
        assert!(
            !name.to_string_lossy().ends_with(".bridgevm-writer.lock"),
            "{name:?}"
        );
    }
}

#[test]
fn read_only_volume_lease_errors_are_not_refusals_but_other_errors_are() {
    let io_error = |errno| MediaLockError::Io {
        path: "/ro/disk.raw.bridgevm-writer.lock".into(),
        source: io::Error::from_raw_os_error(errno),
    };
    assert!(matches!(
        runtime_exclusion::refusal(io_error(libc::EROFS)),
        Ok(None)
    ));
    let denied = runtime_exclusion::refusal(io_error(libc::EACCES)).unwrap_err();
    assert_eq!(denied.kind(), io::ErrorKind::PermissionDenied);
    let held = MediaLockError::Held {
        path: "/m/disk.raw".into(),
        holder: "hvf-runner".into(),
    };
    let held = runtime_exclusion::refusal(held).unwrap_err();
    assert_eq!(held.kind(), io::ErrorKind::WouldBlock);
    assert!(held.to_string().contains(IN_USE), "{held}");
}
