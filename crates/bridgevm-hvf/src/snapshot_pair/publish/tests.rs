use super::*;
use crate::snapshot_pair::tests::{retry_create, Scratch};
use crate::snapshot_pair::{
    create_snapshot, staging_path, sync_dir, verify_snapshot, SnapshotManifest,
};
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;
use std::process::Command;

fn pair(path: &Path, value: &[u8]) {
    fs::create_dir(path).unwrap();
    fs::write(path.join("disk.raw"), value).unwrap();
    fs::write(path.join("vars.fd"), value).unwrap();
}

fn assert_pair(path: &Path, value: &[u8]) {
    assert_eq!(fs::read(path.join("disk.raw")).unwrap(), value);
    assert_eq!(fs::read(path.join("vars.fd")).unwrap(), value);
}

#[test]
fn publishes_a_new_complete_directory() {
    let s = Scratch::new("publish-new");
    pair(&s.path("stage"), b"new");
    publish(&s.path("stage"), &s.path("dest")).unwrap();
    assert_pair(&s.path("dest"), b"new");
    assert!(!s.path("stage").exists());
}

#[test]
fn replaces_an_existing_complete_directory() {
    let s = Scratch::new("publish-existing");
    pair(&s.path("stage"), b"new");
    pair(&s.path("dest"), b"old");
    publish(&s.path("stage"), &s.path("dest")).unwrap();
    assert_pair(&s.path("dest"), b"new");
    assert!(!s.path("stage").exists());
}

#[test]
fn failed_exchange_never_deletes_the_old_snapshot() {
    let s = Scratch::new("publish-refused");
    pair(&s.path("stage"), b"new");
    pair(&s.path("dest"), b"old");
    let result = publish_with(&s.path("stage"), &s.path("dest"), |_, _| {
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "injected exchange refusal",
        ))
    });
    assert!(result.is_err());
    assert_pair(&s.path("dest"), b"old");
    assert_pair(&s.path("stage"), b"new");
}

#[test]
fn failure_after_exchange_retains_two_complete_versions() {
    let s = Scratch::new("publish-post-exchange");
    pair(&s.path("stage"), b"new");
    pair(&s.path("dest"), b"old");
    let result = publish_with(&s.path("stage"), &s.path("dest"), |left, right| {
        exchange(left, right)?;
        Err(io::Error::other("injected failure before directory sync"))
    });
    assert!(result.is_err());
    assert_pair(&s.path("dest"), b"new");
    assert_pair(&s.path("stage"), b"old");
}

#[test]
fn refuses_a_file_destination_without_mutating_it() {
    let s = Scratch::new("publish-file");
    pair(&s.path("stage"), b"new");
    s.write("dest", b"unrelated");
    assert!(publish(&s.path("stage"), &s.path("dest")).is_err());
    assert_eq!(fs::read(s.path("dest")).unwrap(), b"unrelated");
    assert_pair(&s.path("stage"), b"new");
}

#[test]
fn refuses_a_symlink_destination_without_mutating_its_target() {
    let s = Scratch::new("publish-symlink");
    pair(&s.path("stage"), b"new");
    pair(&s.path("target"), b"old");
    std::os::unix::fs::symlink(s.path("target"), s.path("dest")).unwrap();
    assert!(publish(&s.path("stage"), &s.path("dest")).is_err());
    assert_pair(&s.path("target"), b"old");
    assert_pair(&s.path("stage"), b"new");
}

// Empty destinations need atomic rename, not directory exchange support.

fn snapshot(scratch: &Scratch, name: &str, bytes: &[u8]) -> SnapshotManifest {
    let disk = scratch.write(&format!("{name}-disk"), bytes);
    let vars = scratch.write(&format!("{name}-vars"), bytes);
    create_snapshot(&disk, &vars, &scratch.path(name), name).unwrap()
}

