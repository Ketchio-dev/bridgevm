use super::*;
use crate::snapshot_pair::{snapshot_pair_tests::Scratch, verify_snapshot};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::Duration;

#[derive(Debug)]
enum SecondEvent {
    DiskSynced,
    Finished,
}

fn assert_contention(
    tag: &str,
    second_name: &str,
    final_case_alias: bool,
    parent_case_alias: bool,
) {
    let scratch = Scratch::new(tag);
    let destination = scratch.path("snapshot");
    let old_disk = scratch.write("old-disk", b"old-disk");
    let old_vars = scratch.write("old-vars", b"old-vars");
    create_snapshot(&old_disk, &old_vars, &destination, "old", false, 1024).unwrap();
    let second_destination = if parent_case_alias {
        let parent = destination.parent().unwrap();
        let alias_name = parent.file_name().unwrap().to_string_lossy().to_uppercase();
        parent.with_file_name(alias_name).join(second_name)
    } else {
        scratch.path(second_name)
    };
    if (final_case_alias && !second_destination.is_dir())
        || (parent_case_alias && !second_destination.parent().unwrap().is_dir())
    {
        return; // This fixture volume has case-sensitive names.
    }
    let first_disk = scratch.write("first-disk", b"first-disk");
    let first_vars = scratch.write("first-vars", b"first-vars");
    let second_disk = scratch.write("second-disk", b"other-disk");
    let second_vars = scratch.write("second-vars", b"second-vars");

    let (first_staged_tx, first_staged_rx) = mpsc::channel();
    let (first_resume_tx, first_resume_rx) = mpsc::channel();
    let first_destination = destination.clone();
    let first = thread::spawn(move || {
        create_snapshot_using(
            &first_disk,
            &first_vars,
            &first_destination,
            "first",
            false,
            1024,
            |stage| {
                if stage == CreateStage::DiskSynced {
                    first_staged_tx.send(()).unwrap();
                    first_resume_rx.recv().unwrap();
                }
            },
        )
    });
    first_staged_rx
        .recv_timeout(Duration::from_secs(10))
        .expect("first export stages its disk");

    let (second_event_tx, second_event_rx) = mpsc::channel();
    let (second_resume_tx, second_resume_rx) = mpsc::channel();
    let second_staged_tx = second_event_tx.clone();
    let second_target = second_destination.clone();
    let second = thread::spawn(move || {
        let result = create_snapshot_using(
            &second_disk,
            &second_vars,
            &second_target,
            "second",
            false,
            1024,
            |stage| {
                if stage == CreateStage::DiskSynced {
                    second_staged_tx.send(SecondEvent::DiskSynced).unwrap();
                    second_resume_rx.recv().unwrap();
                }
            },
        );
        second_event_tx.send(SecondEvent::Finished).unwrap();
        result
    });
    let event = second_event_rx.recv_timeout(Duration::from_secs(10));
    match event {
        Ok(SecondEvent::DiskSynced) | Ok(SecondEvent::Finished) => {}
        Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => {
            let _ = first_resume_tx.send(());
            let _ = second_resume_tx.send(());
            panic!("second export neither staged nor refused");
        }
    }

    first_resume_tx.send(()).unwrap();
    let first_result = first.join().expect("first export thread");
    let _ = second_resume_tx.send(());
    let second_result = second.join().expect("second export thread");
    assert!(
        matches!(&second_result, Err(SnapshotError::Io(error))
            if error.kind() == std::io::ErrorKind::WouldBlock),
        "contending export did not refuse on the parent lease: {second_result:?}"
    );
    let first_manifest = first_result.expect("first export must finish");
    assert_eq!(verify_snapshot(&destination).unwrap(), first_manifest);
    assert_eq!(first_manifest.vm_id, "first");
    assert_eq!(
        fs::read(destination.join(DISK_NAME)).unwrap(),
        b"first-disk"
    );
    assert_eq!(
        fs::read(destination.join(VARS_NAME)).unwrap(),
        b"first-vars"
    );
    if second_name == "other-snapshot" {
        assert!(
            !second_destination.exists(),
            "refusal must leave no new output"
        );
    }
}

#[test]
fn same_destination_refuses_a_contending_export() {
    assert_contention("create-shared-destination", "snapshot", false, false);
}

#[test]
fn case_alias_refuses_a_contending_export_on_case_insensitive_volumes() {
    assert_contention("create-case-alias", "SNAPSHOT", true, false);
}

#[test]
fn parent_case_alias_refuses_a_contending_export_on_case_insensitive_volumes() {
    assert_contention("create-parent-case-alias", "snapshot", false, true);
}

#[test]
fn different_names_in_one_parent_refuse_a_contending_export() {
    assert_contention("create-shared-parent", "other-snapshot", false, false);
}
