use super::*;
use crate::snapshot_pair::files::{stop_for_test, stream_and_sync, COPY_CHUNK};
use crate::snapshot_pair::managed::LockedPair;
use crate::snapshot_pair::tests::{retry_create, Scratch};
use crate::snapshot_pair::{restore_snapshot, verify_snapshot};
use libc::ENOSPC;
use std::process::Command;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::Duration;

#[derive(Debug)]
enum SecondEvent {
    DiskSynced,
    Finished,
}

/// The first export pauses once its disk is staged; the second, to the same
/// destination under `second_name`, must refuse on a member lease. With
/// `previous`, an older snapshot occupies the destination first.
fn assert_contention(
    tag: &str,
    second_name: &str,
    final_case_alias: bool,
    parent_case_alias: bool,
    previous: bool,
) {
    let scratch = Scratch::new(tag);
    let destination = scratch.path("snapshot");
    let old_disk = scratch.write("old-disk", b"old-disk");
    let old_vars = scratch.write("old-vars", b"old-vars");
    if previous {
        create_snapshot(&old_disk, &old_vars, &destination, "old").unwrap();
    }
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
        "contending export did not refuse on a member lease: {second_result:?}"
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
}

#[test]
fn same_destination_refuses_a_contending_export() {
    assert_contention("create-shared-destination", "snapshot", false, false, true);
}

/// Nothing is at the destination yet, so only the first export's staging
/// member keys can refuse the second before it reclaims that staging.
#[test]
fn same_new_destination_refuses_a_contending_export() {
    assert_contention(
        "create-shared-new-destination",
        "snapshot",
        false,
        false,
        false,
    );
}

#[test]
fn case_alias_refuses_a_contending_export_on_case_insensitive_volumes() {
    assert_contention("create-case-alias", "SNAPSHOT", true, false, true);
}

#[test]
fn parent_case_alias_refuses_a_contending_export_on_case_insensitive_volumes() {
    assert_contention("create-parent-case-alias", "snapshot", false, true, true);
}

#[test]
fn replacing_a_snapshot_refuses_while_its_members_are_being_copied() {
    let scratch = Scratch::new("create-source-member-replacement");
    let old_disk = scratch.write("old-disk", b"old-disk");
    let old_vars = scratch.write("old-vars", b"old-vars");
    let new_disk = scratch.write("new-disk", b"new-disk");
    let new_vars = scratch.write("new-vars", b"new-vars");
    let source = scratch.path("S");
    let original = create_snapshot(&old_disk, &old_vars, &source, "old").unwrap();
    // T lives in another directory, so only the leases on S's members can
    // refuse another create that replaces S itself.
    let target = scratch.path("export-parent/T");
    let mut replacement = None;
    let exported = create_snapshot_using(
        &source.join(DISK_NAME),
        &source.join(VARS_NAME),
        &target,
        "export",
        |stage| {
            if stage == CreateStage::DiskSynced {
                assert!(replacement.is_none(), "disk hook must run exactly once");
                replacement = Some(create_snapshot(&new_disk, &new_vars, &source, "new"));
            }
        },
    )
    .expect("export from the leased source must finish");
    let replacement = replacement.expect("nested create must run after the disk copy");

    // Verify and read T before asserting refusal: a baseline failure must show
    // whether the real nested create produced a manifest-valid mixed pair.
    assert_eq!(verify_snapshot(&target).unwrap(), exported);
    let disk = fs::read(target.join(DISK_NAME)).unwrap();
    let vars = fs::read(target.join(VARS_NAME)).unwrap();
    let mixed_pair = disk == b"old-disk" && vars == b"new-vars";
    if let Ok(replaced) = &replacement {
        assert_eq!(verify_snapshot(&source).unwrap(), *replaced);
        assert!(
            mixed_pair,
            "nested create succeeded but mixed-pair hypothesis was not reproduced: \
             disk={disk:?}, vars={vars:?}"
        );
        eprintln!("nested create replaced S; T's manifest verifies old-disk/new-vars");
    }
    assert!(
        matches!(&replacement, Err(SnapshotError::Io(error))
            if error.kind() == std::io::ErrorKind::WouldBlock),
        "replacement must refuse leased destination members: {replacement:?}; \
         T manifest verified, mixed_old_disk_new_vars={mixed_pair}, disk={:?}, vars={:?}",
        String::from_utf8_lossy(&disk),
        String::from_utf8_lossy(&vars),
    );
    assert_eq!(disk, b"old-disk");
    assert_eq!(vars, b"old-vars");
    assert_eq!(verify_snapshot(&source).unwrap(), original);
}

