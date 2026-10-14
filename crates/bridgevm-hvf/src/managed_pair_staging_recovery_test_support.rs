//! Tests model the user's explicit resolution of ambiguous complete staging.

use super::super::LockedPair;
use super::staging_interruption_tests::{DISK_ENV, SCENARIO_ENV, SNAPSHOT_ENV, VARS_ENV};
use std::{fs, path::Path, process::Command};

pub(super) fn retry_after_explicit_staging_resolution(pair: &mut LockedPair, snapshot: &Path) {
    let staged = pair.root.join("staging");
    if staged.join("manifest.json").exists() {
        let read = |root: &Path| {
            ["disk.raw", "vars.fd", "manifest.json"].map(|name| fs::read(root.join(name)).unwrap())
        };
        let before = read(&staged);
        let selected = pair.paths().unwrap();
        let refusal = pair
            .restore_with_capacity(
                snapshot,
                |_| panic!("ambiguous staging must refuse before capacity admission"),
                |_, _| panic!("ambiguous staging must refuse before publication"),
            )
            .unwrap_err();
        assert!(refusal.to_string().contains("it was left intact"));
        assert_eq!(read(&staged), before);
        assert_eq!(pair.paths().unwrap(), selected);
        let preserved = pair
            .disk
            .parent()
            .unwrap()
            .join("preserved-complete-staging");
        fs::rename(&staged, &preserved)
            .expect("simulate explicit user move outside managed storage");
        assert_eq!(read(&preserved), before);
    }
    pair.restore(snapshot)
        .expect("retry after explicit staging resolution");
}

pub(super) fn crash_incomplete_restore(disk: &Path, vars: &Path, snapshot: &Path) {
    crash_restore("disk-and-vars", 74, disk, vars, snapshot);
}

pub(super) fn crash_restore(
    scenario: &str,
    expected_exit: i32,
    disk: &Path,
    vars: &Path,
    snapshot: &Path,
) {
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