#[test]
fn empty_destination_does_not_require_exchange_support() {
    let scratch = Scratch::new("publish-empty-no-exchange");
    let stage = scratch.path("stage");
    let dest = scratch.path("dest");
    let new = snapshot(&scratch, "stage", b"new-pair");
    fs::create_dir(&dest).unwrap();
    let mut exchanged = false;
    let result = publish_with(&stage, &dest, |_, _| {
        exchanged = true;
        Err(io::Error::from_raw_os_error(libc::ENOTSUP))
    });
    result.expect("an admitted empty destination must not require exchange support");
    assert!(!exchanged);
    assert_eq!(verify_snapshot(&dest).unwrap(), new);
    assert!(!stage.exists());
}

#[test]
fn complete_destination_still_requires_exchange_support() {
    let scratch = Scratch::new("publish-complete-no-exchange");
    let old = snapshot(&scratch, "dest", b"old-pair");
    let new = snapshot(&scratch, "stage", b"new-pair");
    let stage = scratch.path("stage");
    let dest = scratch.path("dest");
    let result = publish_with(&stage, &dest, |_, _| {
        Err(io::Error::from_raw_os_error(libc::ENOTSUP))
    });
    assert_eq!(result.unwrap_err().kind(), io::ErrorKind::Unsupported);
    assert_eq!(verify_snapshot(&dest).unwrap(), old);
    assert_eq!(verify_snapshot(&stage).unwrap(), new);
}

// Process exit around a real empty-directory rename, not a power-loss test.

const CASE_ENV: &str = "BRIDGEVM_TEST_EMPTY_PUBLISH_CRASH";
const ROOT_ENV: &str = "BRIDGEVM_TEST_EMPTY_PUBLISH_ROOT";
const CASES: [(&str, i32, bool); 2] = [("before-rename", 96, true), ("after-rename", 97, false)];

#[test]
fn empty_rename_crash_child() {
    let Ok(case) = std::env::var(CASE_ENV) else {
        return;
    };
    let exit = CASES.iter().find(|(name, ..)| *name == case).unwrap().1;
    let root = PathBuf::from(std::env::var_os(ROOT_ENV).expect("owned fixture root"));
    let dest = root.join("dest");
    publish_using(
        &staging_path(&dest),
        &dest,
        |_, _| panic!("unexpected exchange"),
        |a, b| {
            if case == "before-rename" {
                std::process::exit(exit);
            }
            fs::rename(a, b)?;
            std::process::exit(exit)
        },
        sync_dir,
    )
    .unwrap();
    panic!("selected process-exit boundary must not return");
}

#[test]
fn hard_exit_before_and_after_empty_rename_preserves_pair_and_retry() {
    for (case, exit, finished) in CASES {
        let scratch = Scratch::new(&format!("publish-empty-crash-{case}"));
        let disk = scratch.write("source-disk", b"new-disk");
        let vars = scratch.write("source-vars", b"new-vars");
        let dest = scratch.path("dest");
        let stage = staging_path(&dest);
        fs::create_dir(&dest).unwrap();
        let new = create_snapshot(&disk, &vars, &stage, "new").unwrap();
        let status = Command::new(std::env::current_exe().unwrap())
            .arg("snapshot_pair::publish::tests::empty_rename_crash_child")
            .args(["--exact", "--nocapture"])
            .env(CASE_ENV, case)
            .env(ROOT_ENV, &scratch.0)
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(exit));
        if case == "before-rename" {
            assert_eq!(fs::read_dir(&dest).unwrap().count(), 0);
            assert_eq!(verify_snapshot(&stage).unwrap(), new);
        } else {
            assert_eq!(verify_snapshot(&dest).unwrap(), new);
            assert!(!stage.exists());
        }
        let retry = retry_create(&disk, &vars, &dest, finished);
        assert_eq!(verify_snapshot(&dest).unwrap(), retry);
        assert!(!stage.exists());
        assert_eq!(fs::read(&disk).unwrap(), b"new-disk");
        assert_eq!(fs::read(&vars).unwrap(), b"new-vars");
    }
}

// Real rename refuses a late entry; publication never falls back to exchange.

fn unexpected_exchange(_: &Path, _: &Path) -> io::Result<()> {
    panic!("the inspected empty destination must remain on the rename branch")
}

