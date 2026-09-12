use super::*;
use crate::media::WritableMedia;
use crate::snapshot_pair::{create_snapshot, snapshot_pair_tests::Scratch};
use std::path::PathBuf;

fn fixture(tag: &str) -> (Scratch, VirtBootMediaConfig, PathBuf) {
    let s = Scratch::new(tag);
    let disk = s.write("disk", b"old-disk");
    let vars = s.write("vars", b"old-vars");
    let sd = s.write("source-disk", b"new-disk");
    let sv = s.write("source-vars", b"new-vars");
    let snapshot = s.path("snapshot");
    create_snapshot(&sd, &sv, &snapshot, "runtime", false, 1024).unwrap();
    let mut media = VirtBootMediaConfig::qemu_defaults();
    media.nvme_disk = Some(WritableMedia::new(disk).with_write_back(true));
    media.flash_vars = WritableMedia::new(vars).with_write_back(true);
    (s, media, snapshot)
}

#[test]
fn runtime_selects_restored_pair_and_persists_vars_there() {
    let (_s, mut media, snapshot) = fixture("managed-runtime-selected");
    let disk = media.nvme_disk.as_ref().unwrap().path.clone();
    let vars = media.flash_vars.path.clone();
    LockedPair::open(&disk, &vars)
        .unwrap()
        .restore(&snapshot)
        .unwrap();
    let guard = acquire(&mut media).unwrap();
    assert_eq!(
        fs::read(&media.nvme_disk.as_ref().unwrap().path).unwrap(),
        b"new-disk"
    );
    assert_eq!(media.flash_vars.read_bounded(128).unwrap(), b"new-vars");
    media.flash_vars.persist(b"guest-vars").unwrap();
    assert_eq!(fs::read(&vars).unwrap(), b"old-vars");
    assert!(LockedPair::open(&disk, &vars).is_err());
    assert!(MediaLease::acquire([media.nvme_disk.as_ref().unwrap().path.as_path()]).is_err());
    drop(guard);
    assert_eq!(
        fs::read(LockedPair::open(&disk, &vars).unwrap().paths().unwrap().1).unwrap(),
        b"guest-vars"
    );
}

#[test]
fn second_namespace_alone_uses_the_same_pair_selection() {
    let (_s, mut media, snapshot) = fixture("managed-runtime-second");
    media.nvme_target = media.nvme_disk.take();
    LockedPair::open(
        &media.nvme_target.as_ref().unwrap().path,
        &media.flash_vars.path,
    )
    .unwrap()
    .restore(&snapshot)
    .unwrap();
    let _guard = acquire(&mut media).unwrap();
    assert!(media.nvme_disk.is_none());
    assert_eq!(
        fs::read(&media.nvme_target.as_ref().unwrap().path).unwrap(),
        b"new-disk"
    );
}

#[test]
fn unmanaged_multi_disk_works_but_managed_shared_vars_are_rejected() {
    let (s, mut media, snapshot) = fixture("managed-runtime-multi");
    media.nvme_target = Some(WritableMedia::new(s.write("target", b"target")));
    drop(acquire(&mut media).unwrap());
    LockedPair::open(
        &media.nvme_disk.as_ref().unwrap().path,
        &media.flash_vars.path,
    )
    .unwrap()
    .restore(&snapshot)
    .unwrap();
    let original = media.clone();
    assert!(acquire(&mut media).is_err());
    assert_eq!(media, original);
}

#[test]
fn deleted_selected_directory_does_not_fall_back_to_originals() {
    let (_s, mut media, snapshot) = fixture("managed-runtime-missing");
    let mut pair = LockedPair::open(
        &media.nvme_disk.as_ref().unwrap().path,
        &media.flash_vars.path,
    )
    .unwrap();
    pair.restore(&snapshot).unwrap();
    let selected = pair.paths().unwrap().0;
    fs::remove_dir_all(selected.parent().unwrap()).unwrap();
    assert!(pair.paths().is_err());
    drop(pair);
    assert!(acquire(&mut media).is_err());
}

#[test]
fn incomplete_store_initialization_fails_closed() {
    let (_s, mut media, _) = fixture("managed-runtime-incomplete");
    let disk = fs::canonicalize(&media.nvme_disk.as_ref().unwrap().path).unwrap();
    let vars = fs::canonicalize(&media.flash_vars.path).unwrap();
    layout::private_directory(&layout::store_root(&disk, &vars), true).unwrap();
    assert!(acquire(&mut media).is_err());
}

#[test]
fn firmware_only_probe_does_not_require_default_vars_file() {
    let mut media = VirtBootMediaConfig::qemu_defaults();
    media.flash_vars.path = PathBuf::from("/no-such-vars-fixture");
    acquire(&mut media).unwrap();
}
