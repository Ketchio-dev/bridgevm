//! Agent-update runtime-metadata reconciliation tests.

use super::helpers::*;
use super::wait::wait_up_to_ten_seconds;
use crate::*;
use bridgevm_agent_protocol::AgentAuth;
use bridgevm_agent_protocol::AgentCapability;
use bridgevm_agent_protocol::AgentEnvelope;
use bridgevm_agent_protocol::AgentMessage;
use bridgevm_agent_protocol::PROTOCOL_VERSION;
use bridgevm_agentd::encode_envelope_line;
use bridgevm_storage::VmRuntimeState;
use std::io::Write;
use std::os::unix::net::UnixListener;
use std::process::Command;
use std::thread;
use std::time::Duration;

#[test]
fn reconcile_children_records_agent_update_notice_as_runtime_metadata() {
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
                    name: "agent-update".to_string(),
                    version: 1,
                },
            ],
            auth: Some(AgentAuth::ToolsToken { token }),
        });
        stream
            .write_all(encode_envelope_line(&hello).unwrap().as_bytes())
            .unwrap();
        stream
            .write_all(
                encode_envelope_line(&AgentEnvelope::new(AgentMessage::AgentUpdateAvailable {
                    current_version: "1.0.0".to_string(),
                    available_version: "1.1.0".to_string(),
                    download_url: Some("https://updates.example/bridgevm-tools".to_string()),
                    signature: Some("signature-bytes".to_string()),
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

    assert!(
        wait_up_to_ten_seconds(|| {
            state.reconcile_children().unwrap();
            store
                .guest_tools_runtime_metadata("legacy")
                .unwrap()
                .and_then(|metadata| metadata.agent_update)
                .is_some()
        }),
        "agent update metadata was never reconciled"
    );

    let backend = state.children.get("legacy").unwrap();
    assert_eq!(backend.guest_tools_commands.pending_count(), 0);
    let runtime = store
        .guest_tools_runtime_metadata("legacy")
        .unwrap()
        .expect("runtime metadata");
    assert!(runtime.connected);
    assert!(runtime
        .capabilities
        .iter()
        .any(|name| name == "agent-update"));
    let update = runtime.agent_update.expect("agent update metadata");
    assert_eq!(update.current_version, "1.0.0");
    assert_eq!(update.available_version, "1.1.0");
    assert_eq!(
        update.download_url.as_deref(),
        Some("https://updates.example/bridgevm-tools")
    );
    assert_eq!(update.signature.as_deref(), Some("signature-bytes"));
    assert!(update.observed_at_unix > 0);

    state.cleanup_owned_backend("legacy", false).unwrap();
    server.join().unwrap();
}
