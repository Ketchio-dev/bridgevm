//! A crashed restore's owned copies must be reclaimed before measuring space.

use super::*;
use crate::snapshot_pair::{snapshot_publish, verify_snapshot};

fn constrained_crash_retry(selected: bool) {
    let tag = format!("managed-capacity-crash-{selected}");
    let (_scratch, disk, vars, snapshot) = fixture(&tag);
    if selected {
        let mut pair = LockedPair::open(&disk, &vars).unwrap();
        pair.restore(&snapshot).unwrap();
        let (disk, vars) = pair.paths().unwrap();
        fs::write(disk, b"guest-disk").unwrap();
        fs::write(vars, b"guest-vars").unwrap();
    }
    crash_restore("before-publish", BEFORE_EXIT, &disk, &vars, &snapshot);
    let mut pair = LockedPair::open(&disk, &vars).unwrap();
    let old_paths = pair.paths().unwrap();
    let old = contents(&pair);
    let manifest = verify_snapshot(&snapshot).unwrap();
    let needed = manifest.disk_bytes + manifest.vars_bytes;
    let staging = pair.root.join("staging");
    let stale_bytes = || {
        ["disk.raw", "vars.fd"]
            .iter()
            .filter_map(|name| fs::metadata(staging.join(name)).ok())
            .map(|metadata| metadata.len())
            .sum::<u64>()
    };
    assert_eq!(stale_bytes(), needed, "child must leave the complete pair");
    assert_eq!(fs::read(&disk).unwrap(), b"old-disk");
    assert_eq!(fs::read(&vars).unwrap(), b"old-vars");
    let result = pair.restore_with_capacity(
        &snapshot,
        |_| {
            let stale = stale_bytes();
            let available = needed - stale;
            eprintln!("selected={selected} needed={needed} stale={stale} available={available}");
            assert_eq!(pair_paths(&old_paths), old, "admission changed selection");
            Some(available)
        },
        snapshot_publish::publish,
    );
    eprintln!("selected={selected} retry={result:?}");
    result.expect("reclaiming owned crash debris makes this retry fit exactly");
    assert_eq!(
        contents(&pair),
        (b"new-disk".to_vec(), b"new-vars".to_vec())
    );
    assert_eq!(fs::read(&disk).unwrap(), b"old-disk");
    assert_eq!(fs::read(&vars).unwrap(), b"old-vars");
    assert!(!staging.exists());
}

fn pair_paths(paths: &(PathBuf, PathBuf)) -> (Vec<u8>, Vec<u8>) {
    (fs::read(&paths.0).unwrap(), fs::read(&paths.1).unwrap())
}

#[test]
fn initial_selection_retry_reclaims_crashed_staging_before_capacity_admission() {
    constrained_crash_retry(false);
}

#[test]
fn current_selection_retry_reclaims_crashed_staging_before_capacity_admission() {
    constrained_crash_retry(true);
}

#[path = "managed_pair_retry_capacity_refusal_tests.rs"]
mod refusal_tests;
