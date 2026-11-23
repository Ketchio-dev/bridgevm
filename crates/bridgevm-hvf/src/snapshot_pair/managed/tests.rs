use super::*;
use crate::snapshot_pair::tests::Scratch;
use crate::snapshot_pair::{create_snapshot, restore_snapshot, verify_snapshot};

fn fixture(tag: &str) -> (Scratch, PathBuf, PathBuf, PathBuf) {
    let s = Scratch::new(tag);
    let disk = s.write("disk", b"old-disk");
    let vars = s.write("vars", b"old-vars");
    let sd = s.write("source-disk", b"new-disk");
    let sv = s.write("source-vars", b"new-vars");
    let snapshot = s.path("snapshot");
    create_snapshot(&sd, &sv, &snapshot, "managed").unwrap();
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
        crate::snapshot_pair::publish::publish(staged, current)?;
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
    assert!(crate::snapshot_pair::restore_snapshot(&snapshot, &disk, &vars).is_err());
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

#[test]
fn separate_disk_and_metadata_directories_use_one_selected_generation() {
    let s = Scratch::new("managed-api-layout");
    fs::create_dir(s.path("disks")).unwrap();
    fs::create_dir(s.path("metadata")).unwrap();
    let disk = s.write("disks/hvf-target.raw", b"disk");
    let vars = s.write("metadata/hvf-vars.fd", b"vars");
    create_snapshot(&disk, &vars, &s.path("snapshot"), "vm").unwrap();
    restore_snapshot(&s.path("snapshot"), &disk, &vars).unwrap();
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
    create_snapshot(&disk, &vars, &snap, "vm").unwrap();
    let manifest_path = snap.join("manifest.json");
    let manifest = fs::read_to_string(&manifest_path).unwrap();
    let corrupt = manifest.replace("\"disk_bytes\": 4", "\"disk_bytes\": 18446744073709551615");
    assert_ne!(corrupt, manifest);
    fs::write(manifest_path, corrupt).unwrap();
    assert!(restore_snapshot(&snap, &disk, &vars).is_err());
    let pair = LockedPair::open(&disk, &vars).unwrap();
    assert_eq!(
        pair.paths().unwrap(),
        (
            fs::canonicalize(disk).unwrap(),
            fs::canonicalize(vars).unwrap()
        )
    );
}

#[test]
fn export_after_restore_reads_selected_generation_not_logical_originals() {
    let s = Scratch::new("managed-export-selected");
    fs::create_dir_all(s.path("vm/disks")).unwrap();
    fs::create_dir_all(s.path("vm/metadata")).unwrap();
    let disk = s.write("vm/disks/disk.raw", b"restored-disk");
    let vars = s.write("vm/metadata/vars.fd", b"restored-vars");
    let source = s.path("source.snapshot");
    create_snapshot(&disk, &vars, &source, "vm").unwrap();

    fs::write(&disk, b"pre-restore-stale-disk").unwrap();
    fs::write(&vars, b"pre-restore-stale-vars").unwrap();
    restore_snapshot(&source, &disk, &vars).unwrap();
    fs::write(&disk, b"logical-original-stale-disk").unwrap();
    fs::write(&vars, b"logical-original-stale-vars").unwrap();

    let exported = s.path("external.snapshot");
    let manifest = create_snapshot(&disk, &vars, &exported, "vm").unwrap();
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
    assert!(create_snapshot(&disk, &vars, &exported, "vm").is_err());
    assert!(!exported.exists());
}

fn assert_owned(path: &Path) {
    assert_eq!(
        MediaLease::acquire([path]).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
}

fn alias(s: &Scratch, path: &Path, name: &str) -> PathBuf {
    let alias = s.path(name);
    fs::hard_link(path, &alias).unwrap();
    alias
}

#[test]
fn repeated_restore_owns_new_inodes_and_releases_old_generations() {
    let (s, disk, vars, snapshot) = fixture("managed-repeat-ownership");
    let mut pair = LockedPair::open(&disk, &vars).unwrap();
    let mut previous: Option<(PathBuf, PathBuf)> = None;
    for generation in 0..3 {
        pair.restore(&snapshot).unwrap();
        let (selected_disk, selected_vars) = pair.paths().unwrap();
        let disk_alias = alias(&s, &selected_disk, &format!("disk-alias-{generation}"));
        let vars_alias = alias(&s, &selected_vars, &format!("vars-alias-{generation}"));
        for path in [&selected_disk, &selected_vars, &disk_alias, &vars_alias] {
            assert_owned(path);
        }
        assert_owned(&disk);
        assert_owned(&vars);
        if let Some((old_disk, old_vars)) = previous.take() {
            MediaLease::acquire([old_disk.as_path(), old_vars.as_path()]).unwrap();
        }
        previous = Some((disk_alias, vars_alias));
    }
    let selected = pair.paths().unwrap();
    drop(pair);
    let (disk_alias, vars_alias) = previous.unwrap();
    MediaLease::acquire([
        selected.0.as_path(),
        selected.1.as_path(),
        disk_alias.as_path(),
        vars_alias.as_path(),
    ])
    .unwrap();
}

#[test]
fn publication_keeps_staged_and_selected_aliases_owned() {
    let (s, disk, vars, snapshot) = fixture("managed-publish-ownership");
    let mut pair = LockedPair::open(&disk, &vars).unwrap();
    for generation in 0..2 {
        let old = pair.paths().unwrap();
        pair.restore_using(&snapshot, |staged, current| {
            let staged_alias = alias(
                &s,
                &staged.join("disk.raw"),
                &format!("publishing-alias-{generation}"),
            );
            assert_owned(&old.0);
            assert_owned(&old.1);
            assert_owned(&staged.join("disk.raw"));
            assert_owned(&staged.join("vars.fd"));
            assert_owned(&staged_alias);
            crate::snapshot_pair::publish::publish(staged, current)?;
            assert_owned(&current.join("disk.raw"));
            assert_owned(&current.join("vars.fd"));
            assert_owned(&staged_alias);
            Ok(())
        })
        .unwrap();
    }
}

#[test]
fn publication_errors_retain_old_and_new_inode_ownership() {
    for already_managed in [false, true] {
        for after_publish in [false, true] {
            let (s, disk, vars, snapshot) = fixture("managed-error-ownership");
            let mut pair = LockedPair::open(&disk, &vars).unwrap();
            if already_managed {
                pair.restore(&snapshot).unwrap();
            }
            let old = pair.paths().unwrap();
            let old_alias = alias(&s, &old.0, "old-alias");
            let staged_alias = s.path("staged-alias");
            let result = pair.restore_using(&snapshot, |staged, current| {
                fs::hard_link(staged.join("disk.raw"), &staged_alias)?;
                if after_publish {
                    crate::snapshot_pair::publish::publish(staged, current)?;
                }
                Err(io::Error::other("injected publication uncertainty"))
            });
            assert!(result.is_err());
            let selected = pair.paths().unwrap();
            for path in [&selected.0, &selected.1, &old_alias, &staged_alias] {
                assert_owned(path);
            }
            pair.restore(&snapshot).unwrap();
            MediaLease::acquire([staged_alias.as_path()]).unwrap();
            if already_managed {
                MediaLease::acquire([old_alias.as_path()]).unwrap();
            } else {
                assert_owned(&old_alias);
            }
            let recovered = pair.paths().unwrap();
            assert_owned(&recovered.0);
            assert_owned(&recovered.1);
            drop(pair);
            MediaLease::acquire([old_alias.as_path(), staged_alias.as_path()]).unwrap();
        }
    }
}

mod identity {
    use super::*;
    use crate::media::{VirtBootMediaConfig, WritableMedia};

    fn fixture(tag: &str) -> (Scratch, PathBuf, PathBuf) {
        let s = Scratch::new(tag);
        fs::create_dir_all(s.path("vm/disks")).unwrap();
        fs::create_dir_all(s.path("vm/metadata")).unwrap();
        let disk = s.write("vm/disks/disk.raw", b"restored-disk");
        let vars = s.write("vm/metadata/vars.fd", b"restored-vars");
        create_snapshot(&disk, &vars, &s.path("snapshot"), "move").unwrap();
        fs::write(&disk, b"stale-disk").unwrap();
        fs::write(&vars, b"stale-vars").unwrap();
        restore_snapshot(&s.path("snapshot"), &disk, &vars).unwrap();
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
        create_snapshot(&disk, &vars, &s.path("after-move"), "move").unwrap();
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
        assert!(create_snapshot(&disk, &vars, &s.path("wrong"), "move").is_err());
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
        private_directory(&legacy_root(&disk, &vars), true).unwrap();
        assert!(LockedPair::open(&disk, &vars).is_err());
        assert_eq!(
            fs::read(stable_root(&disk, &vars).join("current/disk.raw")).unwrap(),
            b"restored-disk"
        );
    }
}
