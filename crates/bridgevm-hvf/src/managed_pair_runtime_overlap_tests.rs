use super::*;
use crate::snapshot_pair::{create_snapshot, managed::LockedPair, snapshot_pair_tests::Scratch};
use std::os::unix::fs::symlink;
fn fixture(tag: &str) -> (Scratch, VirtBootMediaConfig) {
    let scratch = Scratch::new(tag);
    let mut media = VirtBootMediaConfig::qemu_defaults();
    media.flash_vars = WritableMedia::new(scratch.write("vars", b"vars"));
    media.nvme_disk = Some(WritableMedia::new(scratch.write("disk", b"valuable disk")));
    (scratch, media)
}
#[test]
fn vars_snapshot_cannot_replace_disk_or_alias() {
    for alias in ["direct", "symlink", "hardlink"] {
        let (scratch, mut media) = fixture(alias);
        let disk = media.nvme_disk.as_ref().unwrap().path.clone();
        let output = if alias == "direct" {
            disk.clone()
        } else {
            let output = scratch.path("output");
            if alias == "symlink" {
                symlink(&disk, &output).unwrap()
            } else {
                fs::hard_link(&disk, &output).unwrap()
            }
            output
        };
        media.flash_vars.snapshot_path = Some(output);
        assert!(super::super::acquire(&mut media).is_err());
        assert_eq!(fs::read(disk).unwrap(), b"valuable disk");
    }
}
#[test]
fn absent_shared_output_is_refused_before_creation() {
    let (scratch, mut media) = fixture("absent-output");
    let output = scratch.path("missing/output");
    media.flash_vars.snapshot_path = Some(output.clone());
    media.nvme_disk.as_mut().unwrap().snapshot_path = Some(output.clone());
    assert!(super::super::acquire(&mut media).is_err());
    assert!(!output.exists());
}
#[test]
fn managed_selection_still_protects_logical_originals() {
    let (scratch, mut media) = fixture("managed-original");
    let disk = media.nvme_disk.as_ref().unwrap().path.clone();
    let vars = media.flash_vars.path.clone();
    let snapshot = scratch.path("snapshot");
    create_snapshot(&disk, &vars, &snapshot, "vm", false, 1024).unwrap();
    LockedPair::open(&disk, &vars)
        .unwrap()
        .restore(&snapshot)
        .unwrap();
    media.flash_vars.snapshot_path = Some(disk.clone());
    assert!(super::super::acquire(&mut media).is_err());
    assert_eq!(fs::read(&disk).unwrap(), b"valuable disk");
    assert_eq!(
        fs::read(LockedPair::open(&disk, &vars).unwrap().paths().unwrap().0).unwrap(),
        b"valuable disk"
    );
}
#[test]
fn late_symlink_retarget_is_refused_before_any_write() {
    let (scratch, mut media) = fixture("late-overlap");
    let disk = media.nvme_disk.as_ref().unwrap().path.clone();
    let output = scratch.path("output");
    media.flash_vars.snapshot_path = Some(output.clone());
    let mut owner = super::super::acquire(&mut media).unwrap();
    symlink(&disk, &output).unwrap();
    assert!(owner.persist(RuntimeMediaSlot::Vars, b"new vars").is_err());
    assert_eq!(fs::read(disk).unwrap(), b"valuable disk");
    assert!(fs::symlink_metadata(output).unwrap().is_symlink());
}
#[test]
fn same_slot_snapshot_and_writeback_remain_supported() {
    let (_scratch, mut media) = fixture("same-slot");
    media.flash_vars.snapshot_path = Some(media.flash_vars.path.clone());
    media.flash_vars.write_back = true;
    let vars = media.flash_vars.path.clone();
    let mut owner = super::super::acquire(&mut media).unwrap();
    assert_eq!(
        owner
            .persist(RuntimeMediaSlot::Vars, b"new vars")
            .unwrap()
            .len(),
        2
    );
    assert_eq!(fs::read(vars).unwrap(), b"new vars");
}

#[test]
fn absent_outputs_through_parent_alias_are_refused() {
    let (scratch, mut media) = fixture("parent-alias-output");
    let directory = scratch.path("output-dir");
    fs::create_dir(&directory).unwrap();
    let alias = scratch.path("output-alias");
    symlink(&directory, &alias).unwrap();
    media.flash_vars.snapshot_path = Some(directory.join("output"));
    media.nvme_disk.as_mut().unwrap().snapshot_path = Some(alias.join("output"));
    assert!(super::super::acquire(&mut media).is_err());
    assert!(!directory.join("output").exists());
}
#[cfg(target_os = "macos")]
#[test]
fn absent_case_alias_outputs_are_refused_on_case_insensitive_volume() {
    let (scratch, mut media) = fixture("case-alias-output");
    let probe = scratch.write("probe", b"probe");
    if !scratch.path("PROBE").exists() {
        return;
    }
    fs::remove_file(probe).unwrap();
    media.flash_vars.snapshot_path = Some(scratch.path("output"));
    media.nvme_disk.as_mut().unwrap().snapshot_path = Some(scratch.path("OUTPUT"));
    assert!(super::super::acquire(&mut media).is_err());
    assert!(!scratch.path("output").exists());
}
