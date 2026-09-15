use super::*;
use crate::snapshot_pair::{create_snapshot, restore_snapshot, snapshot_pair_tests::Scratch};

#[test]
fn separate_disk_and_metadata_directories_use_one_selected_generation() {
    let s = Scratch::new("managed-api-layout");
    fs::create_dir(s.path("disks")).unwrap();
    fs::create_dir(s.path("metadata")).unwrap();
    let disk = s.write("disks/hvf-target.raw", b"disk");
    let vars = s.write("metadata/hvf-vars.fd", b"vars");
    create_snapshot(&disk, &vars, &s.path("snapshot"), "vm", false, 1024).unwrap();
    restore_snapshot(&s.path("snapshot"), &disk, &vars, false).unwrap();
    let pair = LockedPair::open(&disk, &vars).unwrap();
    let (selected_disk, selected_vars) = pair.paths().unwrap();
    assert_eq!(selected_disk.parent(), selected_vars.parent());
    assert!(MediaLease::acquire([selected_vars.as_path()]).is_err());
    assert_eq!(fs::read(selected_disk).unwrap(), b"disk");
    assert_eq!(fs::read(selected_vars).unwrap(), b"vars");
}

#[test]
fn overflowing_declared_pair_size_is_refused_without_publication() {
    let s = Scratch::new("managed-api-overflow");
    let disk = s.write("disk", b"disk");
    let vars = s.write("vars", b"vars");
    let snap = s.path("snapshot");
    create_snapshot(&disk, &vars, &snap, "vm", false, 1024).unwrap();
    let manifest_path = snap.join("manifest.json");
    let manifest = fs::read_to_string(&manifest_path).unwrap();
    let corrupt = manifest.replace("\"disk_bytes\": 4", "\"disk_bytes\": 18446744073709551615");
    assert_ne!(corrupt, manifest);
    fs::write(manifest_path, corrupt).unwrap();
    assert!(restore_snapshot(&snap, &disk, &vars, false).is_err());
    let pair = LockedPair::open(&disk, &vars).unwrap();
    assert_eq!(
        pair.paths().unwrap(),
        (
            fs::canonicalize(disk).unwrap(),
            fs::canonicalize(vars).unwrap()
        )
    );
}
