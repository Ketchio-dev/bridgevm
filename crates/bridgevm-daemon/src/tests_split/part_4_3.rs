//! Split test module.

use super::helpers::*;
use crate::*;
use bridgevm_storage::VmRuntimeState;
use std::process::Command;
use std::thread;
use std::time::Duration;

#[test]
fn shutdown_reaps_supervised_children_so_none_orphan() {
    // Regression guard: killing bridgevmd must not leave its spawned QEMU /
    // AppleVzRunner children orphaned (still running, still holding ports).
    let store = temp_store();
    store.create_vm(&compatibility_manifest("legacy")).unwrap();
    store
        .transition_state("legacy", VmRuntimeState::Running)
        .unwrap();

    // A long-lived stand-in for a spawned backend process.
    let child = Command::new("sh")
        .arg("-c")
        .arg("sleep 60")
        .spawn()
        .unwrap();
    let pid = child.id() as libc::pid_t;
    let mut state = DaemonState::new(store.clone());
    state
        .children
        .insert("legacy".to_string(), SupervisedBackend::new(child));

    // Sanity: the child is alive before shutdown.
    assert_eq!(
        unsafe { libc::kill(pid, 0) },
        0,
        "the supervised child should be alive before shutdown"
    );

    state.shutdown_reap_children();

    assert!(
        !state.children.contains_key("legacy"),
        "the supervised child must be removed from the daemon on shutdown"
    );
    // The spawned process must be reaped (SIGKILL + wait), not orphaned.
    let mut gone = false;
    for _ in 0..40 {
        if unsafe { libc::kill(pid, 0) } == -1 {
            gone = true;
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    assert!(
        gone,
        "the supervised child must be killed on shutdown, not left orphaned"
    );
    assert_eq!(
        store.state("legacy").unwrap().state,
        VmRuntimeState::Stopped,
        "the VM should be marked Stopped after its backend is reaped"
    );
}