#[test]
fn a_sentinel_arriving_after_the_empty_check_is_preserved() {
    let scratch = Scratch::new("publish-empty-sentinel");
    let new = snapshot(&scratch, "stage", b"new-pair");
    let stage = scratch.path("stage");
    let dest = scratch.path("dest");
    fs::create_dir(&dest).unwrap();
    let inode = fs::symlink_metadata(&dest).unwrap().ino();
    let result = publish_using(
        &stage,
        &dest,
        unexpected_exchange,
        |a, b| {
            fs::write(b.join("keep.txt"), b"caller sentinel")?;
            fs::rename(a, b)
        },
        sync_dir,
    );
    let error = result.unwrap_err();
    assert!(matches!(
        error.raw_os_error(),
        Some(libc::ENOTEMPTY) | Some(libc::EEXIST)
    ));
    assert_eq!(fs::symlink_metadata(&dest).unwrap().ino(), inode);
    assert_eq!(fs::read(dest.join("keep.txt")).unwrap(), b"caller sentinel");
    assert_eq!(fs::read_dir(&dest).unwrap().count(), 1);
    assert_eq!(verify_snapshot(&stage).unwrap(), new);
}

#[test]
fn rename_publication_does_not_delete_a_recreated_staging_path() {
    for empty in [true, false] {
        let scratch = Scratch::new(&format!("publish-recreated-stage-empty-{empty}"));
        let new = snapshot(&scratch, "stage", b"new-pair");
        let stage = scratch.path("stage");
        let dest = scratch.path("dest");
        if empty {
            fs::create_dir(&dest).unwrap();
        }
        publish_using(
            &stage,
            &dest,
            unexpected_exchange,
            |a, b| {
                fs::rename(a, b)?;
                fs::create_dir(a)?;
                fs::write(a.join("keep.txt"), b"new staging owner")
            },
            sync_dir,
        )
        .unwrap();
        assert_eq!(verify_snapshot(&dest).unwrap(), new);
        assert_eq!(
            fs::read(stage.join("keep.txt")).unwrap(),
            b"new staging owner"
        );
    }
}

#[test]
fn failed_parent_sync_after_empty_rename_retains_the_published_pair() {
    let scratch = Scratch::new("publish-empty-sync-failure");
    let new = snapshot(&scratch, "stage", b"new-pair");
    let stage = scratch.path("stage");
    let dest = scratch.path("dest");
    fs::create_dir(&dest).unwrap();
    let result = publish_using(
        &stage,
        &dest,
        unexpected_exchange,
        |a, b| fs::rename(a, b),
        |_| Err(io::Error::other("injected parent sync failure")),
    );
    assert_eq!(
        result.unwrap_err().to_string(),
        "injected parent sync failure"
    );
    assert_eq!(verify_snapshot(&dest).unwrap(), new);
    assert!(!stage.exists());
}

// The exchange refusal names the volume type the scratch volume reports
// (macOS); elsewhere it says only "a volume".

#[test]
fn enotsup_names_the_operation_and_the_volume_type_and_other_errors_pass_through() {
    let scratch = std::env::temp_dir();
    let volume = if cfg!(target_os = "macos") {
        let name = volume_type(&scratch).expect("the scratch volume reports its type");
        assert!(!name.is_empty() && name.is_ascii(), "{name:?}");
        format!("a volume of type {name}")
    } else {
        assert_eq!(volume_type(&scratch), None);
        "a volume".to_string()
    };
    let refused = refuse_unsupported(io::Error::from_raw_os_error(libc::ENOTSUP), &scratch);
    assert_eq!(refused.kind(), io::ErrorKind::Unsupported);
    let text = refused.to_string();
    let prefix = format!("{} is on {volume} that ", scratch.display());
    assert!(text.starts_with(&prefix), "{text}");
    assert!(text.ends_with("; it was left unchanged"), "{text}");
    for errno in [libc::EXDEV, libc::EIO, libc::ENOENT] {
        let passed = refuse_unsupported(io::Error::from_raw_os_error(errno), &scratch);
        assert_eq!(passed.raw_os_error(), Some(errno));
    }
    assert_eq!(
        volume_type(Path::new("/nonexistent-bridgevm-volume-probe")),
        None
    );
}

