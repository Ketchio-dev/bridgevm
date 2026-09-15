use super::*;
use crate::snapshot_pair::restore_snapshot;
use std::path::Path;

fn assert_owned(path: &Path) {
    assert_eq!(
        MediaLease::acquire([path]).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
}

fn vars_case(tag: &str, managed: bool, second: bool) {
    let (s, mut media, snapshot) = fixture(tag);
    let logical_vars = media.flash_vars.path.clone();
    if managed {
        LockedPair::open(&media.nvme_disk.as_ref().unwrap().path, &logical_vars)
            .unwrap()
            .restore(&snapshot)
            .unwrap();
    }
    if second {
        media.nvme_target = Some(WritableMedia::new(s.write("second", b"second")));
    }
    let mut guard = acquire(&mut media).unwrap();
    assert_owned(&media.flash_vars.path);
    guard
        .persist(RuntimeMediaSlot::Vars, b"persisted-vars")
        .unwrap();
    let alias = s.path("persisted-alias");
    fs::hard_link(&media.flash_vars.path, &alias).unwrap();
    assert_owned(&media.flash_vars.path);
    assert_owned(&alias);
    assert_owned(&media.nvme_disk.as_ref().unwrap().path);
    if let Some(second) = &media.nvme_target {
        assert_owned(&second.path);
    }
    if managed {
        assert_eq!(fs::read(logical_vars).unwrap(), b"old-vars");
    }
    drop(guard);
    MediaLease::acquire([alias.as_path(), media.flash_vars.path.as_path()]).unwrap();
}

#[test]
fn unmanaged_vars_persistence_retains_the_new_inode() {
    vars_case("runtime-persist-unmanaged", false, false);
}

#[test]
fn managed_vars_persistence_retains_the_new_inode() {
    vars_case("runtime-persist-managed", true, false);
}

#[test]
fn two_disk_vars_persistence_retains_the_new_inode() {
    vars_case("runtime-persist-two-disks", false, true);
}

#[test]
fn sequential_slot_writes_retain_every_input_and_snapshot_output() {
    let (s, mut media, _) = fixture("runtime-persist-all-slots");
    media.nvme_target =
        Some(WritableMedia::new(s.write("target", b"target")).with_write_back(true));
    media.flash_vars.snapshot_path = Some(s.path("vars-out"));
    media.nvme_disk.as_mut().unwrap().snapshot_path = Some(s.path("primary-out"));
    media.nvme_target.as_mut().unwrap().snapshot_path = Some(s.path("target-out"));
    let mut guard = acquire(&mut media).unwrap();
    let mut outputs = Vec::new();
    for (slot, content) in [
        (RuntimeMediaSlot::Vars, b"vars".as_slice()),
        (RuntimeMediaSlot::Primary, b"primary".as_slice()),
        (RuntimeMediaSlot::Target, b"target".as_slice()),
    ] {
        let writes = guard.persist(slot, content).unwrap();
        assert_eq!(writes.len(), 2);
        assert_eq!(writes[0].kind, crate::media::MediaWriteKind::Snapshot);
        assert_eq!(writes[1].kind, crate::media::MediaWriteKind::WriteBack);
        for write in writes {
            assert_eq!(fs::read(&write.path).unwrap(), content);
            let alias = s.path(&format!("output-alias-{}", outputs.len()));
            fs::hard_link(&write.path, &alias).unwrap();
            outputs.push((write.path, alias));
        }
        for (path, alias) in &outputs {
            assert_owned(path);
            assert_owned(alias);
        }
    }
    let retained = guard.retained.len();
    for _ in 0..8 {
        let old_alias = s.path("old-vars");
        fs::hard_link(&media.flash_vars.path, &old_alias).unwrap();
        guard
            .persist(RuntimeMediaSlot::Vars, b"updated-vars")
            .unwrap();
        assert_eq!(guard.retained.len(), retained);
        MediaLease::acquire([old_alias.as_path()]).unwrap();
        fs::remove_file(old_alias).unwrap();
    }
    drop(guard);
    for (path, alias) in outputs {
        MediaLease::acquire([path.as_path(), alias.as_path()]).unwrap();
    }
}

#[test]
fn no_disk_noop_needs_no_files_and_actual_output_is_owned_lazily() {
    let s = Scratch::new("runtime-persist-no-disk");
    let mut media = VirtBootMediaConfig::qemu_defaults();
    media.flash_vars.path = s.path("absent-parent/absent-vars");
    let mut guard = acquire(&mut media).unwrap();
    assert!(guard
        .persist(RuntimeMediaSlot::Vars, b"unused")
        .unwrap()
        .is_empty());
    assert_eq!(
        guard
            .persist(RuntimeMediaSlot::Primary, b"absent")
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    drop(guard);
    media.flash_vars.snapshot_path = Some(s.path("vars-output"));
    let mut guard = acquire(&mut media).unwrap();
    guard
        .persist(RuntimeMediaSlot::Vars, b"saved-vars")
        .unwrap();
    assert_owned(&s.path("vars-output"));
    assert!(!media.flash_vars.path.exists());
}

#[test]
fn failed_writeback_keeps_the_already_published_snapshot_owned() {
    let s = Scratch::new("runtime-persist-partial-policy");
    let vars = s.write("vars", b"original");
    let output = s.path("snapshot-output");
    let mut media = VirtBootMediaConfig::qemu_defaults();
    media.flash_vars = WritableMedia::new(&vars)
        .with_write_back(true)
        .with_snapshot_path(Some(&output));
    let mut guard = acquire(&mut media).unwrap();
    let competitor = MediaLease::acquire([vars.as_path()]).unwrap();
    assert_eq!(
        guard
            .persist(RuntimeMediaSlot::Vars, b"saved")
            .unwrap_err()
            .kind(),
        io::ErrorKind::WouldBlock
    );
    assert_eq!(fs::read(&vars).unwrap(), b"original");
    assert_eq!(fs::read(&output).unwrap(), b"saved");
    let alias = s.path("published-output-alias");
    fs::hard_link(&output, &alias).unwrap();
    assert_owned(&alias);
    drop(competitor);
    guard.persist(RuntimeMediaSlot::Vars, b"retried").unwrap();
    MediaLease::acquire([alias.as_path()]).unwrap();
    assert_owned(&vars);
    assert_owned(&output);
}

#[test]
fn captured_policy_cannot_be_retargeted_by_mutating_the_config() {
    let (s, mut media, _) = fixture("runtime-persist-captured-policy");
    let original = media.flash_vars.path.clone();
    let unrelated = s.write("unrelated", b"keep");
    let mut guard = acquire(&mut media).unwrap();
    media.flash_vars.path = unrelated.clone();
    guard
        .persist(RuntimeMediaSlot::Vars, b"owned-vars")
        .unwrap();
    assert_eq!(fs::read(&original).unwrap(), b"owned-vars");
    assert_eq!(fs::read(unrelated).unwrap(), b"keep");
}

#[path = "managed_pair_runtime_atomic_tests.rs"]
mod atomic;

#[test]
fn runtime_selects_restored_pair_and_persists_vars_there() {
    let (_s, mut media, snapshot) = fixture("managed-runtime-selected");
    let disk = media.nvme_disk.as_ref().unwrap().path.clone();
    let vars = media.flash_vars.path.clone();
    LockedPair::open(&disk, &vars)
        .unwrap()
        .restore(&snapshot)
        .unwrap();
    let mut guard = acquire(&mut media).unwrap();
    assert_eq!(
        fs::read(&media.nvme_disk.as_ref().unwrap().path).unwrap(),
        b"new-disk"
    );
    assert_eq!(media.flash_vars.read_bounded(128).unwrap(), b"new-vars");
    guard
        .persist(RuntimeMediaSlot::Vars, b"guest-vars")
        .unwrap();
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
    let mut runtime = acquire(&mut media).unwrap();
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
    runtime
        .persist(RuntimeMediaSlot::Vars, b"guest vars")
        .unwrap();
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
