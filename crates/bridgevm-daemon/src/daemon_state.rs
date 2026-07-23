//! Ownership records for supervised backends and the request-to-method routing table.

use crate::*;
use bridgevm_agentd::AgentCommandTracker;
use bridgevm_agentd::AgentSession;
use bridgevm_api::guest_tools_mount_approved_share_envelope;
use bridgevm_api::handle_request;
use bridgevm_api::BridgeVmRequest;
use bridgevm_api::BridgeVmResponse;
use bridgevm_qemu::QmpClient;
use bridgevm_storage::VmStore;
use std::collections::HashMap;
use std::io::BufReader;
use std::os::unix::net::UnixStream;
use std::process::Child;

pub(crate) struct DaemonState {
    pub(crate) store: VmStore,
    pub(crate) children: HashMap<String, SupervisedBackend>,
}

pub(crate) struct SupervisedBackend {
    pub(crate) child: Child,
    pub(crate) qmp: Option<QmpClient>,
    pub(crate) guest_tools: Option<AgentSession>,
    pub(crate) guest_tools_stream: Option<BufReader<UnixStream>>,
    /// A guest-tools socket connection established host-first (right after the
    /// backend is spawned, before the guest agent boots) and HELD open across
    /// reconcile ticks. The guest agent writes its `GuestHello` exactly once,
    /// as the first frame, when it comes up ~a minute into boot. Connecting
    /// fresh on each tick races past that one-shot hello (the daemon would read
    /// a later Heartbeat first -> `ExpectedGuestHello`), so instead we connect
    /// once and keep this reader until the hello arrives or the socket dies.
    pub(crate) guest_tools_pending: Option<UnixStream>,
    pub(crate) guest_tools_commands: AgentCommandTracker,
    pub(crate) proxy_window_crop_targets: HashMap<String, ProxyWindowCropTarget>,
    pub(crate) proxy_window_framebuffer_signature: Option<ProxyWindowFramebufferSignature>,
}

impl SupervisedBackend {
    pub(crate) fn new(child: Child) -> Self {
        Self {
            child,
            qmp: None,
            guest_tools: None,
            guest_tools_stream: None,
            guest_tools_pending: None,
            guest_tools_commands: AgentCommandTracker::new(),
            proxy_window_crop_targets: HashMap::new(),
            proxy_window_framebuffer_signature: None,
        }
    }
}

impl DaemonState {
    pub(crate) fn new(store: VmStore) -> Self {
        Self {
            store,
            children: HashMap::new(),
        }
    }

    pub(crate) fn handle_request(&mut self, request: BridgeVmRequest) -> BridgeVmResponse {
        if let Err(error) = self.reconcile_children() {
            return BridgeVmResponse::Error {
                message: error.to_string(),
            };
        }

        match request {
            BridgeVmRequest::RunBackend { name, spawn: true } => self
                .spawn_backend(&name)
                .unwrap_or_else(|error| BridgeVmResponse::Error {
                    message: error.to_string(),
                }),
            BridgeVmRequest::ResumeBackend { name } => self
                .resume_backend_supervised(&name)
                .unwrap_or_else(|error| BridgeVmResponse::Error {
                    message: error.to_string(),
                }),
            BridgeVmRequest::SuspendBackend { name } => self
                .suspend_backend_supervised(&name)
                .unwrap_or_else(|error| BridgeVmResponse::Error {
                    message: error.to_string(),
                }),
            BridgeVmRequest::StopBackend { name } if self.children.contains_key(&name) => self
                .stop_owned_backend(&name)
                .unwrap_or_else(|error| BridgeVmResponse::Error {
                    message: error.to_string(),
                }),
            BridgeVmRequest::RestartVm { name } if self.children.contains_key(&name) => self
                .restart_owned_backend(&name)
                .unwrap_or_else(|error| BridgeVmResponse::Error {
                    message: error.to_string(),
                }),
            BridgeVmRequest::GuestToolsSendCommand { name, envelope }
                if self.children.contains_key(&name) =>
            {
                self.send_guest_tools_command(&name, envelope)
                    .unwrap_or_else(|error| BridgeVmResponse::Error {
                        message: error.to_string(),
                    })
            }
            BridgeVmRequest::GuestToolsMountApprovedShare {
                name,
                share,
                request_id,
            } if self.children.contains_key(&name) => {
                guest_tools_mount_approved_share_envelope(&self.store, &name, &share, request_id)
                    .and_then(|envelope| {
                        self.send_guest_tools_command(&name, envelope)
                            .map_err(|error| error.to_string())
                    })
                    .unwrap_or_else(|message| BridgeVmResponse::Error { message })
            }
            BridgeVmRequest::SnapshotPreflightStatus { name, consistency }
                if self.children.contains_key(&name) =>
            {
                self.owned_backend_snapshot_preflight_status(&name, consistency)
                    .unwrap_or_else(|error| BridgeVmResponse::Error {
                        message: error.to_string(),
                    })
            }
            BridgeVmRequest::ExecuteApplicationConsistentSnapshot {
                vm,
                name,
                freeze_timeout_millis,
            } if self.children.contains_key(&vm) => self
                .execute_application_consistent_snapshot(&vm, &name, freeze_timeout_millis)
                .unwrap_or_else(|error| BridgeVmResponse::Error {
                    message: error.to_string(),
                }),
            BridgeVmRequest::CreatePerformanceSample {
                name,
                output,
                artifact_bytes,
                iterations,
                sync,
            } if self.children.contains_key(&name) => self
                .create_performance_sample_with_optional_guest_benchmark(
                    &name,
                    output,
                    artifact_bytes,
                    iterations,
                    sync,
                )
                .unwrap_or_else(|error| BridgeVmResponse::Error {
                    message: error.to_string(),
                }),
            request => handle_request(&self.store, request),
        }
    }
}
