use super::*;
use crate::media::{VirtBootMediaConfig, WritableMedia};
use crate::snapshot_pair::managed::{runtime, LockedPair};
use crate::snapshot_pair::{create_snapshot, restore_snapshot, snapshot_pair_tests::Scratch};

fn fixture(tag: &str) -> (Scratch, PathBuf, PathBuf) {
    let s = Scratch::new(tag);
    fs::create_dir_all(s.path("vm/disks")).unwrap();
    fs::create_dir_all(s.path("vm/metadata")).unwrap();
    let disk = s.write("vm/disks/disk.raw", b"restored-disk");
    let vars = s.write("vm/metadata/vars.fd", b"restored-vars");
    create_snapshot(&disk, &vars, &s.path("snapshot"), "move", false, 1024).unwrap();
    fs::write(&disk, b"stale-disk").unwrap();
    fs::write(&vars, b"stale-vars").unwrap();
    restore_snapshot(&s.path("snapshot"), &disk, &vars, false).unwrap();
    (
        s,
        fs::canonicalize(disk).unwrap(),
        fs::canonicalize(vars).unwrap(),
    )
}

fn moved(s: &Scratch) -> (PathBuf, PathBuf) {
    fs::rename(s.path("vm"), s.path("moved")).unwrap();
    (
        s.path("moved/disks/disk.raw"),
        s.path("moved/metadata/vars.fd"),
    )
}

fn assert_current(disk: &Path, vars: &Path) {
    let pair = LockedPair::open(disk, vars).unwrap();
    let (d, v) = pair.paths().unwrap();
    assert_eq!(fs::read(d).unwrap(), b"restored-disk");
    assert_eq!(fs::read(v).unwrap(), b"restored-vars");
}

#[test]
fn whole_bundle_move_preserves_snapshot_and_native_selection() {
    let (s, _, _) = fixture("identity-move");
    let (disk, vars) = moved(&s);
    assert_current(&disk, &vars);
    create_snapshot(&disk, &vars, &s.path("after-move"), "move", false, 1024).unwrap();
    assert_eq!(
        fs::read(s.path("after-move/disk.raw")).unwrap(),
        b"restored-disk"
    );
    assert_eq!(
        fs::read(s.path("after-move/vars.fd")).unwrap(),
        b"restored-vars"
    );
    let mut media = VirtBootMediaConfig::qemu_defaults();
    media.nvme_disk = Some(WritableMedia::new(disk));
    media.flash_vars = WritableMedia::new(vars);
    let _owner = runtime::acquire(&mut media).unwrap();
    assert_eq!(
        fs::read(&media.nvme_disk.as_ref().unwrap().path).unwrap(),
        b"restored-disk"
    );
    assert_eq!(
        media.flash_vars.read_bounded(128).unwrap(),
        b"restored-vars"
    );
}

#[test]
fn legacy_identity_upgrades_before_a_bundle_move() {
    let (s, disk, vars) = fixture("identity-upgrade");
    fs::rename(stable_root(&disk, &vars), legacy_root(&disk, &vars)).unwrap();
    assert_current(&disk, &vars);
    assert!(!legacy_root(&disk, &vars).exists());
    let (disk, vars) = moved(&s);
    assert_current(&disk, &vars);
}

#[test]
fn already_moved_legacy_identity_refuses_instead_of_guessing() {
    let (s, disk, vars) = fixture("identity-orphan");
    fs::rename(stable_root(&disk, &vars), legacy_root(&disk, &vars)).unwrap();
    let (disk, vars) = moved(&s);
    assert!(LockedPair::open(&disk, &vars).is_err());
    assert!(create_snapshot(&disk, &vars, &s.path("wrong"), "move", false, 1024).is_err());
}

#[test]
fn changing_relative_vars_binding_refuses_instead_of_falling_back() {
    let (s, disk, vars) = fixture("identity-rebind");
    let renamed = s.path("vm/metadata/renamed.fd");
    fs::rename(vars, &renamed).unwrap();
    assert!(LockedPair::open(&disk, &renamed).is_err());
}

#[test]
fn simultaneous_old_and_new_roots_are_not_silently_prioritized() {
    let (_s, disk, vars) = fixture("identity-conflict");
    super::super::private_directory(&legacy_root(&disk, &vars), true).unwrap();
    assert!(LockedPair::open(&disk, &vars).is_err());
    assert_eq!(
        fs::read(stable_root(&disk, &vars).join("current/disk.raw")).unwrap(),
        b"restored-disk"
    );
}
