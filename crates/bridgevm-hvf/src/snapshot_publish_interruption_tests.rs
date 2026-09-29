use super::*;
use crate::snapshot_pair::managed::LockedPair;
use crate::snapshot_pair::{create_snapshot, free_space, sync_dir};
use std::path::PathBuf;
use std::process::Command;

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
    pair.restore_with_capacity(&snapshot, free_space::available_bytes, interrupted)
        .expect("the crash boundary must exit before restore returns");
}

fn crash_restore(scenario: &str, exit: i32, disk: &Path, vars: &Path, snapshot: &Path) {
    let status = Command::new(std::env::current_exe().expect("current test executable"))
        .arg("snapshot_pair::snapshot_publish::tests::interruption_tests::swap_crash_child")
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
    create_snapshot(&source_disk, &source_vars, &snapshot, name, false, 1024)
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

        pair.restore(&g2)
            .unwrap_or_else(|error| panic!("retry restore after {scenario}: {error}"));
        assert_eq!(contents(&pair), owned(G2), "retry after {scenario}");
        assert!(!root.join("staging").exists(), "debris after {scenario}");
        drop(pair);
        assert_eq!(fs::read(&disk).unwrap(), b"old-disk");
        assert_eq!(fs::read(&vars).unwrap(), b"old-vars");
        let reopened = LockedPair::open(&disk, &vars).expect("reopen after retry");
        assert_eq!(contents(&reopened), owned(G2));
    }
}
