use super::*;
use crate::snapshot_pair::managed::{private_directory, stable_root};
use crate::snapshot_pair::tests::Scratch;
use crate::snapshot_pair::{create_snapshot, restore_snapshot};
use std::process::Command;

fn fixture(tag: &str) -> (Scratch, VirtBootMediaConfig, PathBuf) {
    let s = Scratch::new(tag);
    let disk = s.write("disk", b"old-disk");
    let vars = s.write("vars", b"old-vars");
    let sd = s.write("source-disk", b"new-disk");
    let sv = s.write("source-vars", b"new-vars");
    let snapshot = s.path("snapshot");
    create_snapshot(&sd, &sv, &snapshot, "runtime").unwrap();
    let mut media = VirtBootMediaConfig::qemu_defaults();
    media.nvme_disk = Some(WritableMedia::new(disk).with_write_back(true));
    media.flash_vars = WritableMedia::new(vars).with_write_back(true);
    (s, media, snapshot)
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
    private_directory(&stable_root(&disk, &vars), true).unwrap();
    assert!(acquire(&mut media).is_err());
}

#[test]
fn firmware_only_probe_does_not_require_default_vars_file() {
    let mut media = VirtBootMediaConfig::qemu_defaults();
    media.flash_vars.path = PathBuf::from("/no-such-vars-fixture");
    acquire(&mut media).unwrap();
}

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
fn restore_after_runtime_vars_write_interruption() {
    const CHILD: &str = "BRIDGEVM_TEST_INTERRUPTED_RUNTIME_VARS";
    const EXIT: i32 = 81;
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let mut media = VirtBootMediaConfig::qemu_defaults();
        media.nvme_disk = Some(WritableMedia::new(root.join("disk")).with_write_back(true));
        media.flash_vars = WritableMedia::new(root.join("vars")).with_write_back(true);
        let mut runtime = acquire(&mut media).unwrap();
        runtime
            .persist_using(
                RuntimeMediaSlot::Vars,
                b"interrupted vars write",
                |staged, _| {
                    assert_eq!(fs::read(staged).unwrap(), b"interrupted vars write");
                    std::process::exit(EXIT);
                },
            )
            .unwrap();
        panic!("the child did not enter the real runtime write path");
    }

    let (_scratch, media, snapshot) = fixture("managed-runtime-vars-abrupt-exit");
    let disk = &media.nvme_disk.as_ref().unwrap().path;
    let vars = &media.flash_vars.path;
    restore_snapshot(&snapshot, disk, vars).unwrap();
    let current = LockedPair::open(disk, vars).unwrap().paths().unwrap().1;
    let status = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "snapshot_pair::managed::runtime::tests::restore_after_runtime_vars_write_interruption",
        ])
        .env(CHILD, disk.parent().unwrap())
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(EXIT));
    assert_eq!(fs::read(&current).unwrap(), b"new-vars");
    let debris: Vec<_> = fs::read_dir(current.parent().unwrap())
        .unwrap()
        .map(|item| item.unwrap().path())
        .filter(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(".bridgevm-write-")
        })
        .collect();
    assert_eq!(debris.len(), 1);
    assert_eq!(fs::read(&debris[0]).unwrap(), b"interrupted vars write");
    let restored = restore_snapshot(&snapshot, disk, vars);
    assert!(
        restored.is_ok(),
        "restore should succeed after a runtime vars write interruption: {restored:?}"
    );
    assert_eq!(fs::read(current).unwrap(), b"new-vars");
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
    let original = create_snapshot(&disk, &vars, &s.path("snap"), "vm").unwrap();
    fs::write(&disk, b"clobbered disk").unwrap();
    fs::write(&vars, b"clobbered vars").unwrap();
    assert_eq!(
        restore_snapshot(&s.path("snap"), &disk, &vars).unwrap(),
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
    assert!(create_snapshot(&disk, &vars, &s.path("busy"), "vm").is_err());
    fs::write(&media.nvme_disk.as_ref().unwrap().path, b"guest disk").unwrap();
    runtime
        .persist(RuntimeMediaSlot::Vars, b"guest vars")
        .unwrap();
    drop(runtime);
    let changed = create_snapshot(&disk, &vars, &s.path("changed"), "vm").unwrap();
    assert_eq!(fs::read(s.path("changed/disk.raw")).unwrap(), b"guest disk");
    assert_eq!(fs::read(s.path("changed/vars.fd")).unwrap(), b"guest vars");
    assert_ne!(changed.disk_sha256, original.disk_sha256);
    assert_eq!(
        restore_snapshot(&s.path("snap"), &disk, &vars).unwrap(),
        original
    );
    assert_eq!(
        create_snapshot(&disk, &vars, &s.path("again"), "vm").unwrap(),
        original
    );
    assert_eq!(fs::read(disk).unwrap(), b"clobbered disk");
    assert_eq!(fs::read(vars).unwrap(), b"clobbered vars");
}

