//! Split test module.

use crate::*;
use bridgevm_agent_protocol::AgentEnvelope;
use bridgevm_agent_protocol::AgentMessage;

#[test]
fn file_drop_commands_track_alpha_transfer_state() {
    let mut state = GuestToolsState::new(&default_capabilities());
    let start = AgentEnvelope::with_request_id(
        AgentMessage::FileDropStart {
            transfer_id: "drop-1".to_string(),
            file_name: "notes.txt".to_string(),
            size_bytes: 11,
        },
        "drop-start-1",
    );
    let chunk = AgentEnvelope::with_request_id(
        AgentMessage::FileDropChunk {
            transfer_id: "drop-1".to_string(),
            chunk_index: 0,
            data_base64: "aGVsbG8gd29ybGQ=".to_string(),
        },
        "drop-chunk-1",
    );
    let complete = AgentEnvelope::with_request_id(
        AgentMessage::FileDropComplete {
            transfer_id: "drop-1".to_string(),
        },
        "drop-complete-1",
    );

    assert_eq!(
        state.handle_command(&start).unwrap().message,
        AgentMessage::CommandResult {
            request_id: "drop-start-1".to_string(),
            ok: true,
            error_code: None,
            message: Some("started file drop drop-1".to_string()),
            result: None,
            metadata: None,
        }
    );
    assert_eq!(
        state.handle_command(&chunk).unwrap().message,
        AgentMessage::CommandResult {
            request_id: "drop-chunk-1".to_string(),
            ok: true,
            error_code: None,
            message: Some("accepted file drop drop-1 chunk 0".to_string()),
            result: None,
            metadata: None,
        }
    );
    assert_eq!(
        state.handle_command(&complete).unwrap().message,
        AgentMessage::CommandResult {
            request_id: "drop-complete-1".to_string(),
            ok: true,
            error_code: None,
            message: Some("completed file drop notes.txt (11 bytes across 1 chunks)".to_string()),
            result: None,
            metadata: None,
        }
    );
    assert!(state.file_drops.is_empty());
}
