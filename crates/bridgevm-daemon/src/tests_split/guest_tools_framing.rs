//! Active guest-tools reads retain fragments through drains and command waits.

use super::helpers::*;
use crate::*;
use bridgevm_agent_protocol::{AgentCapability, AgentEnvelope, AgentMessage};
use bridgevm_agentd::{encode_envelope_line, read_envelope_line, AgentSession, EnvelopeLineReader};
use std::io::{BufReader, Write};
use std::os::unix::net::UnixStream;
use std::process::Command;
use std::time::Duration;

fn connected_state() -> (TestStoreRoot, DaemonState, UnixStream) {
    let (_root, store) = temp_store();
    store.create_vm(&compatibility_manifest("legacy")).unwrap();
    let (host, guest) = UnixStream::pair().unwrap();
    host.set_read_timeout(Some(Duration::from_millis(25)))
        .unwrap();
    let child = Command::new("sh").arg("-c").arg("sleep 5").spawn().unwrap();
    let mut backend = SupervisedBackend::new(child);
    backend.guest_tools = Some(AgentSession {
        guest_os: "linux".to_string(),
        agent_version: Some("1.0.0".to_string()),
        capabilities: ["heartbeat", "clipboard"]
            .into_iter()
            .map(|name| AgentCapability {
                name: name.to_string(),
                version: 1,
            })
            .collect(),
    });
    backend.guest_tools_stream = Some(EnvelopeLineReader::new(BufReader::new(host)));
    let mut state = DaemonState::new(store);
    state.children.insert("legacy".to_string(), backend);
    (_root, state, guest)
}

#[test]
fn drain_reassembles_active_heartbeat_after_socket_timeout() {
    let (_root, mut state, mut guest) = connected_state();
    let line = encode_envelope_line(&AgentEnvelope::new(AgentMessage::Heartbeat)).unwrap();
    let split = line.len() / 2;
    guest.write_all(&line.as_bytes()[..split]).unwrap();
    let backend = state.children.get_mut("legacy").unwrap();
    drain_guest_tools_messages(&state.store, "legacy", backend).unwrap();
    assert!(backend.guest_tools.is_some());
    assert!(state
        .store
        .guest_tools_runtime_metadata("legacy")
        .unwrap()
        .is_none());
    guest.write_all(&line.as_bytes()[split..]).unwrap();
    drain_guest_tools_messages(&state.store, "legacy", backend).unwrap();
    let runtime = state
        .store
        .guest_tools_runtime_metadata("legacy")
        .unwrap()
        .unwrap();
    assert!(runtime.last_heartbeat_at_unix.is_some());
    assert!(backend.guest_tools.is_some());
    state.cleanup_owned_backend("legacy", false).unwrap();
}

#[test]
fn command_wait_retains_result_prefix_when_wait_times_out() {
    let (_root, mut state, mut guest) = connected_state();
    let command = AgentEnvelope::with_request_id(
        AgentMessage::SetClipboard {
            text: "test".to_string(),
        },
        "fragmented-result",
    );
    state
        .send_guest_tools_command_record("legacy", command.clone())
        .unwrap();
    assert_eq!(
        read_envelope_line(&mut BufReader::new(guest.try_clone().unwrap())),
        Ok(Some(command))
    );
    let result = AgentEnvelope::new(AgentMessage::CommandResult {
        request_id: "fragmented-result".to_string(),
        ok: true,
        error_code: None,
        message: Some("completed".to_string()),
        result: None,
        metadata: None,
    });
    let line = encode_envelope_line(&result).unwrap();
    let split = line.len() / 2;
    guest.write_all(&line.as_bytes()[..split]).unwrap();
    let error = state
        .wait_for_guest_tools_command_result("legacy", "fragmented-result", Duration::ZERO)
        .unwrap_err();
    assert!(error.to_string().contains("timed out waiting"));
    assert!(state.children["legacy"].guest_tools.is_some());
    guest.write_all(&line.as_bytes()[split..]).unwrap();
    let completed = state
        .wait_for_guest_tools_command_result("legacy", "fragmented-result", Duration::from_secs(1))
        .unwrap();
    assert!(completed.ok);
    assert_eq!(completed.request_id, "fragmented-result");
    assert_eq!(completed.pending_commands, 0);
    state.cleanup_owned_backend("legacy", false).unwrap();
}
