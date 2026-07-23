//! Split test module.

use super::helpers::*;
use crate::*;
use bridgevm_agent_protocol::AgentAuth;
use bridgevm_agent_protocol::AgentCapability;
use bridgevm_agent_protocol::AgentEnvelope;
use bridgevm_agent_protocol::AgentMessage;
use bridgevm_agent_protocol::GuestIpAddress;
use bridgevm_agent_protocol::PROTOCOL_VERSION;
use bridgevm_agentd::encode_envelope_line;
use bridgevm_qemu::qmp_socket_path;
use bridgevm_storage::RunnerMetadata;
use bridgevm_storage::VmRuntimeState;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Write;
use std::os::unix::net::UnixListener;
use std::process::Command;
use std::thread;
use std::time::Duration;

#[test]
fn reconcile_children_records_qmp_drain_limit_metadata() {
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

        for seq in 0..QMP_SUPERVISOR_DRAIN_LIMIT {
            writeln!(stream, r#"{{"event":"RESUME","data":{{"seq":{seq}}}}}"#).unwrap();
        }
        thread::sleep(Duration::from_millis(100));
    });

    let child = Command::new("sh").arg("-c").arg("sleep 5").spawn().unwrap();
    let mut state = DaemonState::new(store.clone());
    state
        .children
        .insert("legacy".to_string(), SupervisedBackend::new(child));

    state.reconcile_children().unwrap();

    assert!(state.children.contains_key("legacy"));
    assert_eq!(
        store.state("legacy").unwrap().state,
        VmRuntimeState::Running
    );
    let qmp = store
        .qmp_supervisor_metadata("legacy")
        .unwrap()
        .expect("qmp supervisor metadata");
    assert_eq!(qmp.envelopes_read, QMP_SUPERVISOR_DRAIN_LIMIT);
    assert_eq!(qmp.events.len(), QMP_SUPERVISOR_DRAIN_LIMIT);
    assert_eq!(qmp.events.first().unwrap().name, "RESUME");
    assert_eq!(
        qmp.events
            .first()
            .unwrap()
            .data
            .as_ref()
            .and_then(|data| data.get("seq"))
            .and_then(|seq| seq.as_u64()),
        Some(0)
    );
    assert_eq!(
        qmp.events
            .last()
            .unwrap()
            .data
            .as_ref()
            .and_then(|data| data.get("seq"))
            .and_then(|seq| seq.as_u64()),
        Some((QMP_SUPERVISOR_DRAIN_LIMIT - 1) as u64)
    );
    assert!(qmp.terminal_event.is_none());
    assert!(qmp.limit_reached);

    state.cleanup_owned_backend("legacy", false).unwrap();
    server.join().unwrap();
}

