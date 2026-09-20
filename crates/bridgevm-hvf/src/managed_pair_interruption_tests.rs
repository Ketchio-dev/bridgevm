use super::super::*;
use crate::snapshot_pair::{create_snapshot, snapshot_pair_tests::Scratch};
use std::process::Command;

const SCENARIO_ENV: &str = "BRIDGEVM_TEST_MANAGED_PAIR_CRASH";
const DISK_ENV: &str = "BRIDGEVM_TEST_MANAGED_PAIR_DISK";
const VARS_ENV: &str = "BRIDGEVM_TEST_MANAGED_PAIR_VARS";
const SNAPSHOT_ENV: &str = "BRIDGEVM_TEST_MANAGED_PAIR_SNAPSHOT";
const BEFORE_EXIT: i32 = 71;
const AFTER_EXIT: i32 = 72;

fn required_path(name: &str) -> PathBuf {
    std::env::var_os(name)
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("missing subprocess path {name}"))
}

/// Subprocess entry point. A normal test run leaves this inert; the parent
/// below selects it alone and supplies one exact crash boundary.
#[test]
fn restore_crash_child() {
    let Ok(scenario) = std::env::var(SCENARIO_ENV) else {
        return;
    };
    let disk = required_path(DISK_ENV);
    let vars = required_path(VARS_ENV);
    let snapshot = required_path(SNAPSHOT_ENV);
    let mut pair = LockedPair::open(&disk, &vars).expect("child owns pair");
    pair.restore_using(&snapshot, |staged, current| match scenario.as_str() {
        "before-publish" => std::process::exit(BEFORE_EXIT),
        "after-publish" => {
            crate::snapshot_pair::snapshot_publish::publish(staged, current)
                .expect("publish restored pair");
            std::process::exit(AFTER_EXIT)
        }
        other => panic!("unknown crash scenario {other}"),
    })
    .expect("the crash boundary must exit before restore returns");
}

fn crash_restore(scenario: &str, expected_exit: i32, disk: &Path, vars: &Path, snapshot: &Path) {
    let status = Command::new(std::env::current_exe().expect("current test executable"))
        .arg("snapshot_pair::managed::tests::interruption_tests::restore_crash_child")
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
        "interrupted-restore",
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
fn hard_exit_around_publication_reopens_only_complete_generations() {
    let (_scratch, disk, vars, snapshot) = fixture("managed-crash-before");
    crash_restore("before-publish", BEFORE_EXIT, &disk, &vars, &snapshot);
    let mut pair = LockedPair::open(&disk, &vars).expect("reopen before-publish pair");
    assert_eq!(
        contents(&pair),
        (b"old-disk".to_vec(), b"old-vars".to_vec())
    );
    pair.restore(&snapshot).expect("recover from staged debris");
    assert_eq!(
        contents(&pair),
        (b"new-disk".to_vec(), b"new-vars".to_vec())
    );
    drop(pair);

    let (_scratch, disk, vars, snapshot) = fixture("managed-crash-after");
    crash_restore("after-publish", AFTER_EXIT, &disk, &vars, &snapshot);
    let mut pair = LockedPair::open(&disk, &vars).expect("reopen after-publish pair");
    assert_eq!(
        contents(&pair),
        (b"new-disk".to_vec(), b"new-vars".to_vec())
    );
    pair.restore(&snapshot)
        .expect("restore remains usable after unacknowledged publication");
    assert_eq!(
        contents(&pair),
        (b"new-disk".to_vec(), b"new-vars".to_vec())
    );
}
