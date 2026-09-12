use super::*;
use crate::snapshot_pair::{create_snapshot, snapshot_pair_tests::Scratch};

fn fixture(tag: &str) -> (Scratch, PathBuf, PathBuf, PathBuf) {
    let s = Scratch::new(tag);
    let disk = s.write("disk", b"old-disk");
    let vars = s.write("vars", b"old-vars");
    let sd = s.write("source-disk", b"new-disk");
    let sv = s.write("source-vars", b"new-vars");
    let snapshot = s.path("snapshot");
    create_snapshot(&sd, &sv, &snapshot, "managed", false, 1024).unwrap();
    (s, disk, vars, snapshot)
}

fn contents(pair: &LockedPair) -> (Vec<u8>, Vec<u8>) {
    let (disk, vars) = pair.paths().unwrap();
    (fs::read(disk).unwrap(), fs::read(vars).unwrap())
}

#[test]
fn selects_both_restored_files_and_preserves_originals() {
    let (_s, disk, vars, snapshot) = fixture("managed-select");
    let mut pair = LockedPair::open(&disk, &vars).unwrap();
    assert_eq!(
        contents(&pair),
        (b"old-disk".to_vec(), b"old-vars".to_vec())
    );
    pair.restore(&snapshot).unwrap();
    assert_eq!(
        contents(&pair),
        (b"new-disk".to_vec(), b"new-vars".to_vec())
    );
    assert_eq!(fs::read(&disk).unwrap(), b"old-disk");
    assert_eq!(fs::read(&vars).unwrap(), b"old-vars");
    drop(pair);
    let reopened = LockedPair::open(&disk, &vars).unwrap();
    assert_eq!(
        contents(&reopened),
        (b"new-disk".to_vec(), b"new-vars".to_vec())
    );
}

#[test]
fn failed_publication_preserves_selected_pair() {
    let (_s, disk, vars, snapshot) = fixture("managed-before");
    let mut pair = LockedPair::open(&disk, &vars).unwrap();
    let result = pair.restore_using(&snapshot, |_, _| {
        Err(io::Error::other("injected before publish"))
    });
    assert!(result.is_err());
    assert_eq!(
        contents(&pair),
        (b"old-disk".to_vec(), b"old-vars".to_vec())
    );
    pair.restore(&snapshot).unwrap();
    let (d, v) = pair.paths().unwrap();
    fs::write(d, b"guest-disk").unwrap();
    fs::write(v, b"guest-vars").unwrap();
    assert!(pair
        .restore_using(&snapshot, |_, _| Err(io::Error::other("again")))
        .is_err());
    assert_eq!(
        contents(&pair),
        (b"guest-disk".to_vec(), b"guest-vars".to_vec())
    );
}

#[test]
fn error_after_publication_still_selects_one_complete_pair() {
    let (_s, disk, vars, snapshot) = fixture("managed-after");
    let mut pair = LockedPair::open(&disk, &vars).unwrap();
    pair.restore(&snapshot).unwrap();
    let (d, v) = pair.paths().unwrap();
    fs::write(d, b"guest-disk").unwrap();
    fs::write(v, b"guest-vars").unwrap();
    let result = pair.restore_using(&snapshot, |staged, current| {
        crate::snapshot_pair::snapshot_publish::publish(staged, current)?;
        Err(io::Error::other("injected after publish"))
    });
    assert!(result.is_err());
    assert_eq!(
        contents(&pair),
        (b"new-disk".to_vec(), b"new-vars".to_vec())
    );
}

#[test]
fn corrupt_source_and_missing_selected_file_fail_closed() {
    let (_s, disk, vars, snapshot) = fixture("managed-corrupt");
    let mut pair = LockedPair::open(&disk, &vars).unwrap();
    pair.restore(&snapshot).unwrap();
    fs::write(snapshot.join("vars.fd"), b"corrupt").unwrap();
    assert!(pair.restore(&snapshot).is_err());
    assert_eq!(
        contents(&pair),
        (b"new-disk".to_vec(), b"new-vars".to_vec())
    );
    fs::remove_file(pair.paths().unwrap().1).unwrap();
    assert!(pair.paths().is_err());
    drop(pair);
    assert!(LockedPair::open(&disk, &vars).is_err());
}

#[test]
fn selected_directory_symlink_is_not_followed() {
    let (s, disk, vars, _) = fixture("managed-symlink");
    let pair = LockedPair::open(&disk, &vars).unwrap();
    private_directory(&pair.root, true).unwrap();
    std::os::unix::fs::symlink(&s.0, pair.root.join("current")).unwrap();
    assert!(pair.paths().is_err());
}

#[test]
fn logical_pair_lease_excludes_competing_readers_and_legacy_restore() {
    let (_s, disk, vars, snapshot) = fixture("managed-lock");
    let pair = LockedPair::open(&disk, &vars).unwrap();
    assert!(LockedPair::open(&disk, &vars).is_err());
    assert!(crate::snapshot_pair::restore_snapshot(&snapshot, &disk, &vars, false).is_err());
    drop(pair);
    LockedPair::open(&disk, &vars).unwrap();
}

#[test]
fn disk_vars_alias_is_rejected() {
    let (s, disk, _, _) = fixture("managed-alias");
    let alias = s.path("alias");
    fs::hard_link(&disk, &alias).unwrap();
    assert!(LockedPair::open(&disk, &alias).is_err());
}
