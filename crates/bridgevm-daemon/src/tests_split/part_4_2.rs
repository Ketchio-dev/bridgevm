//! Split test module.

use super::helpers::*;
use crate::*;
use bridgevm_agent_protocol::AgentAuth;
use bridgevm_agent_protocol::AgentCapability;
use bridgevm_agent_protocol::AgentEnvelope;
use bridgevm_agent_protocol::AgentMessage;
use bridgevm_agent_protocol::PROTOCOL_VERSION;
use bridgevm_agentd::encode_envelope_line;
use bridgevm_api::BridgeVmRequest;
use bridgevm_api::BridgeVmResponse;
use bridgevm_storage::SnapshotKind;
use bridgevm_storage::VmRuntimeState;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Write;
use std::os::unix::net::UnixListener;
use std::process::Command;
use std::thread;
use std::time::Duration;

#[test]
fn daemon_executes_application_consistent_snapshot_scaffold_commands() {
    let (_root, store) = temp_store();
    store.create_vm(&compatibility_manifest("legacy")).unwrap();
    store
        .transition_state("legacy", VmRuntimeState::Running)
        .unwrap();

    let token = store.guest_tools_token("legacy").unwrap().token;
    let guest_tools = store.guest_tools_runner_metadata("legacy").unwrap();
    let listener = UnixListener::bind(&guest_tools.socket_path).unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let hello = AgentEnvelope::new(AgentMessage::GuestHello {
            version: PROTOCOL_VERSION,
            guest_os: "linux".to_string(),
            agent_version: Some("1.0.0".to_string()),
            capabilities: vec![
                AgentCapability {
                    name: "heartbeat".to_string(),
                    version: 1,
                },
                AgentCapability {
                    name: "fs-freeze".to_string(),
                    version: 1,
                },
                AgentCapability {
                    name: "fs-thaw".to_string(),
                    version: 1,
                },
            ],
            auth: Some(AgentAuth::ToolsToken { token }),
        });
        stream
            .write_all(encode_envelope_line(&hello).unwrap().as_bytes())
            .unwrap();

        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut freeze_line = String::new();
        reader.read_line(&mut freeze_line).unwrap();
        let freeze: AgentEnvelope = serde_json::from_str(freeze_line.trim_end()).unwrap();
        assert_eq!(
            freeze.request_id.as_deref(),
            Some("application-consistent-snapshot:before-upgrade:freeze")
        );
        assert_eq!(
            freeze.message,
            AgentMessage::FreezeFilesystem {
                timeout_millis: Some(5_000),
            }
        );
        stream
            .write_all(
                encode_envelope_line(&AgentEnvelope::new(AgentMessage::CommandResult {
                    request_id: "application-consistent-snapshot:before-upgrade:freeze".to_string(),
                    ok: true,
                    error_code: None,
                    message: Some("freeze scaffold acknowledged".to_string()),
                    result: None,
                    metadata: None,
                }))
                .unwrap()
                .as_bytes(),
            )
            .unwrap();

        let mut thaw_line = String::new();
        reader.read_line(&mut thaw_line).unwrap();
        let thaw: AgentEnvelope = serde_json::from_str(thaw_line.trim_end()).unwrap();
        assert_eq!(
            thaw.request_id.as_deref(),
            Some("application-consistent-snapshot:before-upgrade:thaw")
        );
        assert_eq!(thaw.message, AgentMessage::ThawFilesystem);
        stream
            .write_all(
                encode_envelope_line(&AgentEnvelope::new(AgentMessage::CommandResult {
                    request_id: "application-consistent-snapshot:before-upgrade:thaw".to_string(),
                    ok: true,
                    error_code: None,
                    message: Some("thaw scaffold acknowledged".to_string()),
                    result: None,
                    metadata: None,
                }))
                .unwrap()
                .as_bytes(),
            )
            .unwrap();
        thread::sleep(Duration::from_millis(250));
    });

    let child = Command::new("sh").arg("-c").arg("sleep 5").spawn().unwrap();
    let mut state = DaemonState::new(store.clone());
    state
        .children
        .insert("legacy".to_string(), SupervisedBackend::new(child));

    state.reconcile_children().unwrap();
    let preflight = state
        .handle_request(BridgeVmRequest::SnapshotPreflightStatus {
            name: "legacy".to_string(),
            consistency: bridgevm_api::SnapshotConsistency::ApplicationConsistent,
        })
        .into_result()
        .unwrap();
    let BridgeVmResponse::SnapshotPreflightStatus { preflight } = preflight else {
        panic!("expected snapshot preflight response");
    };
    assert!(preflight.backend_freeze_thaw_supported);
    assert!(preflight.ready);

    let response = state
        .handle_request(BridgeVmRequest::ExecuteApplicationConsistentSnapshot {
            vm: "legacy".to_string(),
            name: "before-upgrade".to_string(),
            freeze_timeout_millis: Some(5_000),
        })
        .into_result()
        .unwrap();
    let BridgeVmResponse::ApplicationConsistentSnapshotExecution { execution } = response else {
        panic!("expected application-consistent snapshot execution response");
    };
    assert_eq!(execution.vm, "legacy");
    assert_eq!(execution.snapshot, "before-upgrade");
    assert_eq!(execution.pending_commands_after_freeze, 0);
    assert_eq!(execution.pending_commands_after_thaw, 0);
    assert_eq!(
        execution.freeze_result.capability.as_deref(),
        Some("fs-freeze")
    );
    assert!(execution.freeze_result.ok);
    assert_eq!(execution.thaw_result.capability.as_deref(), Some("fs-thaw"));
    assert!(execution.thaw_result.ok);

    let snapshots = store.snapshots("legacy").unwrap();
    assert_eq!(snapshots.len(), 1);
    assert_eq!(snapshots[0].kind, SnapshotKind::ApplicationConsistent);

    state.cleanup_owned_backend("legacy", false).unwrap();
    server.join().unwrap();
}

