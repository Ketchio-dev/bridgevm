//! Proves live VNC reservations fail closed when child metadata is unknown.

use super::helpers::compatibility_manifest;
use super::helpers::temp_store;
use crate::*;
use bridgevm_storage::RunnerMetadata;
use bridgevm_storage::VmStore;
use std::fs;
use std::process::Command;

fn live_backend(command: Vec<String>) -> (VmStore, DaemonState) {
    let store = temp_store();
    store.create_vm(&compatibility_manifest("legacy")).unwrap();
    let child = Command::new("/bin/sleep").arg("5").spawn().unwrap();
    let pid = child.id();
    let bundle = store.bundle_path("legacy");
    store
        .write_runner_metadata(
            "legacy",
            &RunnerMetadata {
                engine: "fullvm".to_string(),
                pid: Some(pid),
                command,
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
    (store, state)
}

fn cleanup(store: &VmStore, state: &mut DaemonState) {
    if let Some(mut backend) = state.children.remove("legacy") {
        let _ = backend.child.kill();
        let _ = backend.child.wait();
    }
    fs::remove_dir_all(store.root()).unwrap();
}

#[test]
fn live_vnc_displays_reads_the_recorded_reservation() {
    let (store, mut state) = live_backend(vec![
        "qemu-system-x86_64".to_string(),
        "-display".to_string(),
        "vnc=:7".to_string(),
    ]);

    assert_eq!(state.live_vnc_displays().unwrap(), vec![7]);

    cleanup(&store, &mut state);
}

#[test]
fn live_non_vnc_backend_metadata_is_allowed() {
    let (store, mut state) = live_backend(vec![
        "AppleVzRunner".to_string(),
        "--launch-spec".to_string(),
        "vm.json".to_string(),
    ]);

    assert!(state.live_vnc_displays().unwrap().is_empty());

    cleanup(&store, &mut state);
}

#[test]
fn live_vnc_displays_rejects_missing_runner_metadata() {
    let store = temp_store();
    store.create_vm(&compatibility_manifest("legacy")).unwrap();
    let child = Command::new("/bin/sleep").arg("5").spawn().unwrap();
    let mut state = DaemonState::new(store.clone());
    state
        .children
        .insert("legacy".to_string(), SupervisedBackend::new(child));

    let error = state.live_vnc_displays().unwrap_err();

    assert!(error
        .to_string()
        .contains("live backend 'legacy' has no runner metadata"));
    cleanup(&store, &mut state);
}

#[test]
fn live_vnc_displays_rejects_corrupt_runner_metadata() {
    let (store, mut state) = live_backend(vec![
        "qemu-system-x86_64".to_string(),
        "-display".to_string(),
        "vnc=:3".to_string(),
    ]);
    let runner_path = store
        .bundle_path("legacy")
        .join("metadata")
        .join("runner.json");
    fs::write(&runner_path, b"not-json").unwrap();

    let error = state.live_vnc_displays().unwrap_err();

    assert!(error
        .to_string()
        .contains("failed to read runner metadata for live backend 'legacy'"));
    cleanup(&store, &mut state);
}
