use super::*;
use crate::media::{VirtBootMediaConfig, WritableMedia};
use crate::snapshot_pair::{create_snapshot, restore_snapshot, snapshot_pair_tests::Scratch};

#[test]
fn public_api_and_native_runtime_share_the_selected_pair() {
    let s = Scratch::new("managed-api-runtime");
    let disk = s.write("disk", b"original disk");
    let vars = s.write("vars", b"original vars");
    let original = create_snapshot(&disk, &vars, &s.path("snap"), "vm", false, 1024).unwrap();
    fs::write(&disk, b"clobbered disk").unwrap();
    fs::write(&vars, b"clobbered vars").unwrap();
    assert_eq!(
        restore_snapshot(&s.path("snap"), &disk, &vars, false).unwrap(),
        original
    );
    let mut media = VirtBootMediaConfig::qemu_defaults();
    media.nvme_disk = Some(WritableMedia::new(&disk).with_write_back(true));
    media.flash_vars = WritableMedia::new(&vars).with_write_back(true);
    let runtime = runtime::acquire(&mut media).unwrap();
    assert_eq!(
        fs::read(&media.nvme_disk.as_ref().unwrap().path).unwrap(),
        b"original disk"
    );
    assert_eq!(
        media.flash_vars.read_bounded(128).unwrap(),
        b"original vars"
    );
    assert!(create_snapshot(&disk, &vars, &s.path("busy"), "vm", false, 1024).is_err());
    fs::write(&media.nvme_disk.as_ref().unwrap().path, b"guest disk").unwrap();
    media.flash_vars.persist(b"guest vars").unwrap();
    drop(runtime);
    let changed = create_snapshot(&disk, &vars, &s.path("changed"), "vm", false, 1024).unwrap();
    assert_eq!(fs::read(s.path("changed/disk.raw")).unwrap(), b"guest disk");
    assert_eq!(fs::read(s.path("changed/vars.fd")).unwrap(), b"guest vars");
    assert_ne!(changed.disk_sha256, original.disk_sha256);
    assert_eq!(
        restore_snapshot(&s.path("snap"), &disk, &vars, false).unwrap(),
        original
    );
    assert_eq!(
        create_snapshot(&disk, &vars, &s.path("again"), "vm", false, 1024).unwrap(),
        original
    );
    assert_eq!(fs::read(disk).unwrap(), b"clobbered disk");
    assert_eq!(fs::read(vars).unwrap(), b"clobbered vars");
}

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
