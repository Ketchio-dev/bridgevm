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
use bridgevm_config::SharedFolder;
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
fn daemon_sends_guest_tools_command_and_tracks_result() {
    let store = temp_store();
    let mut manifest = compatibility_manifest("legacy");
    manifest.shared_folders = vec![SharedFolder {
        name: "work".to_string(),
        host_path: "/Users/me/work".to_string(),
        read_only: false,
        host_path_token: Some("share-token-1".to_string()),
    }];
    store.create_vm(&manifest).unwrap();
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
                    name: "clipboard".to_string(),
                    version: 1,
                },
                AgentCapability {
                    name: "shared-folders".to_string(),
                    version: 1,
                },
            ],
            auth: Some(AgentAuth::ToolsToken { token }),
        });
        stream
            .write_all(encode_envelope_line(&hello).unwrap().as_bytes())
            .unwrap();

        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut command_line = String::new();
        reader.read_line(&mut command_line).unwrap();
        let command: AgentEnvelope = serde_json::from_str(command_line.trim_end()).unwrap();
        assert_eq!(command.request_id.as_deref(), Some("clipboard-1"));
        assert_eq!(
            command.message,
            AgentMessage::SetClipboard {
                text: "hello from host".to_string()
            }
        );

        let result = AgentEnvelope::new(AgentMessage::CommandResult {
            request_id: "clipboard-1".to_string(),
            ok: true,
            error_code: None,
            message: Some("clipboard accepted".to_string()),
            result: Some(serde_json::json!({
                "text_length": 15,
                "changed": true
            })),
            metadata: Some(serde_json::json!({
                "handler": "clipboard",
                "duration_ms": 3
            })),
        });
        stream
            .write_all(encode_envelope_line(&result).unwrap().as_bytes())
            .unwrap();

        let mut command_line = String::new();
        reader.read_line(&mut command_line).unwrap();
        let command: AgentEnvelope = serde_json::from_str(command_line.trim_end()).unwrap();
        assert_eq!(command.request_id.as_deref(), Some("mount-1"));
        assert_eq!(
            command.message,
            AgentMessage::MountShare {
                name: "work".to_string(),
                host_path_token: "share-token-1".to_string()
            }
        );

        let result = AgentEnvelope::new(AgentMessage::CommandResult {
            request_id: "mount-1".to_string(),
            ok: true,
            error_code: None,
            message: None,
            result: None,
            metadata: None,
        });
        stream
            .write_all(encode_envelope_line(&result).unwrap().as_bytes())
            .unwrap();
        thread::sleep(Duration::from_millis(250));
    });

    let child = Command::new("sh").arg("-c").arg("sleep 5").spawn().unwrap();
    let mut state = DaemonState::new(store.clone());
    state
        .children
        .insert("legacy".to_string(), SupervisedBackend::new(child));

    state.reconcile_children().unwrap();
    let command = AgentEnvelope::with_request_id(
        AgentMessage::SetClipboard {
            text: "hello from host".to_string(),
        },
        "clipboard-1",
    );
    let response = state
        .handle_request(BridgeVmRequest::GuestToolsSendCommand {
            name: "legacy".to_string(),
            envelope: command,
        })
        .into_result()
        .unwrap();
    let BridgeVmResponse::GuestToolsCommand { command } = response else {
        panic!("expected guest tools command response");
    };
    assert_eq!(command.request_id.as_deref(), Some("clipboard-1"));
    assert_eq!(command.pending_commands, 1);

    state.reconcile_children().unwrap();
    let backend = state.children.get("legacy").unwrap();
    assert_eq!(backend.guest_tools_commands.pending_count(), 0);
    let runtime = store
        .guest_tools_runtime_metadata("legacy")
        .unwrap()
        .expect("runtime metadata");
    let result = runtime.last_command_result.expect("last command result");
    assert_eq!(result.request_id, "clipboard-1");
    assert_eq!(result.capability.as_deref(), Some("clipboard"));
    assert!(result.ok);
    assert_eq!(result.message.as_deref(), Some("clipboard accepted"));
    assert_eq!(
        result.result,
        Some(serde_json::json!({
            "text_length": 15,
            "changed": true
        }))
    );
    assert_eq!(
        result.metadata,
        Some(serde_json::json!({
            "handler": "clipboard",
            "duration_ms": 3
        }))
    );

    let response = state
        .handle_request(BridgeVmRequest::GuestToolsMountApprovedShare {
            name: "legacy".to_string(),
            share: "work".to_string(),
            request_id: Some("mount-1".to_string()),
        })
        .into_result()
        .unwrap();
    let BridgeVmResponse::GuestToolsCommand { command } = response else {
        panic!("expected guest tools command response");
    };
    assert_eq!(command.request_id.as_deref(), Some("mount-1"));
    assert_eq!(command.pending_commands, 1);

    state.reconcile_children().unwrap();
    let backend = state.children.get("legacy").unwrap();
    assert_eq!(backend.guest_tools_commands.pending_count(), 0);
    let runtime = store
        .guest_tools_runtime_metadata("legacy")
        .unwrap()
        .expect("runtime metadata");
    assert_eq!(runtime.shared_folders.len(), 1);
    assert_eq!(runtime.shared_folders[0].name, "work");
    assert_eq!(runtime.shared_folders[0].host_path_token, "share-token-1");
    let result = runtime.last_command_result.expect("last command result");
    assert_eq!(result.request_id, "mount-1");
    assert_eq!(result.capability.as_deref(), Some("shared-folders"));
    assert!(result.ok);

    state.cleanup_owned_backend("legacy", false).unwrap();
    server.join().unwrap();
}

#[test]
fn daemon_executes_application_consistent_snapshot_scaffold_commands() {
    let store = temp_store();
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
    let store = temp_store();
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
    let store = temp_store();
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
