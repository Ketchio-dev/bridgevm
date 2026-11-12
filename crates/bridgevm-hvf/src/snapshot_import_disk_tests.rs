use super::*;
use crate::snapshot_pair::{create_snapshot, managed::LockedPair, restore_snapshot};

#[test]
fn disk_only_import_selects_restored_pair_and_refuses_owner() {
    let root = std::env::temp_dir().join(format!("import-disk-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let disk = root.join("disk.raw");
    let vars = root.join("vars.fd");
    fs::write(&disk, b"old").unwrap();
    fs::write(&vars, b"vars-old").unwrap();
    let snapshot = root.join("snapshot");
    create_snapshot(&disk, &vars, &snapshot, "test", false, u64::MAX).unwrap();
    fs::write(&disk, b"new").unwrap();
    restore_snapshot(&snapshot, &disk, &vars, false).unwrap();
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