mod swap_crash {
    use super::*;
    use crate::snapshot_pair::managed::LockedPair;

    const SCENARIO_ENV: &str = "BRIDGEVM_TEST_PUBLISH_SWAP_CRASH";
    const DISK_ENV: &str = "BRIDGEVM_TEST_PUBLISH_SWAP_DISK";
    const VARS_ENV: &str = "BRIDGEVM_TEST_PUBLISH_SWAP_VARS";
    const SNAPSHOT_ENV: &str = "BRIDGEVM_TEST_PUBLISH_SWAP_SNAPSHOT";
    const SCENARIOS: [(&str, i32); 4] = [
        ("before-swap", 91),
        ("after-swap", 92),
        ("old-generation-synced", 93),
        ("old-generation-partly-removed", 94),
    ];

    type Generation = (&'static [u8], &'static [u8]);
    const G1: Generation = (b"g1-disk", b"g1-vars");
    const G2: Generation = (b"g2-disk", b"g2-vars");

    fn required_path(name: &str) -> PathBuf {
        std::env::var_os(name)
            .map(PathBuf::from)
            .unwrap_or_else(|| panic!("missing subprocess path {name}"))
    }

    /// Subprocess entry point. A normal test run leaves this inert; the parent
    /// selects it alone and names one boundary inside the generation swap.
    #[test]
    fn swap_crash_child() {
        let Ok(scenario) = std::env::var(SCENARIO_ENV) else {
            return;
        };
        let Some(&(_, exit)) = SCENARIOS.iter().find(|(name, _)| *name == scenario) else {
            panic!("unknown crash scenario {scenario}");
        };
        let disk = required_path(DISK_ENV);
        let vars = required_path(VARS_ENV);
        let snapshot = required_path(SNAPSHOT_ENV);
        let interrupted = |staged: &Path, current: &Path| {
            publish_with(staged, current, |left, right| {
                if scenario == "before-swap" {
                    std::process::exit(exit);
                }
                exchange(left, right)?;
                if scenario == "after-swap" {
                    std::process::exit(exit);
                }
                // The remaining boundaries replay publication's tail: the parent
                // sync, then removal of the old generation now at the staging name.
                sync_dir(left.parent().unwrap())?;
                if scenario == "old-generation-synced" {
                    std::process::exit(exit);
                }
                fs::remove_file(left.join("disk.raw"))?;
                fs::remove_file(left.join("manifest.json"))?;
                std::process::exit(exit)
            })
        };
        let mut pair = LockedPair::open(&disk, &vars).expect("child owns pair");
        pair.restore_copying(
            &snapshot,
            crate::snapshot_pair::files::copy_and_sync,
            interrupted,
        )
        .expect("the crash boundary must exit before restore returns");
    }

    fn crash_restore(scenario: &str, exit: i32, disk: &Path, vars: &Path, snapshot: &Path) {
        let status = Command::new(std::env::current_exe().expect("current test executable"))
            .arg("snapshot_pair::publish::tests::swap_crash::swap_crash_child")
            .arg("--exact")
            .arg("--nocapture")
            .env(SCENARIO_ENV, scenario)
            .env(DISK_ENV, disk)
            .env(VARS_ENV, vars)
            .env(SNAPSHOT_ENV, snapshot)
            .status()
            .expect("launch crash subprocess");
        assert_eq!(status.code(), Some(exit), "child status {status}");
    }

    fn owned((disk, vars): Generation) -> (Vec<u8>, Vec<u8>) {
        (disk.to_vec(), vars.to_vec())
    }

    fn contents(pair: &LockedPair) -> (Vec<u8>, Vec<u8>) {
        let (disk, vars) = pair.paths().expect("resolve selected pair");
        (
            fs::read(disk).expect("read disk"),
            fs::read(vars).expect("read vars"),
        )
    }

