//! Guest-tools runtime metadata updates and their persistence to the store.

use crate::*;
use anyhow::Context;
use anyhow::Result;
use bridgevm_agentd::AgentSession;
use bridgevm_storage::GuestToolsAgentUpdateMetadata;
use bridgevm_storage::GuestToolsClipboardMetadata;
use bridgevm_storage::GuestToolsCommandResultMetadata;
use bridgevm_storage::GuestToolsIpAddressMetadata;
use bridgevm_storage::GuestToolsMetricsMetadata;
use bridgevm_storage::GuestToolsRuntimeMetadata;
use bridgevm_storage::GuestToolsSharedFolderMetadata;
use bridgevm_storage::VmStore;

pub(crate) enum GuestToolsRuntimeUpdate {
    Connected,
    Heartbeat,
    GuestIp(Vec<GuestToolsIpAddressMetadata>),
    MountShare {
        name: String,
        host_path_token: String,
    },
    UnmountShare {
        name: String,
    },
    Metrics {
        cpu_percent: u8,
        memory_used_mib: u64,
    },
    CommandResult {
        request_id: String,
        capability: Option<String>,
        ok: bool,
        error_code: Option<String>,
        message: Option<String>,
        result: Option<serde_json::Value>,
        metadata: Option<serde_json::Value>,
        completed_at_unix: u64,
    },
    AgentUpdateAvailable {
        current_version: String,
        available_version: String,
        download_url: Option<String>,
        signature: Option<String>,
    },
    Clipboard {
        text: String,
    },
}

pub(crate) fn write_guest_tools_runtime(
    store: &VmStore,
    name: &str,
    session: &AgentSession,
    update: GuestToolsRuntimeUpdate,
) -> Result<()> {
    let now = now_unix();
    let mut metadata = store
        .guest_tools_runtime_metadata(name)
        .context("failed to read guest tools runtime metadata")?
        .unwrap_or_else(|| GuestToolsRuntimeMetadata {
            connected: true,
            guest_os: Some(session.guest_os.clone()),
            agent_version: session.agent_version.clone(),
            capabilities: session
                .capabilities
                .iter()
                .map(|capability| capability.name.clone())
                .collect(),
            last_heartbeat_at_unix: None,
            guest_ip_addresses: Vec::new(),
            shared_folders: Vec::new(),
            metrics: None,
            last_command_result: None,
            agent_update: None,
            clipboard: None,
            updated_at_unix: now,
        });

    metadata.connected = true;
    metadata.guest_os = Some(session.guest_os.clone());
    metadata.agent_version = session.agent_version.clone();
    metadata.capabilities = session
        .capabilities
        .iter()
        .map(|capability| capability.name.clone())
        .collect();
    metadata.updated_at_unix = now;

    match update {
        GuestToolsRuntimeUpdate::Connected => {}
        GuestToolsRuntimeUpdate::Heartbeat => metadata.last_heartbeat_at_unix = Some(now),
        GuestToolsRuntimeUpdate::GuestIp(addresses) => metadata.guest_ip_addresses = addresses,
        GuestToolsRuntimeUpdate::MountShare {
            name,
            host_path_token,
        } => {
            if let Some(folder) = metadata
                .shared_folders
                .iter_mut()
                .find(|folder| folder.name == name)
            {
                folder.host_path_token = host_path_token;
                folder.mounted_at_unix = now;
            } else {
                metadata
                    .shared_folders
                    .push(GuestToolsSharedFolderMetadata {
                        name,
                        host_path_token,
                        mounted_at_unix: now,
                    });
            }
        }
        GuestToolsRuntimeUpdate::UnmountShare { name } => {
            metadata.shared_folders.retain(|folder| folder.name != name);
        }
        GuestToolsRuntimeUpdate::Metrics {
            cpu_percent,
            memory_used_mib,
        } => {
            metadata.metrics = Some(GuestToolsMetricsMetadata {
                cpu_percent,
                memory_used_mib,
                updated_at_unix: now,
            });
        }
        GuestToolsRuntimeUpdate::CommandResult {
            request_id,
            capability,
            ok,
            error_code,
            message,
            result,
            metadata: command_metadata,
            completed_at_unix,
        } => {
            metadata.last_command_result = Some(GuestToolsCommandResultMetadata {
                request_id,
                capability,
                ok,
                error_code,
                message,
                result,
                metadata: command_metadata,
                completed_at_unix,
            });
        }
        GuestToolsRuntimeUpdate::AgentUpdateAvailable {
            current_version,
            available_version,
            download_url,
            signature,
        } => {
            metadata.agent_update = Some(GuestToolsAgentUpdateMetadata {
                current_version,
                available_version,
                download_url,
                signature,
                observed_at_unix: now,
            });
        }
        GuestToolsRuntimeUpdate::Clipboard { text } => {
            metadata.clipboard = Some(GuestToolsClipboardMetadata {
                text,
                updated_at_unix: now,
            });
        }
    }

    store
        .write_guest_tools_runtime_metadata(name, &metadata)
        .context("failed to write guest tools runtime metadata")
}