#[test]
fn atomic_publication_owns_both_inodes_and_recovers_from_uncertain_errors() {
    for managed in [false, true] {
        for after_rename in [false, true] {
            let (s, mut media, snapshot) = fixture("runtime-persist-atomic-errors");
            if managed {
                LockedPair::open(
                    &media.nvme_disk.as_ref().unwrap().path,
                    &media.flash_vars.path,
                )
                .unwrap()
                .restore(&snapshot)
                .unwrap();
            }
            let mut guard = acquire(&mut media).unwrap();
            let destination = media.flash_vars.path.clone();
            let old_bytes = fs::read(&destination).unwrap();
            let old_alias = s.path("old-alias");
            let new_alias = s.path("new-alias");
            fs::hard_link(&destination, &old_alias).unwrap();
            let result =
                guard.persist_using(RuntimeMediaSlot::Vars, b"next-vars", |staged, dest| {
                    fs::hard_link(staged, &new_alias)?;
                    for path in [staged, dest, old_alias.as_path(), new_alias.as_path()] {
                        assert_owned(path);
                    }
                    if after_rename {
                        fs::rename(staged, dest)?;
                        assert_owned(dest);
                        assert_owned(&new_alias);
                    }
                    Err(io::Error::other("injected publication error"))
                });
            assert!(result.is_err());
            assert_owned(&old_alias);
            assert_owned(&new_alias);
            assert_eq!(
                fs::read(&destination).unwrap(),
                if after_rename {
                    b"next-vars".to_vec()
                } else {
                    old_bytes
                }
            );
            guard
                .persist(RuntimeMediaSlot::Primary, b"next-disk")
                .unwrap();
            assert_owned(&destination);
            if after_rename {
                assert_owned(&new_alias);
            }
            guard
                .persist(RuntimeMediaSlot::Vars, b"recovered-vars")
                .unwrap();
            MediaLease::acquire([old_alias.as_path(), new_alias.as_path()]).unwrap();
            assert_owned(&destination);
            drop(guard);
            MediaLease::acquire([destination.as_path()]).unwrap();
        }
    }
}

#[test]
fn owned_staging_never_truncates_preexisting_temp_aliases() {
    let (s, mut media, _) = fixture("runtime-persist-temp-alias");
    let original = s.write("unrelated", b"preserve");
    fs::hard_link(&original, s.path(".vars.tmp")).unwrap();
    let mut guard = acquire(&mut media).unwrap();
    guard
        .persist(RuntimeMediaSlot::Vars, b"saved-vars")
        .unwrap();
    assert_eq!(fs::read(original).unwrap(), b"preserve");
    assert_eq!(fs::read(s.path(".vars.tmp")).unwrap(), b"preserve");
}

#[test]
fn staging_cleanup_does_not_remove_a_replacement_at_the_old_name() {
    let (_s, mut media, _) = fixture("runtime-persist-temp-replacement");
    let mut guard = acquire(&mut media).unwrap();
    let mut replacement = None;
    guard
        .persist_using(RuntimeMediaSlot::Vars, b"saved", |staged, destination| {
            fs::rename(staged, destination)?;
            fs::write(staged, b"replacement")?;
            replacement = Some(staged.to_path_buf());
            Ok(())
        })
        .unwrap();
    assert_eq!(fs::read(replacement.unwrap()).unwrap(), b"replacement");
}

