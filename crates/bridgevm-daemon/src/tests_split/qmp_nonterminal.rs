//! Nonterminal QMP supervisor event coverage.

use super::helpers::*;
use super::parked_backend::parked_test_backend;
use super::wait::wait_up_to_ten_seconds;
use crate::*;
use bridgevm_qemu::qmp_socket_path;
use bridgevm_storage::{RunnerMetadata, VmRuntimeState};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::thread;
use std::time::Duration;

#[test]
fn reconcile_children_records_nonterminal_qmp_events_without_cleanup() {
    let store = temp_store();
    store.create_vm(&compatibility_manifest("legacy")).unwrap();
    store
        .transition_state("legacy", VmRuntimeState::Running)
        .unwrap();
    store
        .write_runner_metadata(
            "legacy",
            &RunnerMetadata {
                engine: "fullvm".to_string(),
                pid: Some(0),
                command: vec!["sh".to_string(), "-c".to_string(), "sleep 5".to_string()],
                log_path: store.bundle_path("legacy").join("logs").join("qemu.log"),
                started_at_unix: now_unix(),
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

    let (bundle, _) = store.get_vm("legacy").unwrap();
    let socket_path = qmp_socket_path(&bundle);
    let listener = UnixListener::bind(&socket_path).unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .write_all(br#"{"QMP":{"version":{"qemu":{"major":8,"minor":2,"micro":0}}}}"#)
            .unwrap();
        stream.write_all(b"\n").unwrap();

        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        assert!(line.contains("qmp_capabilities"));
        stream.write_all(br#"{"return":{}}"#).unwrap();
        stream.write_all(b"\n").unwrap();
        stream
            .write_all(br#"{"event":"RESUME","data":{"status":"running"}}"#)
            .unwrap();
        stream.write_all(b"\n").unwrap();
        thread::sleep(Duration::from_millis(100));
    });

    let mut state = DaemonState::new(store.clone());
    state
        .children
        .insert("legacy".to_string(), parked_test_backend());

    assert!(wait_up_to_ten_seconds(|| {
        state.reconcile_children().unwrap();
        store.qmp_supervisor_metadata("legacy").unwrap().is_some()
    }));

    assert!(state.children.contains_key("legacy"));
    assert_eq!(
        store.state("legacy").unwrap().state,
        VmRuntimeState::Running
    );
    let qmp = store
        .qmp_supervisor_metadata("legacy")
        .unwrap()
        .expect("qmp supervisor metadata");
    assert_eq!(qmp.envelopes_read, 1);
    assert_eq!(qmp.events.len(), 1);
    assert_eq!(qmp.events[0].name, "RESUME");
    assert_eq!(
        qmp.events[0].data.as_ref().unwrap(),
        &serde_json::json!({"status": "running"})
    );
    assert!(qmp.terminal_event.is_none());
    assert!(!qmp.limit_reached);

    state.cleanup_owned_backend("legacy", false).unwrap();
    server.join().unwrap();
}