    /// Selected media live in the root's "current" directory, two levels down.
    fn managed_root(pair: &LockedPair) -> PathBuf {
        let (disk, _) = pair.paths().expect("resolve selected pair");
        disk.ancestors().nth(2).expect("managed root").to_path_buf()
    }

    fn staged(root: &Path) -> [Option<Vec<u8>>; 2] {
        let staging = root.join("staging");
        ["disk.raw", "vars.fd"].map(|name| fs::read(staging.join(name)).ok())
    }

    fn snapshot(scratch: &Scratch, name: &str, (disk, vars): Generation) -> PathBuf {
        let source_disk = scratch.write(&format!("{name}-source-disk"), disk);
        let source_vars = scratch.write(&format!("{name}-source-vars"), vars);
        let snapshot = scratch.path(name);
        create_snapshot(&source_disk, &source_vars, &snapshot, name)
            .expect("create source snapshot");
        snapshot
    }

    /// G1 is selected before the child runs, so publishing G2 takes the swap
    /// branch. Sources are captured first: another pair's managed root beside
    /// them would make their own identity resolution refuse.
    fn fixture(tag: &str) -> (Scratch, PathBuf, PathBuf, PathBuf) {
        let scratch = Scratch::new(tag);
        let disk = scratch.write("disk", b"old-disk");
        let vars = scratch.write("vars", b"old-vars");
        let g1 = snapshot(&scratch, "g1", G1);
        let g2 = snapshot(&scratch, "g2", G2);
        let mut pair = LockedPair::open(&disk, &vars).expect("own pair");
        pair.restore(&g1).expect("select G1");
        assert_eq!(contents(&pair), owned(G1));
        (scratch, disk, vars, g2)
    }

    #[test]
    fn hard_exit_around_generation_swap_selects_one_complete_generation() {
        for (scenario, exit) in SCENARIOS {
            let (_scratch, disk, vars, g2) = fixture(&format!("swap-crash-{scenario}"));
            crash_restore(scenario, exit, &disk, &vars, &g2);

            let mut pair = LockedPair::open(&disk, &vars).expect("fresh owner after crash");
            let (selected, debris): (Generation, [Option<&[u8]>; 2]) = match scenario {
                "before-swap" => (G1, [Some(G2.0), Some(G2.1)]),
                "old-generation-partly-removed" => (G2, [None, Some(G1.1)]),
                _ => (G2, [Some(G1.0), Some(G1.1)]),
            };
            assert_eq!(
                contents(&pair),
                owned(selected),
                "{scenario} must select exactly one complete generation"
            );
            let root = managed_root(&pair);
            assert_eq!(
                staged(&root),
                debris.map(|bytes| bytes.map(<[u8]>::to_vec)),
                "{scenario} must leave the unselected generation at the staging name"
            );

            retry(&mut pair, &g2, &root, scenario == "before-swap");
            assert_eq!(contents(&pair), owned(G2), "retry after {scenario}");
            assert!(!root.join("staging").exists(), "debris after {scenario}");
            drop(pair);
            assert_eq!(fs::read(&disk).unwrap(), b"old-disk");
            assert_eq!(fs::read(&vars).unwrap(), b"old-vars");
            let reopened = LockedPair::open(&disk, &vars).expect("reopen after retry");
            assert_eq!(contents(&reopened), owned(G2));
        }
    }

    /// Simulate explicit operator resolution after an ambiguous completed swap.
    fn retry(pair: &mut LockedPair, snapshot: &Path, root: &Path, sealed: bool) {
        let staged = root.join("staging");
        if !sealed {
            let read = || {
                ["disk.raw", "vars.fd", "manifest.json"]
                    .map(|name| fs::read(staged.join(name)).ok())
            };
            let before = read();
            let selected = pair.paths().unwrap();
            let refused = pair.restore(snapshot).unwrap_err();
            assert!(refused.to_string().contains("it was left intact"));
            assert_eq!(read(), before);
            assert_eq!(pair.paths().unwrap(), selected);
            let preserved = root.with_file_name("preserved-complete-staging");
            fs::rename(&staged, preserved).unwrap();
        }
        pair.restore(snapshot)
            .expect("retry after explicit staging resolution");
    }
}
