use super::*;
use crate::snapshot_pair::{create_snapshot, restore_snapshot, verify_snapshot};

#[test]
fn export_after_restore_reads_selected_generation_not_logical_originals() {
    let s = Scratch::new("managed-export-selected");
    fs::create_dir_all(s.path("vm/disks")).unwrap();
    fs::create_dir_all(s.path("vm/metadata")).unwrap();
    let disk = s.write("vm/disks/disk.raw", b"restored-disk");
    let vars = s.write("vm/metadata/vars.fd", b"restored-vars");
    let source = s.path("source.snapshot");
    create_snapshot(&disk, &vars, &source, "vm", false, 1024).unwrap();

    fs::write(&disk, b"pre-restore-stale-disk").unwrap();
    fs::write(&vars, b"pre-restore-stale-vars").unwrap();
    restore_snapshot(&source, &disk, &vars, false).unwrap();
    fs::write(&disk, b"logical-original-stale-disk").unwrap();
    fs::write(&vars, b"logical-original-stale-vars").unwrap();

    let exported = s.path("external.snapshot");
    let manifest = create_snapshot(&disk, &vars, &exported, "vm", false, 1024).unwrap();
    assert_eq!(verify_snapshot(&exported).unwrap(), manifest);
    assert_eq!(
        fs::read(exported.join("disk.raw")).unwrap(),
        b"restored-disk"
    );
    assert_eq!(
        fs::read(exported.join("vars.fd")).unwrap(),
        b"restored-vars"
    );
    assert_eq!(fs::read(disk).unwrap(), b"logical-original-stale-disk");
    assert_eq!(fs::read(vars).unwrap(), b"logical-original-stale-vars");
}

#[test]
fn export_refuses_while_another_owner_holds_the_selected_pair() {
    let s = Scratch::new("managed-export-owned");
    let disk = s.write("disk.raw", b"disk");
    let vars = s.write("vars.fd", b"vars");
    let _owner = LockedPair::open(&disk, &vars).unwrap();
    let exported = s.path("external.snapshot");
    assert!(create_snapshot(&disk, &vars, &exported, "vm", false, 1024).is_err());
    assert!(!exported.exists());
}
