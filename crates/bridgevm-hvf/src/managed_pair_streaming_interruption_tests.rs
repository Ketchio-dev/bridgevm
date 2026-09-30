use super::*;
use crate::snapshot_pair::copy::{stream_and_sync, COPY_CHUNK};
use crate::snapshot_pair::{create_snapshot, free_space, snapshot_pair_tests::Scratch};
use std::process::Command;

const SCENARIO_ENV: &str = "BRIDGEVM_TEST_MANAGED_PAIR_STREAM_CRASH";
const DISK_ENV: &str = "BRIDGEVM_TEST_MANAGED_PAIR_STREAM_DISK";
const VARS_ENV: &str = "BRIDGEVM_TEST_MANAGED_PAIR_STREAM_VARS";
const SNAPSHOT_ENV: &str = "BRIDGEVM_TEST_MANAGED_PAIR_STREAM_SNAPSHOT";
const FIRST_CHUNK_EXIT: i32 = 76;

type Pair = (Vec<u8>, Vec<u8>);

fn required_path(name: &str) -> PathBuf {
    std::env::var_os(name)
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("missing subprocess path {name}"))
}

fn refuse_publication(_: &Path, _: &Path) -> io::Result<()> {
    panic!("a partially staged restore reached publication")
}

/// Subprocess entry point. The restore streams, as it does when cloning is
/// refused, and the process exits once the first disk chunk is written.
#[test]
fn restore_streaming_crash_child() {
    let Ok(scenario) = std::env::var(SCENARIO_ENV) else {
        return;
    };
    assert_eq!(scenario, "first-chunk", "unknown crash scenario");
    let disk = required_path(DISK_ENV);
    let vars = required_path(VARS_ENV);
    let snapshot = required_path(SNAPSHOT_ENV);
    let mut pair = LockedPair::open(&disk, &vars).expect("child owns pair");
    let exit_after_first_chunk = |src: &Path, dst: &Path| {
        stream_and_sync(src, dst, |_| std::process::exit(FIRST_CHUNK_EXIT))
    };
    let available = free_space::available_bytes;
    pair.restore_copying(
        &snapshot,
        available,
        exit_after_first_chunk,
        refuse_publication,
    )
    .expect("the crash boundary must exit before restore returns");
}

fn crash_restore(disk: &Path, vars: &Path, snapshot: &Path) {
    let status = Command::new(std::env::current_exe().expect("current test executable"))
        .arg("snapshot_pair::managed::tests::streaming_interruption_tests::restore_streaming_crash_child")
        .arg("--exact")
        .arg("--nocapture")
        .env(SCENARIO_ENV, "first-chunk")
        .env(DISK_ENV, disk)
        .env(VARS_ENV, vars)
        .env(SNAPSHOT_ENV, snapshot)
        .status()
        .expect("launch crash subprocess");
    assert_eq!(
        status.code(),
        Some(FIRST_CHUNK_EXIT),
        "child status {status}"
    );
}

/// The snapshot's disk spans two chunks, so one chunk is a partial copy.
/// With `selected`, an earlier restore has made a managed generation current.
fn fixture(tag: &str, selected: bool) -> (Scratch, PathBuf, PathBuf, PathBuf, Pair, Pair) {
    let scratch = Scratch::new(tag);
    let disk = scratch.write("disk", b"old-disk");
    let vars = scratch.write("vars", b"old-vars");
    let new_disk = vec![7; COPY_CHUNK + 4096];
    let source_disk = scratch.write("source-disk", &new_disk);
    let source_vars = scratch.write("source-vars", b"new-vars");
    let snapshot = scratch.path("snapshot");
    create_snapshot(
        &source_disk,
        &source_vars,
        &snapshot,
        "stream",
        false,
        u64::MAX,
    )
    .expect("create source snapshot");
    let mut old = (b"old-disk".to_vec(), b"old-vars".to_vec());
    if selected {
        let first = scratch.path("first");
        create_snapshot(&disk, &vars, &first, "first", false, 1024).expect("first snapshot");
        let mut pair = LockedPair::open(&disk, &vars).expect("own pair");
        pair.restore(&first).expect("select a managed generation");
        let (selected_disk, selected_vars) = pair.paths().expect("selected pair");
        fs::write(selected_disk, b"guest-disk").expect("guest writes disk");
        fs::write(selected_vars, b"guest-vars").expect("guest writes vars");
        old = (b"guest-disk".to_vec(), b"guest-vars".to_vec());
    }
    let new = (new_disk, b"new-vars".to_vec());
    (scratch, disk, vars, snapshot, old, new)
}

fn contents(pair: &LockedPair) -> Pair {
    let (disk, vars) = pair.paths().expect("resolve selected pair");
    (
        fs::read(disk).expect("read disk"),
        fs::read(vars).expect("read vars"),
    )
}

#[test]
fn hard_exit_mid_streaming_restore_keeps_selected_pair_and_allows_retry() {
    for selected in [false, true] {
        let tag = format!("managed-stream-crash-{selected}");
        let (_scratch, disk, vars, snapshot, old, new) = fixture(&tag, selected);
        crash_restore(&disk, &vars, &snapshot);
        let mut pair = LockedPair::open(&disk, &vars).expect("reopen after crash");
        let staged = fs::metadata(pair.root.join("staging/disk.raw")).map(|m| m.len());
        assert_eq!(
            staged.ok(),
            Some(COPY_CHUNK as u64),
            "child did not stop mid-disk"
        );
        assert_eq!(contents(&pair), old, "partial staging became selected");
        pair.restore(&snapshot).expect("retry restore after crash");
        assert_eq!(contents(&pair), new, "retry did not select the snapshot");
    }
}
#[path = "managed_pair_staging_failure_tests.rs"]
mod staging_failure_tests;