#[test]
fn uppercase_destination_is_not_runtime_staging() {
    const CHILD: &str = "BRIDGEVM_TEST_RUNTIME_STAGING_CASE_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let status = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "snapshot_pair::managed::runtime::tests::uppercase_destination_is_not_runtime_staging", "--nocapture"])
            .env(CHILD, "1").status().unwrap();
        assert!(status.success());
        return;
    }
    let s = Scratch::new("runtime-staging-case");
    let name = format!(".bridgevm-write-{}-0.tmp", std::process::id());
    let lower = s.path(&name);
    let output = s.path(&name.to_ascii_uppercase());
    fs::write(&lower, b"case-probe").unwrap();
    let case_aliases = output.exists();
    fs::remove_file(lower).unwrap();
    let mut media = VirtBootMediaConfig::qemu_defaults();
    media.flash_vars = WritableMedia::new(s.path("unused-vars")).with_snapshot_path(Some(&output));
    let mut guard = acquire(&mut media).unwrap();
    guard
        .persist(RuntimeMediaSlot::Vars, b"saved-vars")
        .unwrap();
    assert!(
        output.exists(),
        "reported success lost output; case_aliases={case_aliases}"
    );
    assert_eq!(fs::read(output).unwrap(), b"saved-vars");
}

mod overlap {
    use super::*;
    use std::os::unix::fs::symlink;