#[test]
fn reconcile_children_bootstraps_guest_tools_session() {
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
        let envelope = AgentEnvelope::new(AgentMessage::GuestHello {
            version: PROTOCOL_VERSION,
            guest_os: "linux".to_string(),
            agent_version: Some("1.0.0".to_string()),
            capabilities: vec![
                AgentCapability {
                    name: "heartbeat".to_string(),
                    version: 1,
                },
                AgentCapability {
                    name: "guest-ip".to_string(),
                    version: 1,
                },
                AgentCapability {
                    name: "guest-metrics".to_string(),
                    version: 1,
                },
                AgentCapability {
                    name: "clipboard".to_string(),
                    version: 1,
                },
            ],
            auth: Some(AgentAuth::ToolsToken { token }),
        });
        let line = encode_envelope_line(&envelope).unwrap();
        stream.write_all(line.as_bytes()).unwrap();
        stream
            .write_all(
                encode_envelope_line(&AgentEnvelope::new(AgentMessage::Heartbeat))
                    .unwrap()
                    .as_bytes(),
            )
            .unwrap();
        stream
            .write_all(
                encode_envelope_line(&AgentEnvelope::new(AgentMessage::GuestIpChanged {
                    addresses: vec![GuestIpAddress {
                        address: "10.0.2.15".parse().unwrap(),
                        interface: Some("eth0".to_string()),
                    }],
                }))
                .unwrap()
                .as_bytes(),
            )
            .unwrap();
        stream
            .write_all(
                encode_envelope_line(&AgentEnvelope::new(AgentMessage::ClipboardChanged {
                    text: "first guest value".to_string(),
                }))
                .unwrap()
                .as_bytes(),
            )
            .unwrap();
        stream
            .write_all(
                encode_envelope_line(&AgentEnvelope::new(AgentMessage::ClipboardChanged {
                    text: "latest guest value".to_string(),
                }))
                .unwrap()
                .as_bytes(),
            )
            .unwrap();
        stream
            .write_all(
                encode_envelope_line(&AgentEnvelope::new(AgentMessage::GuestMetrics {
                    cpu_percent: 17,
                    memory_used_mib: 512,
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

    let backend = state.children.get("legacy").unwrap();
    let session = backend.guest_tools.as_ref().expect("guest tools session");
    assert_eq!(session.guest_os, "linux");
    assert_eq!(session.agent_version.as_deref(), Some("1.0.0"));
    assert!(session.supports("heartbeat"));
    assert!(session.supports("guest-ip"));
    assert!(session.supports("guest-metrics"));
    assert!(session.supports("clipboard"));

    let runtime = store
        .guest_tools_runtime_metadata("legacy")
        .unwrap()
        .expect("runtime metadata");
    assert!(runtime.connected);
    assert_eq!(runtime.guest_os.as_deref(), Some("linux"));
    assert!(runtime.last_heartbeat_at_unix.is_some());
    assert_eq!(runtime.guest_ip_addresses.len(), 1);
    assert_eq!(runtime.guest_ip_addresses[0].address, "10.0.2.15");
    assert_eq!(
        runtime.guest_ip_addresses[0].interface.as_deref(),
        Some("eth0")
    );
    let metrics = runtime.metrics.expect("guest metrics");
    assert_eq!(metrics.cpu_percent, 17);
    assert_eq!(metrics.memory_used_mib, 512);
    let clipboard = runtime.clipboard.expect("clipboard metadata");
    assert_eq!(clipboard.text, "latest guest value");
    assert!(clipboard.updated_at_unix > 0);

    state.cleanup_owned_backend("legacy", false).unwrap();
    server.join().unwrap();
}

#[test]
fn reconcile_holds_connection_and_catches_delayed_guest_hello() {
    // Regression guard for the live application-consistent path: the guest
    // agent emits its GuestHello exactly once, as the first frame, a beat
    // after the host connects. The daemon must connect host-first and HOLD
    // that connection across reconcile ticks so it catches the delayed
    // hello, instead of reconnecting each tick and racing past it.
    let store = temp_store();
    store.create_vm(&compatibility_manifest("legacy")).unwrap();
    store
        .transition_state("legacy", VmRuntimeState::Running)
        .unwrap();

    let token = store.guest_tools_token("legacy").unwrap().token;
    let guest_tools = store.guest_tools_runner_metadata("legacy").unwrap();
    let listener = UnixListener::bind(&guest_tools.socket_path).unwrap();

    let (send_hello, await_hello) = std::sync::mpsc::channel::<()>();
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
        thread::sleep(Duration::from_millis(250));
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
    thread::sleep(Duration::from_millis(50));

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
    server.join().unwrap();
}

#[test]
fn reconcile_reassembles_a_guest_hello_split_across_reads() {
    // The agent's one-shot GuestHello can arrive split across host reads
    // (virtio-serial chunks it), with a gap longer than the socket read
    // timeout. The held connection must NOT consume + lose the partial frame
    // when the timeout fires mid-frame -- it must reassemble and accept once
    // the whole line is present. (Guards the peek-before-consume fix.)
    let store = temp_store();
    store.create_vm(&compatibility_manifest("legacy")).unwrap();
    store
        .transition_state("legacy", VmRuntimeState::Running)
        .unwrap();

    let token = store.guest_tools_token("legacy").unwrap().token;
    let guest_tools = store.guest_tools_runner_metadata("legacy").unwrap();
    let listener = UnixListener::bind(&guest_tools.socket_path).unwrap();

    let (start_send, await_send) = std::sync::mpsc::channel::<()>();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        await_send.recv().unwrap();
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
        let line = encode_envelope_line(&hello).unwrap();
        let bytes = line.as_bytes();
        let mid = bytes.len() / 2;
        // First half, then a pause LONGER than the 25ms read timeout (so a
        // naive read would time out mid-frame), then the rest.
        stream.write_all(&bytes[..mid]).unwrap();
        stream.flush().unwrap();
        thread::sleep(Duration::from_millis(120));
        stream.write_all(&bytes[mid..]).unwrap();
        stream.flush().unwrap();
        thread::sleep(Duration::from_millis(250));
    });

    let child = Command::new("sh").arg("-c").arg("sleep 5").spawn().unwrap();
    let mut state = DaemonState::new(store.clone());
    state
        .children
        .insert("legacy".to_string(), SupervisedBackend::new(child));

    // First reconcile: connect host-first, nothing to read yet -> held.
    state.reconcile_children().unwrap();
    assert!(state
        .children
        .get("legacy")
        .unwrap()
        .guest_tools_pending
        .is_some());

    start_send.send(()).unwrap();

    // Poll reconcile until the split hello is reassembled + accepted. While
    // only the first half is buffered, the connection must stay held (the
    // partial frame must never be consumed/lost or the connection reset).
    let mut accepted = false;
    for _ in 0..40 {
        thread::sleep(Duration::from_millis(20));
        state.reconcile_children().unwrap();
        let backend = state.children.get("legacy").unwrap();
        if backend.guest_tools.is_some() {
            accepted = true;
            break;
        }
        assert!(
            backend.guest_tools_pending.is_some(),
            "the connection must be held while the frame is incomplete"
        );
    }
    assert!(
        accepted,
        "split GuestHello was never reassembled + accepted"
    );
    assert_eq!(
        state
            .children
            .get("legacy")
            .unwrap()
            .guest_tools
            .as_ref()
            .unwrap()
            .guest_os,
        "linux"
    );

    state.cleanup_owned_backend("legacy", false).unwrap();
    server.join().unwrap();
}

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

    state.reconcile_children().unwrap();

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
