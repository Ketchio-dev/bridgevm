//! A real socket peer driven in protocol order, without server-thread timing.

use super::helpers::{compatibility_manifest, temp_store, TestStoreRoot};
use crate::{DaemonState, SupervisedBackend};
use bridgevm_agent_protocol::{
    AgentAuth, AgentCapability, AgentEnvelope, AgentMessage, PROTOCOL_VERSION,
};
use bridgevm_agentd::{encode_envelope_line, read_envelope_line};
use bridgevm_config::SharedFolder;
use bridgevm_storage::VmRuntimeState;
use std::io::{BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::process::{Command, Stdio};
use std::time::Duration;

pub(super) struct GuestCommandFixture {
    pub(super) state: DaemonState,
    peer: Option<BufReader<UnixStream>>,
    _root: TestStoreRoot,
}

impl GuestCommandFixture {
    pub(super) fn new() -> Self {
        let (root, store) = temp_store();
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
        let metadata = store.guest_tools_runner_metadata("legacy").unwrap();
        let listener = UnixListener::bind(&metadata.socket_path).unwrap();
        listener.set_nonblocking(true).unwrap();
        let mut fixture = Self {
            state: DaemonState::new(store),
            peer: None,
            _root: root,
        };
        // A directly owned process blocks on its held stdin, not a timed shell child.
        let child = Command::new("/bin/cat")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        fixture
            .state
            .children
            .insert("legacy".to_string(), SupervisedBackend::new(child));
        fixture.state.reconcile_children().unwrap();
        assert!(fixture.state.children["legacy"]
            .guest_tools_pending
            .is_some());
        assert!(fixture.state.children["legacy"].guest_tools.is_none());
        let (stream, _) = listener.accept().unwrap();
        stream.set_nonblocking(false).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        fixture.peer = Some(BufReader::new(stream));
        fixture.write(&AgentEnvelope::new(AgentMessage::GuestHello {
            version: PROTOCOL_VERSION,
            guest_os: "linux".to_string(),
            agent_version: Some("1.0.0".to_string()),
            capabilities: ["heartbeat", "clipboard", "shared-folders"]
                .into_iter()
                .map(|name| AgentCapability {
                    name: name.to_string(),
                    version: 1,
                })
                .collect(),
            auth: Some(AgentAuth::ToolsToken { token }),
        }));
        fixture.state.reconcile_children().unwrap();
        assert!(fixture.state.children["legacy"].guest_tools.is_some());
        fixture
    }

    pub(super) fn write(&mut self, envelope: &AgentEnvelope) {
        self.peer
            .as_mut()
            .unwrap()
            .get_mut()
            .write_all(encode_envelope_line(envelope).unwrap().as_bytes())
            .unwrap();
    }

    pub(super) fn read(&mut self) -> AgentEnvelope {
        read_envelope_line(self.peer.as_mut().unwrap())
            .unwrap()
            .expect("command frame")
    }

    pub(super) fn finish(mut self) {
        self.state.cleanup_owned_backend("legacy", false).unwrap();
        assert!(!self.state.children.contains_key("legacy"));
    }
}

impl Drop for GuestCommandFixture {
    fn drop(&mut self) {
        // Failure cleanup is not evidence for the explicit production-cleanup assertion.
        self.peer.take();
        for (_, mut backend) in self.state.children.drain() {
            if let Err(error) = backend.child.kill() {
                eprintln!("guest-command fixture kill failed: {error}");
            }
            if let Err(error) = backend.child.wait() {
                eprintln!("guest-command fixture reap failed: {error}");
            }
        }
    }
}
