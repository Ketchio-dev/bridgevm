//! Regression for daemon-owned suspend failure ownership.

use super::helpers::compatibility_manifest;
use super::helpers::temp_store;
use crate::*;
use bridgevm_storage::VmRuntimeState;
use std::fs;
use std::process::Command;

#[test]
fn failed_owned_suspend_keeps_the_live_child_supervised() {
    let store = temp_store();
    let manifest = compatibility_manifest("legacy");
    store.create_vm(&manifest).unwrap();
    store
        .transition_state("legacy", VmRuntimeState::Running)
        .unwrap();

    let child = Command::new("/bin/sleep").arg("5").spawn().unwrap();
    let pid = child.id();
    let mut state = DaemonState::new(store.clone());
    state
        .children
        .insert("legacy".to_string(), SupervisedBackend::new(child));

    let error = state.suspend_backend_supervised("legacy").unwrap_err();
    let retained = state.children.get_mut("legacy").map(|backend| {
        let live = backend.child.try_wait().unwrap().is_none();
        (backend.child.id(), live)
    });
    let runtime_state = store.state("legacy").unwrap().state;

    if let Some(mut backend) = state.children.remove("legacy") {
        let _ = backend.child.kill();
        let _ = backend.child.wait();
    }
    store
        .force_transition_state("legacy", VmRuntimeState::Stopped)
        .unwrap();
    fs::remove_dir_all(store.root()).unwrap();

    assert!(error.to_string().contains("QMP socket unavailable"));
    assert_eq!(retained, Some((pid, true)));
    assert_eq!(runtime_state, VmRuntimeState::Running);
}