#[test]
fn daemon_thaws_after_application_consistent_snapshot_failure() {
    let (_root, store) = temp_store();
    store.create_vm(&compatibility_manifest("legacy")).unwrap();
    store
        .transition_state("legacy", VmRuntimeState::Running)
        .unwrap();
    store
        .create_snapshot(
            "legacy",
            "duplicate",
            bridgevm_storage::SnapshotKind::ApplicationConsistent,
        )
        .unwrap();

    let token = store.guest_tools_token("legacy").unwrap().token;
    let guest_tools = store.guest_tools_runner_metadata("legacy").unwrap();
    let listener = UnixListener::bind(&guest_tools.socket_path).unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let hello = AgentEnvelope::new(AgentMessage::GuestHello {
            version: PROTOCOL_VERSION,
            guest_os: "linux".to_string(),
            agent_version: Some("1.0.0".to_string()),
            capabilities: vec![
                AgentCapability {
                    name: "heartbeat".to_string(),
                    version: 1,
                },
                AgentCapability {
                    name: "fs-freeze".to_string(),
                    version: 1,
                },
                AgentCapability {
                    name: "fs-thaw".to_string(),
                    version: 1,
                },
            ],
            auth: Some(AgentAuth::ToolsToken { token }),
        });
        stream
            .write_all(encode_envelope_line(&hello).unwrap().as_bytes())
            .unwrap();

        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut freeze_line = String::new();
        reader.read_line(&mut freeze_line).unwrap();
        let freeze: AgentEnvelope = serde_json::from_str(freeze_line.trim_end()).unwrap();
        assert_eq!(
            freeze.request_id.as_deref(),
            Some("application-consistent-snapshot:duplicate:freeze")
        );
        stream
            .write_all(
                encode_envelope_line(&AgentEnvelope::new(AgentMessage::CommandResult {
                    request_id: "application-consistent-snapshot:duplicate:freeze".to_string(),
                    ok: true,
                    error_code: None,
                    message: Some("freeze scaffold acknowledged".to_string()),
                    result: None,
                    metadata: None,
                }))
                .unwrap()
                .as_bytes(),
            )
            .unwrap();

        let mut thaw_line = String::new();
        reader.read_line(&mut thaw_line).unwrap();
        let thaw: AgentEnvelope = serde_json::from_str(thaw_line.trim_end()).unwrap();
        assert_eq!(
            thaw.request_id.as_deref(),
            Some("application-consistent-snapshot:duplicate:thaw")
        );
        assert_eq!(thaw.message, AgentMessage::ThawFilesystem);
        stream
            .write_all(
                encode_envelope_line(&AgentEnvelope::new(AgentMessage::CommandResult {
                    request_id: "application-consistent-snapshot:duplicate:thaw".to_string(),
                    ok: true,
                    error_code: None,
                    message: Some("thaw scaffold acknowledged".to_string()),
                    result: None,
                    metadata: None,
                }))
                .unwrap()
                .as_bytes(),
            )
            .unwrap();
        thread::sleep(Duration::from_millis(250));
    });

    let child = Command::new("sh").arg("-c").arg("sleep 5").spawn().unwrap();
    let mut state = DaemonState::new(store.clone());
    state
        .children
        .insert("legacy".to_string(), SupervisedBackend::new(child));

    state.reconcile_children().unwrap();
    let response = state.handle_request(BridgeVmRequest::ExecuteApplicationConsistentSnapshot {
        vm: "legacy".to_string(),
        name: "duplicate".to_string(),
        freeze_timeout_millis: Some(5_000),
    });
    let BridgeVmResponse::Error { message } = response else {
        panic!("expected duplicate snapshot error");
    };
    assert!(message.contains("failed to create application-consistent snapshot"));

    state.reconcile_children().unwrap();
    let runtime = store
        .guest_tools_runtime_metadata("legacy")
        .unwrap()
        .expect("runtime metadata");
    let result = runtime.last_command_result.expect("last command result");
    assert_eq!(
        result.request_id,
        "application-consistent-snapshot:duplicate:thaw"
    );
    assert_eq!(result.capability.as_deref(), Some("fs-thaw"));
    assert!(result.ok);

    state.cleanup_owned_backend("legacy", false).unwrap();
    server.join().unwrap();
}

