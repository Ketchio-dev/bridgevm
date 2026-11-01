//! Delayed guest hello on the daemon's original held connection.

use super::helpers::*;
use crate::*;
use bridgevm_agent_protocol::{
    AgentAuth, AgentCapability, AgentEnvelope, AgentMessage, PROTOCOL_VERSION,
};
use bridgevm_agentd::encode_envelope_line;
use bridgevm_storage::VmRuntimeState;
use std::io::Write;
use std::os::unix::net::UnixListener;
use std::process::Command;
use std::thread;
use std::time::Duration;

#[test]
fn reconcile_holds_connection_and_catches_delayed_guest_hello() {
    // Regression guard for the live application-consistent path: the guest
    // agent emits its GuestHello exactly once, as the first frame, a beat
    // after the host connects. The daemon must connect host-first and HOLD
    // that connection across reconcile ticks so it catches the delayed
    // hello, instead of reconnecting each tick and racing past it.
    let (_root, store) = temp_store();
    store.create_vm(&compatibility_manifest("legacy")).unwrap();
    store
        .transition_state("legacy", VmRuntimeState::Running)
        .unwrap();

    let token = store.guest_tools_token("legacy").unwrap().token;
    let guest_tools = store.guest_tools_runner_metadata("legacy").unwrap();
    let listener = UnixListener::bind(&guest_tools.socket_path).unwrap();

    let (send_hello, await_hello) = std::sync::mpsc::channel::<()>();
    let (hello_written, await_hello_written) = std::sync::mpsc::channel();
    let server = thread::spawn(move || {
        // Accept the daemon's host-first connection, then withhold the hello
        // until the test has run the first (pending, no-data) reconcile —
        // emulating the guest agent coming up a moment after the host
        // attaches. The hello must land on this SAME held connection.
        let (mut stream, _) = listener.accept().unwrap();
        await_hello.recv().unwrap();
        let hello = AgentEnvelope::new(AgentMessage::GuestHello {
            version: PROTOCOL_VERSION,
            guest_os: "linux".to_string(),
            agent_version: Some("1.0.0".to_string()),
            capabilities: vec![AgentCapability {
                name: "heartbeat".to_string(),
                version: 1,
            }],
            auth: Some(AgentAuth::ToolsToken { token }),
        });
        stream
            .write_all(encode_envelope_line(&hello).unwrap().as_bytes())
            .unwrap();
        hello_written.send(stream).unwrap();
    });

    let child = Command::new("sh").arg("-c").arg("sleep 5").spawn().unwrap();
    let mut state = DaemonState::new(store.clone());
    state
        .children
        .insert("legacy".to_string(), SupervisedBackend::new(child));

    // First reconcile: connects host-first, reads no hello yet -> the
    // connection is HELD (pending), no session accepted, no reset.
    state.reconcile_children().unwrap();
    {
        let backend = state.children.get("legacy").unwrap();
        assert!(
            backend.guest_tools.is_none(),
            "no session should be accepted before the hello arrives"
        );
        assert!(
            backend.guest_tools_pending.is_some(),
            "the host-first connection must be held while waiting for the hello"
        );
    }

    // The agent now writes its one-shot hello on the SAME held connection.
    send_hello.send(()).unwrap();
    // Acknowledge the write and keep its peer open until the assertions finish.
    let server_stream = await_hello_written
        .recv_timeout(Duration::from_secs(5))
        .expect("guest hello written on the held connection");

    // Second reconcile: reads the delayed hello on the held connection and
    // accepts the session (proving the connection was not dropped/reconnected).
    state.reconcile_children().unwrap();
    {
        let backend = state.children.get("legacy").unwrap();
        let session = backend
            .guest_tools
            .as_ref()
            .expect("session accepted from the delayed hello on the held connection");
        assert_eq!(session.guest_os, "linux");
        assert!(session.supports("heartbeat"));
        assert!(
            backend.guest_tools_pending.is_none(),
            "the held connection should move to the active stream once accepted"
        );
    }
    let runtime = store
        .guest_tools_runtime_metadata("legacy")
        .unwrap()
        .expect("runtime metadata");
    assert!(runtime.connected);

    state.cleanup_owned_backend("legacy", false).unwrap();
    drop(server_stream);
    server.join().unwrap();
}