    fn fixture(tag: &str) -> (Scratch, VirtBootMediaConfig) {
        let scratch = Scratch::new(tag);
        let mut media = VirtBootMediaConfig::qemu_defaults();
        media.flash_vars = WritableMedia::new(scratch.write("vars", b"vars"));
        media.nvme_disk = Some(WritableMedia::new(scratch.write("disk", b"valuable disk")));
        (scratch, media)
    }
    #[test]
    fn vars_snapshot_cannot_replace_disk_or_alias() {
        for alias in ["direct", "symlink", "hardlink"] {
            let (scratch, mut media) = fixture(alias);
            let disk = media.nvme_disk.as_ref().unwrap().path.clone();
            let output = if alias == "direct" {
                disk.clone()
            } else {
                let output = scratch.path("output");
                if alias == "symlink" {
                    symlink(&disk, &output).unwrap()
                } else {
                    fs::hard_link(&disk, &output).unwrap()
                }
                output
            };
            media.flash_vars.snapshot_path = Some(output);
            assert!(acquire(&mut media).is_err());
            assert_eq!(fs::read(disk).unwrap(), b"valuable disk");
        }
    }
    #[test]
    fn absent_shared_output_is_refused_before_creation() {
        let (scratch, mut media) = fixture("absent-output");
        let output = scratch.path("missing/output");
        media.flash_vars.snapshot_path = Some(output.clone());
        media.nvme_disk.as_mut().unwrap().snapshot_path = Some(output.clone());
        assert!(acquire(&mut media).is_err());
        assert!(!output.exists());
    }
    #[test]
    fn managed_selection_still_protects_logical_originals() {
        let (scratch, mut media) = fixture("managed-original");
        let disk = media.nvme_disk.as_ref().unwrap().path.clone();
        let vars = media.flash_vars.path.clone();
        let snapshot = scratch.path("snapshot");
        create_snapshot(&disk, &vars, &snapshot, "vm").unwrap();
        LockedPair::open(&disk, &vars)
            .unwrap()
            .restore(&snapshot)
            .unwrap();
        media.flash_vars.snapshot_path = Some(disk.clone());
        assert!(acquire(&mut media).is_err());
        assert_eq!(fs::read(&disk).unwrap(), b"valuable disk");
        assert_eq!(
            fs::read(LockedPair::open(&disk, &vars).unwrap().paths().unwrap().0).unwrap(),
            b"valuable disk"
        );
    }
    #[test]
    fn late_symlink_retarget_is_refused_before_any_write() {
        let (scratch, mut media) = fixture("late-overlap");
        let disk = media.nvme_disk.as_ref().unwrap().path.clone();
        let output = scratch.path("output");
        media.flash_vars.snapshot_path = Some(output.clone());
        let mut owner = acquire(&mut media).unwrap();
        symlink(&disk, &output).unwrap();
        assert!(owner.persist(RuntimeMediaSlot::Vars, b"new vars").is_err());
        assert_eq!(fs::read(disk).unwrap(), b"valuable disk");
        assert!(fs::symlink_metadata(output).unwrap().is_symlink());
    }
    #[test]
    fn same_slot_snapshot_and_writeback_remain_supported() {
        let (_scratch, mut media) = fixture("same-slot");
        media.flash_vars.snapshot_path = Some(media.flash_vars.path.clone());
        media.flash_vars.write_back = true;
        let vars = media.flash_vars.path.clone();
        let mut owner = acquire(&mut media).unwrap();
        assert_eq!(
            owner
                .persist(RuntimeMediaSlot::Vars, b"new vars")
                .unwrap()
                .len(),
            2
        );
        assert_eq!(fs::read(vars).unwrap(), b"new vars");
    }
    #[test]
    fn absent_outputs_through_parent_alias_are_refused() {
        let (scratch, mut media) = fixture("parent-alias-output");
        let directory = scratch.path("output-dir");
        fs::create_dir(&directory).unwrap();
        let alias = scratch.path("output-alias");
        symlink(&directory, &alias).unwrap();
        media.flash_vars.snapshot_path = Some(directory.join("output"));
        media.nvme_disk.as_mut().unwrap().snapshot_path = Some(alias.join("output"));
        assert!(acquire(&mut media).is_err());
        assert!(!directory.join("output").exists());
    }
    #[cfg(target_os = "macos")]
    #[test]
    fn absent_case_alias_outputs_are_refused_on_case_insensitive_volume() {
        let (scratch, mut media) = fixture("case-alias-output");
        let probe = scratch.write("probe", b"probe");
        if !scratch.path("PROBE").exists() {
            return;
        }
        fs::remove_file(probe).unwrap();
        media.flash_vars.snapshot_path = Some(scratch.path("output"));
        media.nvme_disk.as_mut().unwrap().snapshot_path = Some(scratch.path("OUTPUT"));
        assert!(acquire(&mut media).is_err());
        assert!(!scratch.path("output").exists());
    }
    #[cfg(target_os = "macos")]
    #[test]
    fn absent_unicode_alias_outputs_are_refused() {
        for (left, right) in [("output-é", "output-e\u{301}"), ("output-É", "output-é")] {
            let (scratch, mut media) = fixture("unicode-alias-output");
            let probe = scratch.write(left, b"probe");
            if !scratch.path(right).exists() {
                fs::remove_file(probe).unwrap();
                continue;
            }
            fs::remove_file(probe).unwrap();
            media.flash_vars.snapshot_path = Some(scratch.path(left));
            media.nvme_disk.as_mut().unwrap().snapshot_path = Some(scratch.path(right));
            assert!(acquire(&mut media).is_err(), "{left:?} aliases {right:?}");
            assert!(!scratch.path(left).exists());
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn unicode_comparison_preserves_case_sensitive_distinctions() {
        use std::ffi::OsStr;
        let equal = |left, right, insensitive| {
            unicode::equivalent(OsStr::new(left), OsStr::new(right), insensitive).unwrap()
        };
        assert!(equal("é", "e\u{301}", false));
        assert!(!equal("É", "é", false));
        assert!(equal("É", "é", true));
        assert!(!equal("é", "e", true));
    }

    #[test]
    fn distinct_outputs_below_absent_parent_remain_supported() {
        let (scratch, mut media) = fixture("different-absent-output");
        media.flash_vars.snapshot_path = Some(scratch.path("missing/vars"));
        media.nvme_disk.as_mut().unwrap().snapshot_path = Some(scratch.path("missing/disk"));
        assert!(acquire(&mut media).is_ok());
        assert!(!scratch.path("missing").exists());
    }
}

mod export {
    use super::*;

    fn fixture(tag: &str) -> (Scratch, VirtBootMediaConfig, RuntimeLease) {
        let s = Scratch::new(tag);
        let mut media = VirtBootMediaConfig::qemu_defaults();
        media.flash_vars = WritableMedia::new(s.write("vars", b"vars")).with_write_back(true);
        media.nvme_disk = Some(
            WritableMedia::new(s.write("disk", b"disk"))
                .with_snapshot_path(Some(s.write("output", b"old-output"))),
        );
        let owner = acquire(&mut media).unwrap();
        (s, media, owner)
    }

    fn owned(path: &Path) {
        assert_eq!(
            MediaLease::acquire([path]).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
    }

    #[test]
    fn output_contention_and_absent_policy_do_not_invoke_writer() {
        let (s, _, mut owner) = fixture("stream-contention");
        let output = s.path("output");
        let competitor = MediaLease::acquire([output.as_path()]).unwrap();
        let error = owner
            .export_snapshot(RuntimeMediaSlot::Primary, |_| panic!("must refuse first"))
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
        assert_eq!(fs::read(output).unwrap(), b"old-output");
        assert!(owner
            .export_snapshot(RuntimeMediaSlot::Vars, |_| panic!("no snapshot"))
            .unwrap()
            .is_none());
        assert_eq!(
            owner
                .export_snapshot(RuntimeMediaSlot::Target, |_| panic!("absent slot"))
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
        drop(competitor);
    }

    #[test]
    fn writer_failure_preserves_output_and_removes_own_staging() {
        let (s, _, mut owner) = fixture("stream-writer-error");
        let before = fs::read_dir(&s.0).unwrap().count();
        let error = owner
            .export_snapshot(RuntimeMediaSlot::Primary, |out| {
                out.write_all(b"partial")?;
                Err(io::Error::other("injected writer failure"))
            })
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::Other);
        assert_eq!(fs::read(s.path("output")).unwrap(), b"old-output");
        assert_eq!(fs::read_dir(&s.0).unwrap().count(), before);
        owned(&s.path("output"));
        owner
            .export_snapshot(RuntimeMediaSlot::Primary, |out| {
                out.write_all(b"retry")?;
                Ok(5)
            })
            .unwrap();
        assert_eq!(fs::read(s.path("output")).unwrap(), b"retry");
    }

    #[test]
    fn streaming_publication_errors_keep_uncertain_image_owned() {
        for after_rename in [false, true] {
            let (s, _, mut owner) = fixture("stream-publish-error");
            let output = s.path("output");
            let alias = s.path("new-alias");
            let result = owner.export_snapshot_using(
                RuntimeMediaSlot::Primary,
                |out| {
                    out.write_all(b"next-image")?;
                    Ok(10)
                },
                |staged, destination| {
                    fs::hard_link(staged, &alias)?;
                    owned(staged);
                    owned(destination);
                    owned(&alias);
                    if after_rename {
                        fs::rename(staged, destination)?;
                    }
                    Err(io::Error::other("injected publication sync error"))
                },
            );
            assert!(result.is_err());
            assert_eq!(
                fs::read(&output).unwrap(),
                if after_rename {
                    b"next-image".as_slice()
                } else {
                    b"old-output"
                }
            );
            owned(&alias);
            owned(&output);
            owner
                .persist(RuntimeMediaSlot::Vars, b"saved-vars")
                .unwrap();
            owned(&output);
            if after_rename {
                owned(&alias);
            }
            owner
                .export_snapshot(RuntimeMediaSlot::Primary, |out| {
                    out.write_all(b"retry")?;
                    Ok(5)
                })
                .unwrap();
            MediaLease::acquire([alias.as_path()]).unwrap();
            owned(&output);
            drop(owner);
            MediaLease::acquire([output.as_path()]).unwrap();
        }
    }
    #[test]
    fn self_export_keeps_original_source_without_pinning_replaced_generations() {
        let (s, media, _) = fixture("stream-source-lifetime");
        let mut media = media;
        let input = media.nvme_disk.as_ref().unwrap().path.clone();
        media.nvme_disk.as_mut().unwrap().snapshot_path = Some(input.clone());
        let original = s.path("original-alias");
        fs::hard_link(&input, &original).unwrap();
        let mut source = fs::File::open(&input).unwrap();
        let mut owner = acquire(&mut media).unwrap();
        let mut old_output: Option<PathBuf> = None;
        for generation in 0..8 {
            use std::io::{Seek, SeekFrom};
            source.seek(SeekFrom::Start(0)).unwrap();
            owner
                .export_snapshot(RuntimeMediaSlot::Primary, |out| io::copy(&mut source, out))
                .unwrap();
            owned(&original);
            owned(&input);
            if let Some(old) = old_output.take() {
                MediaLease::acquire([old.as_path()]).unwrap();
            }
            let alias = s.path(&format!("export-{generation}"));
            fs::hard_link(&input, &alias).unwrap();
            owned(&alias);
            owner
                .persist(RuntimeMediaSlot::Vars, b"updated-vars")
                .unwrap();
            owned(&original);
            owned(&alias);
            old_output = Some(alias);
        }
        drop(owner);
        MediaLease::acquire([original.as_path(), input.as_path()]).unwrap();
    }
}
