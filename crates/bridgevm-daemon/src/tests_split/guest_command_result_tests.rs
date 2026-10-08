//! Real command results must be written before a single nonblocking drain is asserted.

use super::guest_command_fixture::GuestCommandFixture;
use bridgevm_agent_protocol::{AgentEnvelope, AgentMessage};
use bridgevm_api::{BridgeVmRequest, BridgeVmResponse};

#[test]
fn daemon_sends_guest_tools_command_and_tracks_result() {
    command_round_trip(false);
}

#[test]
fn delayed_guest_tools_result_stays_pending_until_written() {
    command_round_trip(true);
}

fn command_round_trip(withhold_results: bool) {
    let mut fixture = GuestCommandFixture::new();
    let command = AgentEnvelope::with_request_id(
        AgentMessage::SetClipboard {
            text: "hello from host".to_string(),
        },
        "clipboard-1",
    );
    let response = fixture
        .state
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
    let command = fixture.read();
    assert_eq!(command.request_id.as_deref(), Some("clipboard-1"));
    assert_eq!(
        command.message,
        AgentMessage::SetClipboard {
            text: "hello from host".to_string()
        }
    );
    if withhold_results {
        assert_pending_without_result(&mut fixture, None);
    }
    fixture.write(&AgentEnvelope::new(AgentMessage::CommandResult {
        request_id: "clipboard-1".to_string(),
        ok: true,
        error_code: None,
        message: Some("clipboard accepted".to_string()),
        result: Some(serde_json::json!({ "text_length": 15, "changed": true })),
        metadata: Some(serde_json::json!({ "handler": "clipboard", "duration_ms": 3 })),
    }));
    fixture.state.reconcile_children().unwrap();
    assert_eq!(
        fixture.state.children["legacy"]
            .guest_tools_commands
            .pending_count(),
        0
    );
    let runtime = fixture
        .state
        .store
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
        Some(serde_json::json!({ "text_length": 15, "changed": true }))
    );
    assert_eq!(
        result.metadata,
        Some(serde_json::json!({ "handler": "clipboard", "duration_ms": 3 }))
    );

    let response = fixture
        .state
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
    let command = fixture.read();
    assert_eq!(command.request_id.as_deref(), Some("mount-1"));
    assert_eq!(
        command.message,
        AgentMessage::MountShare {
            name: "work".to_string(),
            host_path_token: "share-token-1".to_string(),
        }
    );
    if withhold_results {
        assert_pending_without_result(&mut fixture, Some("clipboard-1"));
    }
    fixture.write(&AgentEnvelope::new(AgentMessage::CommandResult {
        request_id: "mount-1".to_string(),
        ok: true,
        error_code: None,
        message: None,
        result: None,
        metadata: None,
    }));
    fixture.state.reconcile_children().unwrap();
    assert_eq!(
        fixture.state.children["legacy"]
            .guest_tools_commands
            .pending_count(),
        0
    );
    let runtime = fixture
        .state
        .store
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
    fixture.finish();
}

fn assert_pending_without_result(fixture: &mut GuestCommandFixture, previous: Option<&str>) {
    fixture.state.reconcile_children().unwrap();
    assert_eq!(
        fixture.state.children["legacy"]
            .guest_tools_commands
            .pending_count(),
        1
    );
    let runtime = fixture
        .state
        .store
        .guest_tools_runtime_metadata("legacy")
        .unwrap()
        .unwrap();
    assert!(fixture.state.children["legacy"].guest_tools.is_some());
    assert!(runtime.shared_folders.is_empty());
    assert_eq!(
        runtime
            .last_command_result
            .as_ref()
            .map(|r| r.request_id.as_str()),
        previous
    );
}
