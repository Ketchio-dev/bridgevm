//! Reconcile must retain exited children until state and runner cleanup persist.

use super::helpers::compatibility_manifest;
use super::helpers::temp_store;
use crate::*;
use bridgevm_storage::RunnerMetadata;
use bridgevm_storage::VmRuntimeState;
use bridgevm_storage::VmStore;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn exited_backend() -> (VmStore, DaemonState, PathBuf, PathBuf) {
    let store = temp_store();
    store.create_vm(&compatibility_manifest("legacy")).unwrap();
    store
        .transition_state("legacy", VmRuntimeState::Running)
        .unwrap();

    let mut child = Command::new("/bin/sh")
        .arg("-c")
        .arg("exit 0")
        .spawn()
        .unwrap();
    let pid = child.id();
    assert!(child.wait().unwrap().success());

    let bundle = store.bundle_path("legacy");
    let state_path = bundle.join("metadata").join("state.json");
    let runner_path = bundle.join("metadata").join("runner.json");
    store
        .write_runner_metadata(
            "legacy",
            &RunnerMetadata {
                engine: "fullvm".to_string(),
                pid: Some(pid),
                command: vec![
                    "/bin/sh".to_string(),
                    "-c".to_string(),
                    "exit 0".to_string(),
                ],
                log_path: bundle.join("logs").join("qemu.log"),
                started_at_unix: 0,
                dry_run: false,
                launch_spec_path: None,
                guest_tools: None,
                disk: None,
                active_disk: None,
                launch_readiness: None,
                runtime_control: None,
            },
        )
        .unwrap();

    let mut state = DaemonState::new(store.clone());
    state
        .children
        .insert("legacy".to_string(), SupervisedBackend::new(child));
    (store, state, state_path, runner_path)
}

#[test]
fn exited_child_is_retained_until_stopped_state_persists() {
    let (store, mut state, state_path, _) = exited_backend();
    fs::remove_file(&state_path).unwrap();
    fs::create_dir(&state_path).unwrap();

    let error = state.reconcile_children().unwrap_err();

    assert!(error
        .to_string()
        .contains("failed to mark exited backend 'legacy' stopped"));
    assert!(state.children.contains_key("legacy"));
    assert!(store.runner_metadata("legacy").unwrap().is_some());

    fs::remove_dir(&state_path).unwrap();
    state.reconcile_children().unwrap();

    assert!(!state.children.contains_key("legacy"));
    assert_eq!(
        store.state("legacy").unwrap().state,
        VmRuntimeState::Stopped
    );
    assert!(store.runner_metadata("legacy").unwrap().is_none());
    fs::remove_dir_all(store.root()).unwrap();
}

#[test]
fn exited_child_is_retained_until_runner_metadata_clears() {
    let (store, mut state, _, runner_path) = exited_backend();
    fs::remove_file(&runner_path).unwrap();
    fs::create_dir(&runner_path).unwrap();

    let error = state.reconcile_children().unwrap_err();

    assert!(error
        .to_string()
        .contains("failed to clear runner metadata for 'legacy'"));
    assert!(state.children.contains_key("legacy"));
    assert_eq!(
        store.state("legacy").unwrap().state,
        VmRuntimeState::Stopped
    );
    assert!(runner_path.is_dir());

    fs::remove_dir(&runner_path).unwrap();
    state.reconcile_children().unwrap();

    assert!(!state.children.contains_key("legacy"));
    assert!(store.runner_metadata("legacy").unwrap().is_none());
    fs::remove_dir_all(store.root()).unwrap();
}
