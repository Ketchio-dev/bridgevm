use super::{Scratch, QUOTA};
use crate::snapshot_pair::*;
use std::fs;

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
            let result = create_snapshot(&disk, &vars, &s.path("snapshot"), "vm", false, QUOTA);
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
    create_snapshot(&disk, &vars, &s.path("snapshot"), "vm", false, QUOTA).unwrap();
    restore_snapshot(&s.path("snapshot"), &disk, &vars, false).unwrap();
    let pair = managed::LockedPair::open(&disk, &vars).unwrap();
    let (selected_disk, selected_vars) = pair.paths().unwrap();
    let destination = selected_disk
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    drop(pair);
    assert!(create_snapshot(&disk, &vars, &destination, "vm", false, QUOTA).is_err());
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
    let manifest =
        create_snapshot(&disk, &vars, &s.path("alias/snapshot"), "vm", false, QUOTA).unwrap();
    assert_eq!(
        verify_snapshot(&s.path("outputs/snapshot")).unwrap(),
        manifest
    );
    fs::create_dir(s.path("outputs/.other.staging")).unwrap();
    let disk = s.write("outputs/.other.staging/source-disk", b"own-disk");
    assert!(create_snapshot(&disk, &vars, &s.path("alias/other"), "vm", false, QUOTA).is_err());
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
    assert!(create_snapshot(&disk, &vars, &s.path("snapshot"), "vm", false, QUOTA).is_err());
    assert!(fs::symlink_metadata(s.path("snapshot"))
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(fs::read(marker).unwrap(), b"unrelated");
    assert_eq!(fs::read(disk).unwrap(), b"own-disk");
    assert_eq!(fs::read(vars).unwrap(), b"own-vars");
}
