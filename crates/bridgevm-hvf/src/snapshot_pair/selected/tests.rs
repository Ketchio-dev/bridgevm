use super::*;
use crate::media::lock::MediaLease as WriterLease;
use crate::media_lease::MediaLease;
use crate::nvme::{NvmeController, NSID};
use crate::snapshot_pair::tests::Scratch;
use crate::snapshot_pair::{create_snapshot, restore_snapshot, verify_snapshot};
use std::fs::{self, OpenOptions};
use std::os::unix::fs::{symlink, FileExt, MetadataExt};
use std::sync::mpsc;
use std::time::Duration;

#[test]
fn after_a_restore_the_selected_pair_matches_the_snapshot_and_the_originals_do_not() {
    let s = Scratch::new("selected");
    let disk = s.write("disk.raw", b"disk before");
    let vars = s.write("vars.fd", b"vars before");
    let snap = s.path("snap");
    let m = create_snapshot(&disk, &vars, &snap, "vm-1").expect("create");
    fs::write(&disk, b"disk mutated!").expect("mutate disk");
    fs::write(&vars, b"vars mutated!").expect("mutate vars");
    restore_snapshot(&snap, &disk, &vars).expect("restore");

    let selected = selected_digest(&disk, &vars).expect("digest");
    assert_eq!(
        (selected.disk_sha256.as_str(), selected.disk_bytes),
        (m.disk_sha256.as_str(), m.disk_bytes)
    );
    assert_eq!(
        (selected.vars_sha256.as_str(), selected.vars_bytes),
        (m.vars_sha256.as_str(), m.vars_bytes)
    );
    assert_eq!(
        fs::read(&disk).expect("original disk"),
        b"disk mutated!",
        "originals keep their bytes"
    );
}

#[test]
fn without_a_managed_generation_the_originals_are_the_selected_pair() {
    let s = Scratch::new("selected-plain");
    let disk = s.write("disk.raw", b"d");
    let vars = s.write("vars.fd", b"v");
    let selected = selected_digest(&disk, &vars).expect("digest");
    assert_eq!((selected.disk_bytes, selected.vars_bytes), (1, 1));
    assert_ne!(selected.disk_sha256, selected.vars_sha256);
}

#[test]
fn selected_digest_keeps_accepting_logical_aliases_after_canonical_selection() {
    let scratch = Scratch::new("selected-digest-logical-link");
    let disk = scratch.write("disk", b"disk");
    let vars = scratch.write("vars", b"vars");
    let alias = scratch.path("disk-alias");
    symlink(&disk, &alias).unwrap();
    let digest = selected_digest(&alias, &vars).unwrap();
    assert_eq!((digest.disk_bytes, digest.vars_bytes), (4, 4));
    assert!(fs::symlink_metadata(alias).unwrap().is_symlink());
}

#[test]
fn original_pair_sizes_and_logical_aliases_are_supported() {
    let s = Scratch::new("size-original");
    let disk = s.write("disk", &[0; 144]);
    let vars = s.write("vars", &[0; 17]);
    let alias = s.path("disk-alias");
    symlink(&disk, &alias).unwrap();
    for path in [&disk, &alias] {
        assert_eq!(
            selected_size(path, &vars).unwrap(),
            SelectedSize {
                disk_bytes: 144,
                vars_bytes: 17,
            }
        );
    }
}

#[test]
fn restored_selected_sizes_not_preserved_originals_are_reported_and_exported() {
    let s = Scratch::new("size-restored");
    let disk = s.write("disk", &[1; 144]);
    let vars = s.write("vars", &[2; 17]);
    let snapshot = s.path("snapshot");
    let original = create_snapshot(&disk, &vars, &snapshot, "vm").unwrap();
    fs::write(&disk, [3; 72]).unwrap();
    fs::write(&vars, [4; 9]).unwrap();
    restore_snapshot(&snapshot, &disk, &vars).unwrap();

    let size = selected_size(&disk, &vars).unwrap();
    assert_eq!((size.disk_bytes, size.vars_bytes), (144, 17));
    assert_eq!(fs::read(&disk).unwrap(), [3; 72]);
    assert_eq!(fs::read(&vars).unwrap(), [4; 9]);
    let exported = create_snapshot(&disk, &vars, &s.path("exported"), "vm").unwrap();
    assert_eq!(exported, original);
}

