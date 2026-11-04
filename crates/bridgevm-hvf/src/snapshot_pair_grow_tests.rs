use super::*;
use crate::media::lock::MediaLease;
use crate::nvme::{NvmeController, NSID};
use crate::snapshot_pair::snapshot_pair_tests::{Scratch, QUOTA};
use crate::snapshot_pair::{
    create_snapshot, managed::LockedPair, restore_snapshot, size::selected_size, verify_snapshot,
};
use std::fs;
use std::os::unix::fs::{FileExt, MetadataExt};

const DISK: u64 = 64 * 1024;

#[test]
fn after_a_restore_the_selected_generation_grows_and_the_snapshot_keeps_its_size() {
    let s = Scratch::new("grow-restored");
    let disk = s.write("disk", &[1; DISK as usize]);
    let vars = s.write("vars", &[2; 17]);
    let snapshot = s.path("snapshot");
    let taken = create_snapshot(&disk, &vars, &snapshot, "vm", false, QUOTA).unwrap();
    restore_snapshot(&snapshot, &disk, &vars, false).unwrap();

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
    restore_snapshot(&snapshot, &disk, &vars, false).unwrap();
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
        let runtime = MediaLease::acquire(image, "hvf-runner held").unwrap();
        let error = grow_selected_disk(&disk, &vars, 2 * DISK).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::WouldBlock, "{error}");
        assert!(
            error.to_string().contains("in use by a running VM"),
            "{error}"
        );
        assert_eq!(command(&arguments), ExitCode::from(IN_USE_EXIT));
        drop(runtime);
        assert_eq!(fs::metadata(&disk).unwrap().len(), DISK);
    }
    assert_eq!(command(&arguments), ExitCode::SUCCESS);
    assert_eq!(fs::metadata(&disk).unwrap().len(), 2 * DISK);
    assert_eq!(command(&arguments[..2]), ExitCode::from(2));
    let mut unparsable = arguments.clone();
    unparsable[2] = "64GiB".into();
    assert_eq!(command(&unparsable), ExitCode::from(2));
}
