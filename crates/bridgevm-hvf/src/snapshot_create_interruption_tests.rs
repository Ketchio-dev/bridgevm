use super::*;
use crate::snapshot_pair::snapshot_pair_tests::completed_staging_tests::retry_create;
use crate::snapshot_pair::{snapshot_pair_tests::Scratch, verify_snapshot};
use std::process::Command;

const SCENARIO_ENV: &str = "BRIDGEVM_TEST_SNAPSHOT_CREATE_CRASH";
const DISK_ENV: &str = "BRIDGEVM_TEST_SNAPSHOT_CREATE_DISK";
const VARS_ENV: &str = "BRIDGEVM_TEST_SNAPSHOT_CREATE_VARS";
const DEST_ENV: &str = "BRIDGEVM_TEST_SNAPSHOT_CREATE_DEST";

fn required_path(name: &str) -> PathBuf {
    std::env::var_os(name)
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("missing subprocess path {name}"))
}

/// The last field is whether the attempt leaves a finished snapshot in staging.
fn selected_scenario(stage: CreateStage) -> (&'static str, i32, bool) {
    match stage {
        CreateStage::DiskSynced => ("disk-synced", 81, false),
        CreateStage::VarsSynced => ("vars-synced", 82, false),
        CreateStage::ManifestPublished => ("manifest-published", 83, true),
        CreateStage::StagingDirectorySynced => ("staging-directory-synced", 84, true),
        CreateStage::SnapshotPublished => ("snapshot-published", 85, false),
    }
}

#[test]
fn create_crash_child() {
    let Ok(scenario) = std::env::var(SCENARIO_ENV) else {
        return;
    };
    let disk = required_path(DISK_ENV);
    let vars = required_path(VARS_ENV);
    let dest = required_path(DEST_ENV);
    create_snapshot_using(&disk, &vars, &dest, "new", false, 1024, |stage| {
        let (name, exit, _) = selected_scenario(stage);
        if scenario == name {
            std::process::exit(exit);
        }
    })
    .expect("the selected create boundary must exit before return");
    panic!("unknown create crash scenario {scenario}");
}

fn crash_create(scenario: &str, exit: i32, disk: &Path, vars: &Path, dest: &Path) {
    let status = Command::new(std::env::current_exe().expect("current test executable"))
        .arg("snapshot_pair::creation::interruption_tests::create_crash_child")
        .arg("--exact")
        .arg("--nocapture")
        .env(SCENARIO_ENV, scenario)
        .env(DISK_ENV, disk)
        .env(VARS_ENV, vars)
        .env(DEST_ENV, dest)
        .status()
        .expect("launch create crash subprocess");
    assert_eq!(status.code(), Some(exit), "child status {status}");
}

#[test]
fn hard_exit_during_create_selects_only_one_complete_snapshot_and_allows_retry() {
    for stage in [
        CreateStage::DiskSynced,
        CreateStage::VarsSynced,
        CreateStage::ManifestPublished,
        CreateStage::StagingDirectorySynced,
        CreateStage::SnapshotPublished,
    ] {
        let (scenario, exit, finished) = selected_scenario(stage);
        let scratch = Scratch::new(scenario);
        let disk = scratch.write("disk", b"old-disk");
        let vars = scratch.write("vars", b"old-vars");
        let dest = scratch.path("snapshot");
        let old = create_snapshot(&disk, &vars, &dest, "old", false, 1024).unwrap();
        fs::write(&disk, b"new-disk").unwrap();
        fs::write(&vars, b"new-vars").unwrap();

        crash_create(scenario, exit, &disk, &vars, &dest);
        let selected = verify_snapshot(&dest).expect("one complete snapshot remains selected");
        if stage == CreateStage::SnapshotPublished {
            assert_eq!(selected.vm_id, "new");
        } else {
            assert_eq!(selected, old);
        }

        let retried = retry_create(&disk, &vars, &dest, finished);
        assert_eq!(verify_snapshot(&dest).unwrap(), retried);
        assert_eq!(retried.vm_id, "retry");
    }
}