#[test]
fn logical_and_selected_member_leases_refuse_size_until_released() {
    let s = Scratch::new("size-leased");
    let disk = s.write("disk", b"disk");
    let vars = s.write("vars", b"vars");
    let snapshot = s.path("snapshot");
    create_snapshot(&disk, &vars, &snapshot, "vm").unwrap();
    restore_snapshot(&snapshot, &disk, &vars).unwrap();
    let pair = LockedPair::open(&disk, &vars).unwrap();
    assert_eq!(
        selected_size(&disk, &vars).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    let (selected_disk, selected_vars) = pair.paths().unwrap();
    drop(pair);
    for member in [&disk, &vars, &selected_disk, &selected_vars] {
        let lease = MediaLease::acquire([member.as_path()]).unwrap();
        assert_eq!(
            selected_size(&disk, &vars).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
        drop(lease);
        assert!(selected_size(&disk, &vars).is_ok());
    }
}

#[test]
fn invalid_original_or_selected_member_type_fails_closed() {
    for restored in [false, true] {
        for member in ["disk", "vars"] {
            let s = Scratch::new(&format!("size-invalid-{restored}-{member}"));
            let disk = s.write("disk", b"disk");
            let vars = s.write("vars", b"vars");
            if restored {
                let snapshot = s.path("snapshot");
                create_snapshot(&disk, &vars, &snapshot, "vm").unwrap();
                restore_snapshot(&snapshot, &disk, &vars).unwrap();
            }
            let pair = LockedPair::open(&disk, &vars).unwrap();
            let (selected_disk, selected_vars) = pair.paths().unwrap();
            drop(pair);
            let path = if member == "disk" {
                selected_disk
            } else {
                selected_vars
            };
            fs::remove_file(&path).unwrap();
            fs::create_dir(&path).unwrap();
            assert!(selected_size(&disk, &vars).is_err());
            fs::remove_dir(&path).unwrap();
            assert!(selected_size(&disk, &vars).is_err());
            if restored {
                let outside = s.write("outside", b"not selected media");
                symlink(&outside, &path).unwrap();
                assert!(selected_size(&disk, &vars).is_err());
            }
        }
    }
}

#[test]
fn huge_sparse_disk_is_sized_without_reading_or_hashing_it() {
    let s = Scratch::new("size-sparse");
    let disk = s.write("disk", b"");
    let vars = s.write("vars", b"");
    OpenOptions::new()
        .write(true)
        .open(&disk)
        .unwrap()
        .set_len(1 << 40)
        .unwrap();
    let (send, receive) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        send.send(selected_size(&disk, &vars)).unwrap();
    });
    let size = receive
        .recv_timeout(Duration::from_secs(3))
        .expect("size read file content")
        .unwrap();
    worker.join().unwrap();
    assert_eq!((size.disk_bytes, size.vars_bytes), (1 << 40, 0));
}

#[test]
fn report_has_exactly_two_decimal_lines_and_propagates_write_errors() {
    let size = SelectedSize {
        disk_bytes: 0,
        vars_bytes: u64::MAX,
    };
    let mut output = Vec::new();
    write_report(&size, &mut output).unwrap();
    assert_eq!(output, b"disk_bytes 0\nvars_bytes 18446744073709551615\n");
    assert!(write_report(&size, &mut [0u8; 1][..]).is_err());
    assert_eq!(size_command(&[]), ExitCode::from(2));
    assert_eq!(size_command(&["disk".into()]), ExitCode::from(2));
    assert_eq!(
        size_command(&["disk".into(), "vars".into(), "extra".into()]),
        ExitCode::from(2)
    );
}

const DISK: u64 = 64 * 1024;

