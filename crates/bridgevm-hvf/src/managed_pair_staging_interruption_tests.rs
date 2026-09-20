use super::*;
use crate::snapshot_pair::{copy_and_sync, create_snapshot, snapshot_pair_tests::Scratch};
use std::process::Command;

const SCENARIO_ENV: &str = "BRIDGEVM_TEST_MANAGED_PAIR_STAGING_CRASH";
const DISK_ENV: &str = "BRIDGEVM_TEST_MANAGED_PAIR_STAGING_DISK";
const VARS_ENV: &str = "BRIDGEVM_TEST_MANAGED_PAIR_STAGING_VARS";
const SNAPSHOT_ENV: &str = "BRIDGEVM_TEST_MANAGED_PAIR_STAGING_SNAPSHOT";

fn required_path(name: &str) -> PathBuf {
    std::env::var_os(name)
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("missing subprocess path {name}"))
}

#[test]
fn restore_partial_staging_crash_child() {
    let Ok(scenario) = std::env::var(SCENARIO_ENV) else {
        return;
    };
    let disk = required_path(DISK_ENV);
    let vars = required_path(VARS_ENV);
    let snapshot = required_path(SNAPSHOT_ENV);
    let pair = LockedPair::open(&disk, &vars).expect("child owns pair");
    layout::initialize(&pair.root).expect("initialize managed root");
    let staged = pair.root.join("staging");
    private_directory(&staged, true).expect("create private staging directory");
    copy_and_sync(&snapshot.join("disk.raw"), &staged.join("disk.raw")).expect("stage disk");
    if scenario == "disk-only" {
        std::process::exit(73);
    }
    copy_and_sync(&snapshot.join("vars.fd"), &staged.join("vars.fd")).expect("stage vars");
    if scenario == "disk-and-vars" {
        std::process::exit(74);
    }
    copy_and_sync(
        &snapshot.join("manifest.json"),
        &staged.join("manifest.json"),
    )
    .expect("stage manifest");
    if scenario == "complete-before-directory-sync" {
        std::process::exit(75);
    }
    panic!("unknown crash scenario {scenario}");
}

fn crash_restore(scenario: &str, expected_exit: i32, disk: &Path, vars: &Path, snapshot: &Path) {
    let status = Command::new(std::env::current_exe().expect("current test executable"))
        .arg(
            "snapshot_pair::managed::tests::staging_interruption_tests::restore_partial_staging_crash_child",
        )
        .arg("--exact")
        .arg("--nocapture")
        .env(SCENARIO_ENV, scenario)
        .env(DISK_ENV, disk)
        .env(VARS_ENV, vars)
        .env(SNAPSHOT_ENV, snapshot)
        .status()
        .expect("launch crash subprocess");
    assert_eq!(status.code(), Some(expected_exit), "child status {status}");
}

fn fixture(tag: &str) -> (Scratch, PathBuf, PathBuf, PathBuf) {
    let scratch = Scratch::new(tag);
    let disk = scratch.write("disk", b"old-disk");
    let vars = scratch.write("vars", b"old-vars");
    let source_disk = scratch.write("source-disk", b"new-disk");
    let source_vars = scratch.write("source-vars", b"new-vars");
    let snapshot = scratch.path("snapshot");
    create_snapshot(
        &source_disk,
        &source_vars,
        &snapshot,
        "partial-staging-restore",
        false,
        1024,
    )
    .expect("create source snapshot");
    (scratch, disk, vars, snapshot)
}

fn contents(pair: &LockedPair) -> (Vec<u8>, Vec<u8>) {
    let (disk, vars) = pair.paths().expect("resolve selected pair");
    (
        fs::read(disk).expect("read disk"),
        fs::read(vars).expect("read vars"),
    )
}

#[test]
fn hard_exit_during_partial_staging_preserves_old_pair_and_allows_retry() {
    for (scenario, exit) in [
        ("disk-only", 73),
        ("disk-and-vars", 74),
        ("complete-before-directory-sync", 75),
    ] {
        let (_scratch, disk, vars, snapshot) = fixture(scenario);
        crash_restore(scenario, exit, &disk, &vars, &snapshot);
        let mut pair = LockedPair::open(&disk, &vars).expect("reopen partially staged pair");
        assert_eq!(
            contents(&pair),
            (b"old-disk".to_vec(), b"old-vars".to_vec()),
            "partial staging must never become selected for {scenario}"
        );
        pair.restore(&snapshot).expect("retry restore after crash");
        assert_eq!(
            contents(&pair),
            (b"new-disk".to_vec(), b"new-vars".to_vec()),
            "retry must publish one complete pair for {scenario}"
        );
    }
}
