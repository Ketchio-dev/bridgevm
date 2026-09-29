use super::super::init::InitStage;
use super::*;
use crate::snapshot_pair::{create_snapshot, snapshot_pair_tests::Scratch};
use std::process::Command;

const SCENARIO_ENV: &str = "BRIDGEVM_TEST_MANAGED_PAIR_INIT_CRASH";
const DISK_ENV: &str = "BRIDGEVM_TEST_MANAGED_PAIR_INIT_DISK";
const VARS_ENV: &str = "BRIDGEVM_TEST_MANAGED_PAIR_INIT_VARS";

fn required_path(name: &str) -> PathBuf {
    std::env::var_os(name)
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("missing subprocess path {name}"))
}

fn selected_scenario(stage: InitStage) -> (&'static str, i32) {
    match stage {
        InitStage::DirectoryCreated => ("directory-created", 95),
        InitStage::MarkerSynced => ("marker-synced", 96),
        InitStage::RootPublished => ("root-published", 97),
    }
}

/// Subprocess entry point. A normal test run leaves this inert; the parent
/// selects it alone and names the initialization boundary to die at.
#[test]
fn initialize_crash_child() {
    let Ok(scenario) = std::env::var(SCENARIO_ENV) else {
        return;
    };
    let disk = required_path(DISK_ENV);
    let vars = required_path(VARS_ENV);
    let pair = LockedPair::open(&disk, &vars).expect("child owns pair");
    init::initialize(&pair.root, |stage| {
        let (name, exit) = selected_scenario(stage);
        if scenario == name {
            std::process::exit(exit);
        }
    })
    .expect("the selected initialization boundary must exit before return");
    panic!("unknown initialization crash scenario {scenario}");
}

fn crash_initialize(scenario: &str, exit: i32, disk: &Path, vars: &Path) {
    let status = Command::new(std::env::current_exe().expect("current test executable"))
        .arg("snapshot_pair::managed::tests::init_interruption_tests::initialize_crash_child")
        .arg("--exact")
        .arg("--nocapture")
        .env(SCENARIO_ENV, scenario)
        .env(DISK_ENV, disk)
        .env(VARS_ENV, vars)
        .status()
        .expect("launch initialization crash subprocess");
    assert_eq!(status.code(), Some(exit), "child status {status}");
}

fn contents(pair: &LockedPair) -> (Vec<u8>, Vec<u8>) {
    let (disk, vars) = pair.paths().expect("resolve selected pair");
    (
        fs::read(disk).expect("read disk"),
        fs::read(vars).expect("read vars"),
    )
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
        "interrupted-init",
        false,
        1024,
    )
    .expect("create source snapshot");
    (scratch, disk, vars, snapshot)
}

fn managed_entries(scratch: &Scratch) -> Vec<std::ffi::OsString> {
    fs::read_dir(&scratch.0)
        .expect("list pair directory")
        .map(|entry| entry.expect("pair directory entry").file_name())
        .filter(|name| name.to_string_lossy().starts_with(".bridgevm-"))
        .collect()
}

/// No generation exists before initialization finishes, so a crash inside it
/// must leave the originals selectable and a retried restore able to publish.
#[test]
fn hard_exit_during_store_initialization_keeps_pair_usable() {
    for stage in [
        InitStage::DirectoryCreated,
        InitStage::MarkerSynced,
        InitStage::RootPublished,
    ] {
        let (scenario, exit) = selected_scenario(stage);
        let (scratch, disk, vars, snapshot) = fixture(&format!("init-crash-{scenario}"));
        crash_initialize(scenario, exit, &disk, &vars);
        let mut pair = LockedPair::open(&disk, &vars)
            .unwrap_or_else(|error| panic!("{scenario} left the pair refused: {error}"));
        assert_eq!(
            contents(&pair),
            (b"old-disk".to_vec(), b"old-vars".to_vec()),
            "no generation was published before {scenario}"
        );
        pair.restore(&snapshot)
            .unwrap_or_else(|error| panic!("retry restore after {scenario}: {error}"));
        assert_eq!(
            contents(&pair),
            (b"new-disk".to_vec(), b"new-vars".to_vec()),
            "retry must publish one complete pair after {scenario}"
        );
        drop(pair);
        let reopened = LockedPair::open(&disk, &vars).expect("reopen restored pair");
        assert_eq!(
            contents(&reopened),
            (b"new-disk".to_vec(), b"new-vars".to_vec())
        );
        assert_eq!(
            managed_entries(&scratch),
            [reopened.root.file_name().unwrap().to_os_string()],
            "only the published root may remain after {scenario}"
        );
    }
}

#[path = "managed_pair_init_debris_tests.rs"]
mod debris_tests;