#[test]
fn existing_destination_member_or_hardlink_owner_refuses_before_staging_cleanup() {
    for hardlink in [false, true] {
        let scratch = Scratch::new("member-lease-alias");
        let disk = scratch.write("disk", b"disk");
        let vars = scratch.write("vars", b"vars");
        let dest = scratch.path("snapshot");
        create_snapshot(&disk, &vars, &dest, "vm").unwrap();
        let staged = staging_path(&dest);
        OwnedStaging::create(&staged)
            .unwrap()
            .record_ownership(None)
            .unwrap();
        fs::write(staged.join(DISK_NAME), b"debris-kept").unwrap();
        let member = dest.join(VARS_NAME);
        let owned = if hardlink {
            let alias = scratch.path("alias");
            fs::hard_link(&member, &alias).unwrap();
            alias
        } else {
            member
        };
        let owner = MediaLease::acquire([owned.as_path()]).unwrap();
        assert_eq!(
            claim_staging(&dest, None).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
        assert_eq!(fs::read(staged.join(DISK_NAME)).unwrap(), b"debris-kept");
        assert_eq!(fs::read(dest.join(VARS_NAME)).unwrap(), b"vars");
        drop(owner);
        let (_owner, rebuilt) = claim_staging(&dest, None).unwrap();
        assert!(!rebuilt.path.join(DISK_NAME).exists());
    }
}

#[test]
fn leased_staging_member_is_not_reclaimed() {
    let scratch = Scratch::new("leased-staging-member");
    let dest = scratch.path("snapshot");
    let staged = staging_path(&dest);
    fs::create_dir(&staged).unwrap();
    let member = staged.join(DISK_NAME);
    fs::write(&member, b"retained").unwrap();
    let _owner = MediaLease::acquire([member.as_path()]).unwrap();
    assert_eq!(
        claim_staging(&dest, None).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    assert_eq!(fs::read(member).unwrap(), b"retained");
    assert!(!dest.exists());
}

#[test]
fn fresh_staging_member_paths_stay_owned_until_export_owner_drops() {
    let scratch = Scratch::new("fresh-staging-member");
    let dest = scratch.path("snapshot");
    let (owner, staged) = claim_staging(&dest, None).unwrap();
    for name in [DISK_NAME, VARS_NAME] {
        let member = staged.path.join(name);
        fs::write(&member, b"new").unwrap();
        assert_eq!(
            MediaLease::acquire([member.as_path()]).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
    }
    drop(owner);
    MediaLease::acquire([
        staged.path.join(DISK_NAME).as_path(),
        staged.path.join(VARS_NAME).as_path(),
    ])
    .unwrap();
}

fn assert_refused(result: Result<SnapshotManifest, SnapshotError>, dest: &Path) {
    match result {
        Err(SnapshotError::Io(error)) => assert!(
            error.to_string().contains("left intact"),
            "refusal does not say the output was kept: {error}"
        ),
        other => panic!("expected an intact-output refusal, got {other:?}"),
    }
    assert!(!staging_path(dest).exists(), "refusal stranded staging");
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
    let old = create_snapshot(&disk, &vars, &dest, "old").unwrap();
    fs::write(&disk, b"new disk").unwrap();
    let result = create_snapshot_using(
        &disk,
        &vars,
        &dest,
        "new",
        arrive_after_staging(&dest, false),
    );
    assert_eq!(
        fs::read(dest.join("keep.txt")).ok().as_deref(),
        Some(&b"arrived during copy"[..]),
        "publication deleted data that arrived after admission"
    );
    assert_refused(result, &dest);
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
        arrive_after_staging(&dest, true),
    );
    assert_eq!(
        fs::read(dest.join("keep.txt")).ok().as_deref(),
        Some(&b"arrived during copy"[..]),
        "publication deleted a directory that appeared after admission"
    );
    assert_eq!(fs::read_dir(&dest).unwrap().count(), 1);
    assert_refused(result, &dest);
}

fn fixture(tag: &str) -> (Scratch, PathBuf, PathBuf, PathBuf, SnapshotManifest) {
    let s = Scratch::new(tag);
    let disk = s.write("disk", b"old-disk");
    let vars = s.write("vars", b"old-vars");
    let dest = s.path("snapshot");
    let old = create_snapshot(&disk, &vars, &dest, "old").unwrap();
    fs::write(&disk, b"new-disk").unwrap();
    (s, disk, vars, dest, old)
}

/// The vars source vanishing once the disk is staged fails the second copy
/// while staging already holds a complete disk copy.
#[test]
fn a_copy_error_after_staging_is_claimed_removes_staging() {
    let (_s, disk, vars, dest, old) = fixture("create-copy-error");
    let result = create_snapshot_using(&disk, &vars, &dest, "new", |stage| {
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
    let retried = create_snapshot(&disk, &vars, &dest, "retry").unwrap();
    assert_eq!(verify_snapshot(&dest).unwrap(), retried);
}

/// Clearing after a failure follows debris rules: foreign content is kept.
#[test]
fn a_copy_error_keeps_staging_that_gained_foreign_content() {
    let (_s, disk, vars, dest, old) = fixture("create-copy-error-foreign");
    let staging = staging_path(&dest);
    let result = create_snapshot_using(&disk, &vars, &dest, "new", |stage| {
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
    let result = create_stopped(&disk, &vars, &dest, "new", full, publish, |_| {});
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
    let copy = |src: &Path, dst: &Path| stream_and_sync(src, dst, |_| Ok(()));
    let retried = create_stopped(&disk, &vars, &dest, "retry", copy, publish, drop);
    assert_eq!(verify_snapshot(&dest).unwrap(), retried.unwrap());
}

/// Cancelling an export killed the helper mid-copy and left the partial disk in the hidden
/// staging folder on the backup drive; now the signal stops the copy and the attempt cleans up.
#[test]
fn a_stop_signal_during_a_streaming_copy_removes_the_partial_copy() {
    let (s, _, vars, dest, old) = fixture("create-copy-stopped");
    let disk = s.write("disk", &vec![7; 2 * COPY_CHUNK + 4096]);
    let mut partial = None;
    let stopped = |src: &Path, dst: &Path| {
        let copied = stream_and_sync(src, dst, |_| {
            stop_for_test::set(true); // the signal arrives during the first chunk
            Ok(())
        });
        partial = fs::metadata(dst).ok().map(|metadata| metadata.len());
        copied
    };
    let result = create_stopped(&disk, &vars, &dest, "new", stopped, publish, |_| {});
    stop_for_test::set(false);
    assert_eq!(
        partial,
        Some(COPY_CHUNK as u64),
        "copy did not stop mid-disk"
    );
    assert!(result.is_err(), "a stopped copy must fail the create");
    assert!(
        !staging_path(&dest).exists(),
        "stopped copy stranded staging"
    );
    assert_eq!(verify_snapshot(&dest).unwrap(), old);
}

fn available_vm_id_bytes() -> usize {
    MANIFEST_LIMIT as usize
        - SnapshotManifest {
            format_version: SNAPSHOT_FORMAT_VERSION,
            vm_id: String::new(),
            disk_bytes: 4,
            disk_sha256: "0".repeat(64),
            vars_bytes: 4,
            vars_sha256: "0".repeat(64),
        }
        .to_json()
        .len()
}

#[test]
fn oversized_plain_and_escaped_vm_ids_refuse_and_keep_the_previous_snapshot() {
    for (name, vm_id) in [
        ("ascii", "x".repeat(MANIFEST_LIMIT as usize)),
        ("controls", "\n".repeat(MANIFEST_LIMIT as usize / 6 + 1)),
    ] {
        let scratch = Scratch::new(&format!("create-manifest-limit-{name}"));
        let disk = scratch.write("disk", b"disk");
        let vars = scratch.write("vars", b"vars");
        let snapshot = scratch.path("snapshot");
        let old = create_snapshot(&disk, &vars, &snapshot, "old").unwrap();
        let old_text = fs::read(snapshot.join(MANIFEST_NAME)).unwrap();
        let result = create_snapshot(&disk, &vars, &snapshot, &vm_id);
        let Err(SnapshotError::Io(error)) = result else {
            panic!("accepted unreadable {name} manifest: {result:?}");
        };
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert!(!staging_path(&snapshot).exists());
        assert_eq!(verify_snapshot(&snapshot).unwrap(), old);
        assert_eq!(fs::read(snapshot.join(MANIFEST_NAME)).unwrap(), old_text);
        assert_eq!(fs::read(&disk).unwrap(), b"disk");
        assert_eq!(fs::read(&vars).unwrap(), b"vars");
    }
}

#[test]
fn exactly_bounded_plain_and_escaped_manifests_round_trip_after_creation() {
    let available = available_vm_id_bytes();
    let controls = "\0".repeat(available / 6) + &"x".repeat(available % 6);
    for (name, vm_id) in [("ascii", "x".repeat(available)), ("controls", controls)] {
        let scratch = Scratch::new(&format!("create-manifest-boundary-{name}"));
        let disk = scratch.write("disk", b"disk");
        let vars = scratch.write("vars", b"vars");
        let snapshot = scratch.path("snapshot");
        let manifest = create_snapshot(&disk, &vars, &snapshot, &vm_id).unwrap();
        assert_eq!(
            fs::metadata(snapshot.join(MANIFEST_NAME)).unwrap().len(),
            MANIFEST_LIMIT
        );
        assert_eq!(verify_snapshot(&snapshot).unwrap(), manifest);
        assert_eq!(manifest.vm_id, vm_id);
    }
}

#[test]
fn final_serialized_size_is_rechecked_and_failed_fill_is_removed() {
    let scratch = Scratch::new("create-manifest-final-limit");
    let disk = scratch.write("disk", b"disk");
    let vars = scratch.write("vars", b"vars");
    let snapshot = scratch.path("snapshot");
    let old = create_snapshot(&disk, &vars, &snapshot, "old").unwrap();
    let vm_id = "x".repeat(available_vm_id_bytes());
    let result = create_snapshot_using(&disk, &vars, &snapshot, &vm_id, |stage| {
        if stage == CreateStage::DiskSynced {
            // A changed digit count grows the final serialized metadata by one
            // byte without changing the preflighted vm_id.
            fs::write(&vars, b"ten bytes!").unwrap();
        }
    });
    let Err(SnapshotError::Io(error)) = result else {
        panic!("accepted unreadable final metadata: {result:?}");
    };
    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    assert!(!staging_path(&snapshot).exists());
    assert_eq!(verify_snapshot(&snapshot).unwrap(), old);
}

#[test]
fn creation_refuses_outputs_containing_either_source_before_cleanup() {
    for directory in [".snapshot.staging", "snapshot"] {
        for source in ["disk", "vars"] {
            let s = Scratch::new(&format!("create-overlap-{directory}-{source}"));
            fs::create_dir(s.path(directory)).unwrap();
            let disk_name = if source == "disk" {
                format!("{directory}/source-disk")
            } else {
                "source-disk".into()
            };
            let vars_name = if source == "vars" {
                format!("{directory}/source-vars")
            } else {
                "source-vars".into()
            };
            let disk = s.write(&disk_name, b"own-disk");
            let vars = s.write(&vars_name, b"own-vars");
            let marker = s.write(&format!("{directory}/keep"), b"unrelated");
            let result = create_snapshot(&disk, &vars, &s.path("snapshot"), "vm");
            assert!(result.is_err(), "accepted {source} inside {directory}");
            assert_eq!(fs::read(&disk).unwrap(), b"own-disk");
            assert_eq!(fs::read(&vars).unwrap(), b"own-vars");
            assert_eq!(fs::read(marker).unwrap(), b"unrelated");
        }
    }
}

#[test]
fn creation_refuses_outputs_containing_the_selected_managed_generation() {
    let s = Scratch::new("create-managed-overlap");
    let disk = s.write("disk", b"own-disk");
    let vars = s.write("vars", b"own-vars");
    create_snapshot(&disk, &vars, &s.path("snapshot"), "vm").unwrap();
    restore_snapshot(&s.path("snapshot"), &disk, &vars).unwrap();
    let pair = managed::LockedPair::open(&disk, &vars).unwrap();
    let (selected_disk, selected_vars) = pair.paths().unwrap();
    let destination = selected_disk
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    drop(pair);
    assert!(create_snapshot(&disk, &vars, &destination, "vm").is_err());
    let pair = managed::LockedPair::open(&disk, &vars).unwrap();
    assert_eq!(
        pair.paths().unwrap(),
        (selected_disk.clone(), selected_vars.clone())
    );
    assert_eq!(fs::read(selected_disk).unwrap(), b"own-disk");
    assert_eq!(fs::read(selected_vars).unwrap(), b"own-vars");
    assert_eq!(fs::read(disk).unwrap(), b"own-disk");
    assert_eq!(fs::read(vars).unwrap(), b"own-vars");
}

#[test]
fn parent_aliases_preserve_normal_creation_and_overlap_refusal() {
    let s = Scratch::new("create-parent-alias");
    fs::create_dir(s.path("outputs")).unwrap();
    std::os::unix::fs::symlink(s.path("outputs"), s.path("alias")).unwrap();
    let disk = s.write("disk", b"own-disk");
    let vars = s.write("vars", b"own-vars");
    let manifest = create_snapshot(&disk, &vars, &s.path("alias/snapshot"), "vm").unwrap();
    assert_eq!(
        verify_snapshot(&s.path("outputs/snapshot")).unwrap(),
        manifest
    );
    fs::create_dir(s.path("outputs/.other.staging")).unwrap();
    let disk = s.write("outputs/.other.staging/source-disk", b"own-disk");
    assert!(create_snapshot(&disk, &vars, &s.path("alias/other"), "vm").is_err());
    assert_eq!(fs::read(disk).unwrap(), b"own-disk");
    assert_eq!(fs::read(vars).unwrap(), b"own-vars");
}

#[test]
fn creation_does_not_follow_a_symbolic_destination_for_publication() {
    let s = Scratch::new("create-destination-alias");
    fs::create_dir(s.path("target")).unwrap();
    let marker = s.write("target/keep", b"unrelated");
    std::os::unix::fs::symlink(s.path("target"), s.path("snapshot")).unwrap();
    let disk = s.write("disk", b"own-disk");
    let vars = s.write("vars", b"own-vars");
    assert!(create_snapshot(&disk, &vars, &s.path("snapshot"), "vm").is_err());
    assert!(fs::symlink_metadata(s.path("snapshot"))
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(fs::read(marker).unwrap(), b"unrelated");
    assert_eq!(fs::read(disk).unwrap(), b"own-disk");
    assert_eq!(fs::read(vars).unwrap(), b"own-vars");
}

const SCENARIO_ENV: &str = "BRIDGEVM_TEST_SNAPSHOT_CREATE_CRASH";
const DISK_ENV: &str = "BRIDGEVM_TEST_SNAPSHOT_CREATE_DISK";
const VARS_ENV: &str = "BRIDGEVM_TEST_SNAPSHOT_CREATE_VARS";
const DEST_ENV: &str = "BRIDGEVM_TEST_SNAPSHOT_CREATE_DEST";

fn required_path(name: &str) -> PathBuf {
    std::env::var_os(name)
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("missing subprocess path {name}"))
}

/// The last field is whether the attempt leaves a finished snapshot in staging.
fn selected_scenario(stage: CreateStage) -> (&'static str, i32, bool) {
    match stage {
        CreateStage::DiskSynced => ("disk-synced", 81, false),
        CreateStage::VarsSynced => ("vars-synced", 82, false),
        CreateStage::ManifestPublished => ("manifest-published", 83, true),
        CreateStage::StagingDirectorySynced => ("staging-directory-synced", 84, true),
        CreateStage::SnapshotPublished => ("snapshot-published", 85, false),
    }
}

#[test]
fn create_crash_child() {
    let Ok(scenario) = std::env::var(SCENARIO_ENV) else {
        return;
    };
    let disk = required_path(DISK_ENV);
    let vars = required_path(VARS_ENV);
    let dest = required_path(DEST_ENV);
    create_snapshot_using(&disk, &vars, &dest, "new", |stage| {
        let (name, exit, _) = selected_scenario(stage);
        if scenario == name {
            std::process::exit(exit);
        }
    })
    .expect("the selected create boundary must exit before return");
    panic!("unknown create crash scenario {scenario}");
}

fn crash_create(scenario: &str, exit: i32, disk: &Path, vars: &Path, dest: &Path) {
    let status = Command::new(std::env::current_exe().expect("current test executable"))
        .arg("snapshot_pair::create::tests::create_crash_child")
        .arg("--exact")
        .arg("--nocapture")
        .env(SCENARIO_ENV, scenario)
        .env(DISK_ENV, disk)
        .env(VARS_ENV, vars)
        .env(DEST_ENV, dest)
        .status()
        .expect("launch create crash subprocess");
    assert_eq!(status.code(), Some(exit), "child status {status}");
}

#[test]
fn hard_exit_during_create_selects_only_one_complete_snapshot_and_allows_retry() {
    for stage in [
        CreateStage::DiskSynced,
        CreateStage::VarsSynced,
        CreateStage::ManifestPublished,
        CreateStage::StagingDirectorySynced,
        CreateStage::SnapshotPublished,
    ] {
        let (scenario, exit, finished) = selected_scenario(stage);
        let scratch = Scratch::new(scenario);
        let disk = scratch.write("disk", b"old-disk");
        let vars = scratch.write("vars", b"old-vars");
        let dest = scratch.path("snapshot");
        let old = create_snapshot(&disk, &vars, &dest, "old").unwrap();
        fs::write(&disk, b"new-disk").unwrap();
        fs::write(&vars, b"new-vars").unwrap();

        crash_create(scenario, exit, &disk, &vars, &dest);
        let selected = verify_snapshot(&dest).expect("one complete snapshot remains selected");
        if stage == CreateStage::SnapshotPublished {
            assert_eq!(selected.vm_id, "new");
        } else {
            assert_eq!(selected, old);
        }

        let retried = retry_create(&disk, &vars, &dest, finished);
        assert_eq!(verify_snapshot(&dest).unwrap(), retried);
        assert_eq!(retried.vm_id, "retry");
    }
}

/// The app's next create uses a new name and recovers its owned interrupted
/// staging. A public export, a live lease and unfamiliar directories are kept.
#[test]
fn a_stored_create_clears_only_its_earlier_interrupted_stages() {
    let scratch = Scratch::new("create-sibling-debris");
    let parent = scratch.path("bundle/metadata/snapshots");
    fs::create_dir_all(&parent).unwrap();
    fs::create_dir(scratch.path("bundle/disks")).unwrap();
    let disk = scratch.write("bundle/disks/hvf-target.raw", b"disk");
    let vars = scratch.write("bundle/metadata/hvf-vars.fd", b"vars");
    let killed = parent.join("20261123-100000.snapshot");
    fs::create_dir(&killed).unwrap();
    crash_create("vars-synced", 82, &disk, &vars, &killed);
    let killed_staging = staging_path(&killed);
    assert!(
        killed_staging.join(VARS_NAME).is_file(),
        "no debris to clear"
    );
    let debris = |name: &str, files: &[&str]| {
        let staging = staging_path(&parent.join(name));
        fs::create_dir(&staging).unwrap();
        for file in files {
            fs::write(staging.join(file), b"kept").unwrap();
        }
        staging
    };
    let export = parent.join("export");
    crash_create("disk-synced", 81, &disk, &vars, &export);
    let killed_export = staging_path(&export);
    let unknown = debris("unknown", &[DISK_NAME]);
    let foreign = debris("foreign", &[DISK_NAME, "notes.txt"]);
    let beside_user = debris("user", &[DISK_NAME]);
    fs::create_dir(parent.join("user")).unwrap();
    fs::write(parent.join("user/notes.txt"), b"user").unwrap();
    let completed = debris("completed", &[DISK_NAME, VARS_NAME, MANIFEST_NAME]);

    let live = parent.join("20261123-100001.snapshot");
    fs::create_dir(&live).unwrap();
    let live_stage = staging_path(&live);
    OwnedStaging::create(&live_stage)
        .unwrap()
        .record_ownership(Some(identity(&live).unwrap()))
        .unwrap();
    let live_disk = live_stage.join(DISK_NAME);
    fs::write(&live_disk, b"live-disk").unwrap();
    let _live_lease = MediaLease::acquire([live_disk.as_path()]).unwrap();

    let created = parent.join("20261123-100002.snapshot");
    fs::create_dir(&created).unwrap();
    let result = create_snapshot(&disk, &vars, &created, "new");
    assert_eq!(verify_snapshot(&created).unwrap(), result.unwrap());
    assert_eq!(fs::read(live_disk).unwrap(), b"live-disk");

    for gone in [&killed_staging, &killed] {
        assert!(!gone.exists(), "{} was left behind", gone.display());
    }
    assert_eq!(fs::read(killed_export.join(DISK_NAME)).unwrap(), b"disk");
    for kept in [&unknown, &foreign, &beside_user, &completed] {
        assert_eq!(fs::read(kept.join(DISK_NAME)).unwrap(), b"kept");
    }
    assert_eq!(fs::read(foreign.join("notes.txt")).unwrap(), b"kept");
    assert_eq!(fs::read(parent.join("user/notes.txt")).unwrap(), b"user");
    assert_eq!(fs::read(completed.join(MANIFEST_NAME)).unwrap(), b"kept");
}

#[test]
fn stored_create_preserves_a_snapshot_that_arrives_after_reservation() {
    let scratch = Scratch::new("stored-reservation-filled");
    let parent = scratch.path("bundle/metadata/snapshots");
    fs::create_dir_all(&parent).unwrap();
    fs::create_dir(scratch.path("bundle/disks")).unwrap();
    let disk = scratch.write("bundle/disks/hvf-target.raw", b"disk");
    let vars = scratch.write("bundle/metadata/hvf-vars.fd", b"vars");
    let arriving = scratch.path("arriving");
    let expected = create_snapshot(&disk, &vars, &arriving, "arriving").unwrap();
    let dest = parent.join("20261123-100000.snapshot");
    fs::create_dir(&dest).unwrap();

    let outcome = create_snapshot_using(&disk, &vars, &dest, "new", |stage| {
        if stage == CreateStage::StagingDirectorySynced {
            for name in [DISK_NAME, VARS_NAME, MANIFEST_NAME] {
                fs::copy(arriving.join(name), dest.join(name)).unwrap();
            }
        }
    });
    assert!(outcome.is_err());
    assert_eq!(verify_snapshot(&dest).unwrap(), expected);
    assert!(!staging_path(&dest).exists());
}

#[test]
fn disk_only_import_selects_restored_pair_and_refuses_owner() {
    let root = std::env::temp_dir().join(format!("import-disk-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let disk = root.join("disk.raw");
    let vars = root.join("vars.fd");
    fs::write(&disk, b"old").unwrap();
    fs::write(&vars, b"vars-old").unwrap();
    let snapshot = root.join("snapshot");
    create_snapshot(&disk, &vars, &snapshot, "test").unwrap();
    fs::write(&disk, b"new").unwrap();
    restore_snapshot(&snapshot, &disk, &vars).unwrap();
    let dest = root.join("import");
    assert!(import_disk(&disk, &dest).unwrap());
    assert_eq!(fs::read(dest.join("disk.raw")).unwrap(), b"old");
    assert_eq!(fs::read(dest.join("vars.fd")).unwrap(), b"vars-old");
    assert_eq!(fs::read(&disk).unwrap(), b"new");
    let owner = LockedPair::open(&disk, &vars).unwrap();
    assert!(import_disk(&disk, &root.join("blocked")).is_err());
    assert!(!root.join("blocked").exists());
    drop(owner);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn unmanaged_disk_copy_takes_native_disk_lease() {
    let root = std::env::temp_dir().join(format!("import-raw-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let disk = root.join("disk.raw");
    fs::write(&disk, b"raw").unwrap();
    let owner = MediaLease::acquire([disk.as_path()]).unwrap();
    assert!(import_disk(&disk, &root.join("blocked")).is_err());
    drop(owner);
    let dest = root.join("import");
    assert!(!import_disk(&disk, &dest).unwrap());
    assert_eq!(fs::read(dest.join("disk.raw")).unwrap(), b"raw");
    assert!(!dest.join("vars.fd").exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn managed_pair_kept_in_a_directory_named_metadata_is_one_binding() {
    // Both places searched for the vars file are this one directory when it is named
    // "metadata"; the single pair must not be reported as ambiguous.
    let root = std::env::temp_dir().join(format!("import-metadata-{}", std::process::id()));
    let metadata = root.join("metadata");
    fs::create_dir_all(&metadata).unwrap();
    let disk = metadata.join("disk.raw");
    let vars = metadata.join("vars.fd");
    fs::write(&disk, b"old").unwrap();
    fs::write(&vars, b"vars-old").unwrap();
    let snapshot = root.join("snapshot");
    create_snapshot(&disk, &vars, &snapshot, "test").unwrap();
    fs::write(&disk, b"new").unwrap();
    restore_snapshot(&snapshot, &disk, &vars).unwrap();
    let dest = root.join("import");
    let imported = import_disk(&disk, &dest).map_err(|e| e.to_string());
    assert_eq!(
        imported,
        Ok(true),
        "one managed pair must import with its vars"
    );
    assert_eq!(fs::read(dest.join("disk.raw")).unwrap(), b"old");
    assert_eq!(fs::read(dest.join("vars.fd")).unwrap(), b"vars-old");
    fs::remove_dir_all(root).unwrap();
}

mod runtime_exclusion {
    //! Powered-off create/restore versus the owned runtime's image writer leases.
    use super::*;
    use crate::media::lock::{MediaLease, MediaLockError};
    use crate::snapshot_pair::selected::selected_size;
    use crate::snapshot_pair::{powered_off, writer_refusal};

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
        create_snapshot(&disk, &vars, &snapshot, "vm").unwrap();
        fs::write(&disk, b"later-disk").unwrap();
        for image in [&disk, &vars] {
            let runtime = MediaLease::acquire(image, "hvf-runner held").unwrap();
            let output = scratch.path("output");
            refused(
                create_snapshot(&disk, &vars, &output, "vm"),
                "hvf-runner held",
            );
            assert!(!output.exists() && !scratch.path(".output.staging").exists());
            let size = selected_size(&disk, &vars).expect_err("create sizing admitted");
            assert!(size.to_string().contains(IN_USE), "{size}");
            refused(restore_snapshot(&snapshot, &disk, &vars), "hvf-runner held");
            let (selected, _) = managed::LockedPair::open(&disk, &vars)
                .unwrap()
                .paths()
                .unwrap();
            assert_eq!(fs::read(selected).unwrap(), b"later-disk");
            drop(runtime);
        }
        restore_snapshot(&snapshot, &disk, &vars).unwrap();
    }

    #[test]
    fn a_runtime_cannot_be_prepared_while_a_powered_off_create_runs() {
        let scratch = Scratch::new("runtime-exclusion-create");
        let disk = scratch.write("disk", b"disk");
        let vars = scratch.write("vars", b"vars");
        ran_before(&disk);
        ran_before(&vars);
        let mut during = Vec::new();
        create_snapshot_using(&disk, &vars, &scratch.path("snap"), "vm", |stage| {
            if stage == CreateStage::DiskSynced {
                during = vec![runtime_refusal(&disk), runtime_refusal(&vars)];
            }
        })
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
        create_snapshot(&disk, &vars, &snapshot, "vm").unwrap();
        ran_before(&disk);
        ran_before(&vars);
        let mut owner = powered_off(&disk, &vars, "restore").unwrap();
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
        create_snapshot_using(&disk, &vars, &scratch.path("snap"), "vm", |stage| {
            if stage == CreateStage::DiskSynced {
                generation = Some(managed::LockedPair::open(&disk, &vars).map(|_| ()));
            }
        })
        .unwrap();
        let refusal = generation
            .unwrap()
            .expect_err("a generation opened a pair being copied");
        assert_eq!(refusal.kind(), io::ErrorKind::WouldBlock);
        let snapshot = scratch.path("snap");
        restore_snapshot(&snapshot, &disk, &vars).unwrap();
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
        assert!(matches!(writer_refusal(io_error(libc::EROFS)), Ok(None)));
        let denied = writer_refusal(io_error(libc::EACCES)).unwrap_err();
        assert_eq!(denied.kind(), io::ErrorKind::PermissionDenied);
        let held = MediaLockError::Held {
            path: "/m/disk.raw".into(),
            holder: "hvf-runner".into(),
        };
        let held = writer_refusal(held).unwrap_err();
        assert_eq!(held.kind(), io::ErrorKind::WouldBlock);
        assert!(held.to_string().contains(IN_USE), "{held}");
    }
}