#[test]
fn shell_word_split_handles_quotes_and_escapes() {
    assert_eq!(
        shell_word_split("-drive file=/tmp/a b.iso,if=virtio,format=raw"),
        vec![
            "-drive".to_string(),
            "file=/tmp/a".to_string(),
            "b.iso,if=virtio,format=raw".to_string(),
        ]
    );
    assert_eq!(
        shell_word_split("-drive 'file=/tmp/with space.iso,if=virtio'"),
        vec![
            "-drive".to_string(),
            "file=/tmp/with space.iso,if=virtio".to_string(),
        ]
    );
    assert_eq!(
        shell_word_split("-drive \"file=/tmp/x.iso,id=cidata\""),
        vec![
            "-drive".to_string(),
            "file=/tmp/x.iso,id=cidata".to_string()
        ]
    );
    assert_eq!(
        shell_word_split("file=/tmp/a\\ b.iso"),
        vec!["file=/tmp/a b.iso".to_string()]
    );
    assert!(shell_word_split("   ").is_empty());
}

#[test]
fn daemon_surfaces_thaw_failure_after_successful_snapshot() {
    // The snapshot succeeds and the freeze entered the boundary, but the
    // agent's thaw reply is ok:false. The orchestration must still have
    // DISPATCHED the thaw (the guest cannot be left frozen silently) and
    // then surface the thaw failure to the caller.
    let (_root, store) = temp_store();
    store.create_vm(&compatibility_manifest("legacy")).unwrap();
    store
        .transition_state("legacy", VmRuntimeState::Running)
        .unwrap();

    let token = store.guest_tools_token("legacy").unwrap().token;
    let guest_tools = store.guest_tools_runner_metadata("legacy").unwrap();
    let listener = UnixListener::bind(&guest_tools.socket_path).unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let hello = AgentEnvelope::new(AgentMessage::GuestHello {
            version: PROTOCOL_VERSION,
            guest_os: "linux".to_string(),
            agent_version: Some("1.0.0".to_string()),
            capabilities: vec![
                AgentCapability {
                    name: "heartbeat".to_string(),
                    version: 1,
                },
                AgentCapability {
                    name: "fs-freeze".to_string(),
                    version: 1,
                },
                AgentCapability {
                    name: "fs-thaw".to_string(),
                    version: 1,
                },
            ],
            auth: Some(AgentAuth::ToolsToken { token }),
        });
        stream
            .write_all(encode_envelope_line(&hello).unwrap().as_bytes())
            .unwrap();

        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut freeze_line = String::new();
        reader.read_line(&mut freeze_line).unwrap();
        let freeze: AgentEnvelope = serde_json::from_str(freeze_line.trim_end()).unwrap();
        assert_eq!(
            freeze.request_id.as_deref(),
            Some("application-consistent-snapshot:after-thaw-fail:freeze")
        );
        assert_eq!(
            freeze.message,
            AgentMessage::FreezeFilesystem {
                timeout_millis: Some(5_000),
            }
        );
        stream
            .write_all(
                encode_envelope_line(&AgentEnvelope::new(AgentMessage::CommandResult {
                    request_id: "application-consistent-snapshot:after-thaw-fail:freeze"
                        .to_string(),
                    ok: true,
                    error_code: None,
                    message: Some("freeze acknowledged".to_string()),
                    result: None,
                    metadata: None,
                }))
                .unwrap()
                .as_bytes(),
            )
            .unwrap();

        // The thaw MUST still be dispatched even after a successful
        // snapshot. Reply ok:false to assert the failure is surfaced.
        let mut thaw_line = String::new();
        reader.read_line(&mut thaw_line).unwrap();
        let thaw: AgentEnvelope = serde_json::from_str(thaw_line.trim_end()).unwrap();
        assert_eq!(
            thaw.request_id.as_deref(),
            Some("application-consistent-snapshot:after-thaw-fail:thaw")
        );
        assert_eq!(thaw.message, AgentMessage::ThawFilesystem);
        stream
            .write_all(
                encode_envelope_line(&AgentEnvelope::new(AgentMessage::CommandResult {
                    request_id: "application-consistent-snapshot:after-thaw-fail:thaw".to_string(),
                    ok: false,
                    error_code: Some("filesystem-thaw-failed".to_string()),
                    message: Some("fsfreeze -u failed".to_string()),
                    result: None,
                    metadata: None,
                }))
                .unwrap()
                .as_bytes(),
            )
            .unwrap();
        thread::sleep(Duration::from_millis(250));
    });

    let child = Command::new("sh").arg("-c").arg("sleep 5").spawn().unwrap();
    let mut state = DaemonState::new(store.clone());
    state
        .children
        .insert("legacy".to_string(), SupervisedBackend::new(child));

    state.reconcile_children().unwrap();
    let response = state.handle_request(BridgeVmRequest::ExecuteApplicationConsistentSnapshot {
        vm: "legacy".to_string(),
        name: "after-thaw-fail".to_string(),
        freeze_timeout_millis: Some(5_000),
    });
    let BridgeVmResponse::Error { message } = response else {
        panic!("expected thaw-failure error response");
    };
    assert!(
        message.contains("guest tools thaw failed"),
        "unexpected error: {message}"
    );

    // The snapshot was recorded (thaw failed only afterwards), and the thaw
    // command WAS dispatched + tracked as the last command result.
    let snapshots = store.snapshots("legacy").unwrap();
    assert_eq!(snapshots.len(), 1);
    let runtime = store
        .guest_tools_runtime_metadata("legacy")
        .unwrap()
        .expect("runtime metadata");
    let result = runtime.last_command_result.expect("last command result");
    assert_eq!(
        result.request_id,
        "application-consistent-snapshot:after-thaw-fail:thaw"
    );
    assert_eq!(result.capability.as_deref(), Some("fs-thaw"));
    assert!(!result.ok);

    state.cleanup_owned_backend("legacy", false).unwrap();
    server.join().unwrap();
}