#[test]
fn after_a_restore_the_selected_generation_grows_and_the_snapshot_keeps_its_size() {
    let s = Scratch::new("grow-restored");
    let disk = s.write("disk", &[1; DISK as usize]);
    let vars = s.write("vars", &[2; 17]);
    let snapshot = s.path("snapshot");
    let taken = create_snapshot(&disk, &vars, &snapshot, "vm").unwrap();
    restore_snapshot(&snapshot, &disk, &vars).unwrap();

    assert_eq!(
        grow_selected_disk(&disk, &vars, 4 * DISK).unwrap(),
        4 * DISK
    );

    let (selected, _) = LockedPair::open(&disk, &vars).unwrap().paths().unwrap();
    assert_ne!(selected, disk);
    let grown = fs::read(&selected).unwrap();
    assert_eq!(grown.len() as u64, 4 * DISK);
    assert!(grown[..DISK as usize].iter().all(|&b| b == 1));
    assert!(grown[DISK as usize..].iter().all(|&b| b == 0));
    // The original the restore replaced is not the disk the VM boots.
    assert_eq!(fs::metadata(&disk).unwrap().len(), DISK);
    assert_eq!(selected_size(&disk, &vars).unwrap().disk_bytes, 4 * DISK);
    assert_eq!(verify_snapshot(&snapshot).unwrap(), taken);

    // The runtime boots the selected file: its namespace is the grown size.
    let controller = NvmeController::with_raw_file(&selected, true).unwrap();
    let nsze = u64::from_le_bytes(
        controller.identify_namespace(NSID)[0..8]
            .try_into()
            .unwrap(),
    );
    assert_eq!(nsze, 4 * DISK / 512);
    drop(controller);

    // Restoring the older snapshot brings back its own size.
    restore_snapshot(&snapshot, &disk, &vars).unwrap();
    assert_eq!(selected_size(&disk, &vars).unwrap().disk_bytes, DISK);
}

#[test]
fn an_original_pair_grows_in_place_sparsely_and_never_shrinks() {
    let s = Scratch::new("grow-original");
    let disk = s.write("disk", &[7; DISK as usize]);
    let vars = s.write("vars", &[2; 17]);
    let blocks_before = fs::metadata(&disk).unwrap().blocks();
    let grown = 1 << 40;

    assert_eq!(grow_selected_disk(&disk, &vars, grown).unwrap(), grown);
    let metadata = fs::metadata(&disk).unwrap();
    assert_eq!(metadata.len(), grown);
    assert!(
        metadata.blocks() <= blocks_before + 8,
        "a 1 TiB grow allocated {} blocks",
        metadata.blocks() - blocks_before
    );
    let mut start = vec![0; DISK as usize + 512];
    fs::File::open(&disk)
        .unwrap()
        .read_exact_at(&mut start, 0)
        .unwrap();
    assert!(start[..DISK as usize].iter().all(|&b| b == 7));
    assert!(start[DISK as usize..].iter().all(|&b| b == 0));

    assert_eq!(grow_selected_disk(&disk, &vars, grown).unwrap(), grown);
    for refused in [DISK, grown - 512, grown + 1] {
        let error = grow_selected_disk(&disk, &vars, refused).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput, "{error}");
        assert_eq!(fs::metadata(&disk).unwrap().len(), grown);
    }
}

#[test]
fn a_running_vm_refuses_the_grow_before_the_disk_changes() {
    let s = Scratch::new("grow-running");
    let disk = s.write("disk", &[1; DISK as usize]);
    let vars = s.write("vars", &[2; 17]);
    let arguments = [
        disk.display().to_string(),
        vars.display().to_string(),
        (2 * DISK).to_string(),
    ];
    for image in [&disk, &vars] {
        let runtime = WriterLease::acquire(image, "hvf-runner held").unwrap();
        let error = grow_selected_disk(&disk, &vars, 2 * DISK).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::WouldBlock, "{error}");
        assert!(
            error.to_string().contains("in use by a running VM"),
            "{error}"
        );
        assert_eq!(grow_command(&arguments), ExitCode::from(IN_USE_EXIT));
        drop(runtime);
        assert_eq!(fs::metadata(&disk).unwrap().len(), DISK);
    }
    assert_eq!(grow_command(&arguments), ExitCode::SUCCESS);
    assert_eq!(fs::metadata(&disk).unwrap().len(), 2 * DISK);
    assert_eq!(grow_command(&arguments[..2]), ExitCode::from(2));
    let mut unparsable = arguments.clone();
    unparsable[2] = "64GiB".into();
    assert_eq!(grow_command(&unparsable), ExitCode::from(2));
}
