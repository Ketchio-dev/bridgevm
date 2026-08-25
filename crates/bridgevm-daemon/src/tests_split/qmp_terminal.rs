//! Terminal QMP event drain and owned-backend cleanup coverage.

use super::helpers::*;
use super::parked_backend::parked_test_backend;
use super::wait::wait_up_to_ten_seconds;
use crate::*;
use bridgevm_qemu::{qmp_socket_path, QmpClient};
use bridgevm_storage::{RunnerMetadata, VmRuntimeState};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::path::Path;
use std::thread;
use std::time::Duration;

fn negotiated_terminal_qmp_client(bundle: &Path) -> QmpClient {
    let socket_path = qmp_socket_path(bundle);
    let listener = UnixListener::bind(&socket_path).unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        writeln!(
            stream,
            r#"{{"QMP":{{"version":{{"qemu":{{"major":8,"minor":2,"micro":0}}}}}}}}"#
        )
        .unwrap();
        let mut line = String::new();
        BufReader::new(stream.try_clone().unwrap())
            .read_line(&mut line)
            .unwrap();
        assert!(line.contains("qmp_capabilities"), "line={line:?}");
        writeln!(stream, r#"{{"return":{{}}}}"#).unwrap();
        writeln!(
            stream,
            r#"{{"event":"BLOCK_JOB_COMPLETED","data":{{"device":"drive0"}}}}"#
        )
        .unwrap();
        writeln!(stream, r#"{{"event":"SHUTDOWN","data":{{"guest":true}}}}"#).unwrap();
    });
    let mut client =
        QmpClient::connect_with_timeout(&socket_path, Duration::from_secs(10)).unwrap();
    client.negotiate().unwrap();
    server.join().unwrap();
    client
}

#[test]
fn reconcile_children_cleans_up_terminal_qmp_event() {
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
    let client = negotiated_terminal_qmp_client(&bundle);
    let mut backend = parked_test_backend();
    backend.qmp = Some(client);
    let mut state = DaemonState::new(store.clone());
    state.children.insert("legacy".to_string(), backend);

    assert!(wait_up_to_ten_seconds(|| {
        state.reconcile_children().unwrap();
        state.children.is_empty()
    }));
    assert_eq!(
        store.state("legacy").unwrap().state,
        VmRuntimeState::Stopped
    );
    assert_eq!(store.runner_metadata("legacy").unwrap(), None);
    let qmp = store
        .qmp_supervisor_metadata("legacy")
        .unwrap()
        .expect("qmp supervisor metadata");
    assert_eq!(qmp.envelopes_read, 2);
    assert_eq!(
        qmp.events
            .iter()
            .map(|event| event.name.as_str())
            .collect::<Vec<_>>(),
        ["BLOCK_JOB_COMPLETED", "SHUTDOWN"]
    );
    assert_eq!(qmp.terminal_event.as_ref().unwrap().name, "SHUTDOWN");
    assert_eq!(
        qmp.terminal_event.as_ref().unwrap().data.as_ref().unwrap(),
        &serde_json::json!({"guest": true})
    );
    assert!(!qmp.limit_reached);
}
