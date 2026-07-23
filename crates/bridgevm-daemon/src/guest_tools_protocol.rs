//! Interpreting inbound agent envelopes and the completed-command result record.

use crate::*;
use anyhow::Result;
use bridgevm_agent_protocol::AgentEnvelope;
use bridgevm_agent_protocol::AgentMessage;
use bridgevm_agentd::authorize_message;
use bridgevm_agentd::AgentSession;
use bridgevm_api::ApplicationConsistentSnapshotCommandResultRecord;
use bridgevm_storage::GuestToolsIpAddressMetadata;
use bridgevm_storage::VmStore;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CompletedGuestToolsCommand {
    pub(crate) request_id: String,
    pub(crate) capability: Option<String>,
    pub(crate) ok: bool,
    pub(crate) error_code: Option<String>,
    pub(crate) message: Option<String>,
    pub(crate) result: Option<serde_json::Value>,
    pub(crate) metadata: Option<serde_json::Value>,
    pub(crate) completed_at_unix: u64,
    pub(crate) pending_commands: usize,
}

pub(crate) fn process_guest_tools_envelope(
    store: &VmStore,
    name: &str,
    backend: &mut SupervisedBackend,
    session: &AgentSession,
    envelope: AgentEnvelope,
) -> Result<Option<CompletedGuestToolsCommand>> {
    authorize_message(session, &envelope.message)
        .map_err(|error| anyhow::anyhow!("unauthorized guest tools message: {error:?}"))?;
    match &envelope.message {
        AgentMessage::CommandResult {
            request_id,
            ok,
            error_code,
            message,
            result,
            metadata,
        } => {
            let pending = backend
                .guest_tools_commands
                .complete_command_result(&envelope)
                .map_err(|error| {
                    anyhow::anyhow!("unexpected guest tools command result: {error:?}")
                })?;
            let completed_at_unix = now_unix();
            let mut result = result.clone();
            let metadata = metadata.clone();
            if *ok && pending.capability.as_deref() == Some("windows") {
                if let Err(message) =
                    attach_proxy_window_crop_artifacts(store, name, backend, result.as_mut())
                {
                    eprintln!("bridgevmd: proxy window crop artifact skipped: {message}");
                }
            }
            write_guest_tools_runtime(
                store,
                name,
                session,
                GuestToolsRuntimeUpdate::CommandResult {
                    request_id: request_id.clone(),
                    capability: pending.capability.clone(),
                    ok: *ok,
                    error_code: error_code.clone(),
                    message: message.clone(),
                    result: result.clone(),
                    metadata: metadata.clone(),
                    completed_at_unix,
                },
            )?;
            if *ok {
                match pending.message {
                    AgentMessage::MountShare {
                        name: share_name,
                        host_path_token,
                    } => {
                        write_guest_tools_runtime(
                            store,
                            name,
                            session,
                            GuestToolsRuntimeUpdate::MountShare {
                                name: share_name,
                                host_path_token,
                            },
                        )?;
                    }
                    AgentMessage::UnmountShare { name: share_name } => {
                        write_guest_tools_runtime(
                            store,
                            name,
                            session,
                            GuestToolsRuntimeUpdate::UnmountShare { name: share_name },
                        )?;
                    }
                    _ => {}
                }
            }
            Ok(Some(CompletedGuestToolsCommand {
                request_id: request_id.clone(),
                capability: pending.capability,
                ok: *ok,
                error_code: error_code.clone(),
                message: message.clone(),
                result,
                metadata,
                completed_at_unix,
                pending_commands: backend.guest_tools_commands.pending_count(),
            }))
        }
        AgentMessage::Heartbeat => {
            write_guest_tools_runtime(store, name, session, GuestToolsRuntimeUpdate::Heartbeat)?;
            Ok(None)
        }
        AgentMessage::GuestIpChanged { addresses } => {
            write_guest_tools_runtime(
                store,
                name,
                session,
                GuestToolsRuntimeUpdate::GuestIp(
                    addresses
                        .iter()
                        .map(|address| GuestToolsIpAddressMetadata {
                            address: address.address.to_string(),
                            interface: address.interface.clone(),
                        })
                        .collect(),
                ),
            )?;
            Ok(None)
        }
        AgentMessage::GuestMetrics {
            cpu_percent,
            memory_used_mib,
        } => {
            write_guest_tools_runtime(
                store,
                name,
                session,
                GuestToolsRuntimeUpdate::Metrics {
                    cpu_percent: *cpu_percent,
                    memory_used_mib: *memory_used_mib,
                },
            )?;
            Ok(None)
        }
        AgentMessage::AgentUpdateAvailable {
            current_version,
            available_version,
            download_url,
            signature,
        } => {
            write_guest_tools_runtime(
                store,
                name,
                session,
                GuestToolsRuntimeUpdate::AgentUpdateAvailable {
                    current_version: current_version.clone(),
                    available_version: available_version.clone(),
                    download_url: download_url.clone(),
                    signature: signature.clone(),
                },
            )?;
            Ok(None)
        }
        AgentMessage::ClipboardChanged { text } => {
            write_guest_tools_runtime(
                store,
                name,
                session,
                GuestToolsRuntimeUpdate::Clipboard { text: text.clone() },
            )?;
            Ok(None)
        }
        _ => Ok(None),
    }
}

impl CompletedGuestToolsCommand {
    pub(crate) fn into_record(self) -> ApplicationConsistentSnapshotCommandResultRecord {
        ApplicationConsistentSnapshotCommandResultRecord {
            request_id: self.request_id,
            capability: self.capability,
            ok: self.ok,
            error_code: self.error_code,
            message: self.message,
            completed_at_unix: self.completed_at_unix,
        }
    }
}
