//! Split out of lib.rs by responsibility.

use crate::*;

pub fn inspect_guest_tools_status(
    store: &VmStore,
    name: &str,
) -> Result<GuestToolsStatusRecord, String> {
    let (_, manifest) = store.get_vm(name).map_err(|error| error.to_string())?;
    let token = store
        .guest_tools_token(name)
        .map_err(|error| error.to_string())?;
    Ok(GuestToolsStatusRecord {
        vm: name.to_string(),
        tools: manifest.integration.tools.clone(),
        token_created_at_unix: token.created_at_unix,
        capabilities: guest_tools_capabilities(&manifest),
        approved_shared_folders: guest_tools_approved_shared_folders(&manifest),
        runtime: store
            .guest_tools_runtime_metadata(name)
            .map_err(|error| error.to_string())?,
    })
}

pub fn snapshot_preflight_status(
    store: &VmStore,
    name: &str,
    consistency: SnapshotConsistency,
) -> Result<SnapshotPreflightStatusRecord, String> {
    store.get_vm(name).map_err(|error| error.to_string())?;
    let runtime = store
        .guest_tools_runtime_metadata(name)
        .map_err(|error| error.to_string())?;
    let capabilities = runtime
        .as_ref()
        .map(|runtime| runtime.capabilities.clone())
        .unwrap_or_default();
    let guest_tools_connected = runtime.as_ref().is_some_and(|runtime| runtime.connected);
    let mut blockers = Vec::new();
    // This is the offline / metadata-only preflight: freeze/thaw can only be
    // driven by the bridgevmd-owned running backend that holds the live
    // guest-tools session. The daemon overrides this to `true` in
    // `owned_backend_snapshot_preflight_status` once it owns the backend.
    let backend_freeze_thaw_supported = false;

    if !backend_freeze_thaw_supported && consistency == SnapshotConsistency::ApplicationConsistent {
        blockers.push(SnapshotPreflightBlockerRecord {
            code: "backend-freeze-thaw-unavailable".to_string(),
            message: "Freeze/thaw orchestration requires the bridgevmd-owned running backend; this offline preflight cannot drive the guest agent."
                .to_string(),
            path: None,
        });
    }

    if consistency == SnapshotConsistency::ApplicationConsistent && !guest_tools_connected {
        blockers.push(SnapshotPreflightBlockerRecord {
            code: "guest-tools-not-connected".to_string(),
            message:
                "Guest tools must be connected before application-consistent preflight can pass."
                    .to_string(),
            path: None,
        });
    }

    for capability in application_consistent_snapshot_required_capabilities(consistency) {
        if !capabilities
            .iter()
            .any(|available| available == &capability)
        {
            blockers.push(SnapshotPreflightBlockerRecord {
                code: "missing-capability".to_string(),
                message: format!(
                    "Guest tools did not advertise required capability '{capability}'."
                ),
                path: None,
            });
        }
    }

    Ok(SnapshotPreflightStatusRecord {
        vm: name.to_string(),
        consistency,
        backend_freeze_thaw_supported,
        guest_tools_connected,
        capabilities,
        ready: blockers.is_empty(),
        blockers,
        checked_at_unix: now_unix(),
    })
}

pub(crate) fn application_consistent_snapshot_required_capabilities(
    consistency: SnapshotConsistency,
) -> Vec<String> {
    match consistency {
        SnapshotConsistency::CrashConsistent => Vec::new(),
        SnapshotConsistency::ApplicationConsistent => {
            vec!["fs-freeze".to_string(), "fs-thaw".to_string()]
        }
    }
}

pub fn guest_tools_mount_approved_share_envelope(
    store: &VmStore,
    name: &str,
    share: &str,
    request_id: Option<String>,
) -> Result<AgentEnvelope, String> {
    let (_, manifest) = store.get_vm(name).map_err(|error| error.to_string())?;
    guest_tools_mount_approved_share_envelope_from_manifest(&manifest, share, request_id)
}

pub fn guest_tools_freeze_filesystem_envelope(
    request_id: impl Into<String>,
    timeout_millis: Option<u64>,
) -> AgentEnvelope {
    AgentEnvelope::with_request_id(
        AgentMessage::FreezeFilesystem { timeout_millis },
        request_id,
    )
}

pub fn guest_tools_thaw_filesystem_envelope(request_id: impl Into<String>) -> AgentEnvelope {
    AgentEnvelope::with_request_id(AgentMessage::ThawFilesystem, request_id)
}

pub fn guest_tools_mount_approved_share_envelope_from_manifest(
    manifest: &VmManifest,
    share: &str,
    request_id: Option<String>,
) -> Result<AgentEnvelope, String> {
    if !manifest.integration.shared_folders {
        return Err("manifest.integration.sharedFolders is disabled".to_string());
    }

    let folder = manifest
        .shared_folders
        .iter()
        .find(|folder| folder.name == share)
        .ok_or_else(|| format!("approved shared folder '{share}' was not found"))?;
    let message = AgentMessage::MountShare {
        name: folder.name.clone(),
        host_path_token: folder.resolved_host_path_token(),
    };
    Ok(match request_id {
        Some(request_id) => AgentEnvelope::with_request_id(message, request_id),
        None => AgentEnvelope::new(message),
    })
}

pub fn guest_tools_token(store: &VmStore, name: &str) -> Result<GuestToolsTokenRecord, String> {
    let token = store
        .guest_tools_token(name)
        .map_err(|error| error.to_string())?;
    Ok(GuestToolsTokenRecord {
        vm: name.to_string(),
        token: token.token,
        created_at_unix: token.created_at_unix,
    })
}

pub fn guest_tools_linux_command(
    store: &VmStore,
    name: &str,
    transport: GuestToolsLinuxCommandTransport,
    token_file: Option<PathBuf>,
    device: Option<PathBuf>,
) -> Result<GuestToolsLinuxCommandRecord, String> {
    let status = inspect_guest_tools_status(store, name)?;
    let runner = store
        .guest_tools_runner_metadata(name)
        .map_err(|error| error.to_string())?;
    let token_file = token_file.unwrap_or(runner.token_path);
    let capabilities: Vec<String> = status
        .capabilities
        .iter()
        .map(|capability| format!("{}:{}", capability.name, capability.max_version))
        .collect();

    let mut command = vec!["bridgevm-tools-linux".to_string()];
    match transport {
        GuestToolsLinuxCommandTransport::Socket => {
            command.push("--socket".to_string());
            command.push(runner.socket_path.display().to_string());
        }
        GuestToolsLinuxCommandTransport::Device => {
            command.push("--device".to_string());
            command.push(
                device
                    .unwrap_or_else(|| PathBuf::from(DEFAULT_GUEST_TOOLS_LINUX_DEVICE))
                    .display()
                    .to_string(),
            );
        }
    }
    command.push("--token-file".to_string());
    command.push(token_file.display().to_string());
    for capability in &capabilities {
        command.push("--capability".to_string());
        command.push(capability.clone());
    }

    Ok(GuestToolsLinuxCommandRecord {
        vm: name.to_string(),
        transport,
        command,
        token_file,
        capabilities,
    })
}

pub fn accept_guest_tools_hello(
    store: &VmStore,
    name: &str,
    envelope: &AgentEnvelope,
) -> Result<GuestToolsSessionRecord, String> {
    let policy = guest_tools_agent_policy(store, name)?;
    let session = accept_guest_hello(envelope, &policy).map_err(|error| format!("{error:?}"))?;

    Ok(GuestToolsSessionRecord {
        vm: name.to_string(),
        guest_os: session.guest_os,
        agent_version: session.agent_version,
        capabilities: session.capabilities,
    })
}

pub fn guest_tools_agent_policy(store: &VmStore, name: &str) -> Result<AgentPolicy, String> {
    let status = inspect_guest_tools_status(store, name)?;
    let token = store
        .guest_tools_token(name)
        .map_err(|error| error.to_string())?;
    Ok(AgentPolicy::new(
        token.token,
        status
            .capabilities
            .iter()
            .map(|capability| (capability.name.as_str(), capability.max_version)),
    ))
}

pub(crate) fn guest_tools_capabilities(manifest: &VmManifest) -> Vec<GuestToolsCapabilityRecord> {
    let mut capabilities = vec![
        guest_tools_capability("heartbeat", "base protocol"),
        guest_tools_capability("guest-ip", "network reporting"),
        guest_tools_capability("time-sync", "clock sync"),
        guest_tools_capability("guest-metrics", "diagnostics"),
        guest_tools_capability("benchmark", "performance sampling"),
        guest_tools_capability("fs-freeze", "application-consistent snapshot scaffold"),
        guest_tools_capability("fs-thaw", "application-consistent snapshot scaffold"),
    ];
    if manifest.integration.clipboard {
        capabilities.push(guest_tools_capability(
            "clipboard",
            "manifest.integration.clipboard",
        ));
    }
    if manifest.integration.dynamic_resolution {
        capabilities.push(guest_tools_capability(
            "display-resize",
            "manifest.integration.dynamicResolution",
        ));
    }
    if manifest.integration.shared_folders {
        capabilities.push(guest_tools_capability(
            "shared-folders",
            "manifest.integration.sharedFolders",
        ));
    }
    if manifest.integration.drag_drop {
        capabilities.push(guest_tools_capability(
            "drag-drop",
            "manifest.integration.dragDrop",
        ));
    }
    if manifest.integration.applications {
        capabilities.push(guest_tools_capability(
            "applications",
            "manifest.integration.applications",
        ));
    }
    if manifest.integration.windows {
        capabilities.push(guest_tools_capability(
            "windows",
            "manifest.integration.windows",
        ));
    }
    if manifest.security.signed_agent_updates {
        capabilities.push(guest_tools_capability(
            "agent-update",
            "manifest.security.signedAgentUpdates",
        ));
    }
    capabilities
}

pub(crate) fn guest_tools_capability(name: &str, enabled_by: &str) -> GuestToolsCapabilityRecord {
    GuestToolsCapabilityRecord {
        name: name.to_string(),
        max_version: 1,
        enabled_by: enabled_by.to_string(),
    }
}

pub(crate) fn guest_tools_approved_shared_folders(
    manifest: &VmManifest,
) -> Vec<GuestToolsApprovedSharedFolderRecord> {
    if !manifest.integration.shared_folders {
        return Vec::new();
    }

    manifest
        .shared_folders
        .iter()
        .map(|folder| GuestToolsApprovedSharedFolderRecord {
            name: folder.name.clone(),
            host_path: folder.host_path.clone(),
            host_path_token: folder.resolved_host_path_token(),
            read_only: folder.read_only,
            approval: manifest.security.shared_folder_approval.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approved_share_mount_resolves_manifest_token_to_agent_envelope() {
        let mut manifest = VmManifest::new(
            "dev",
            VmMode::Fast,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "arm64".to_string(),
            },
            "80GiB",
        );
        manifest.shared_folders = vec![SharedFolder {
            name: "workspace".to_string(),
            host_path: "/Users/me/project".to_string(),
            read_only: false,
            host_path_token: Some("share-token-workspace".to_string()),
        }];

        let envelope = guest_tools_mount_approved_share_envelope_from_manifest(
            &manifest,
            "workspace",
            Some("mount-1".to_string()),
        )
        .unwrap();

        assert_eq!(envelope.request_id, Some("mount-1".to_string()));
        assert_eq!(
            envelope.message,
            AgentMessage::MountShare {
                name: "workspace".to_string(),
                host_path_token: "share-token-workspace".to_string(),
            }
        );
    }

    #[test]
    fn approved_share_mount_rejects_missing_or_disabled_share() {
        let mut manifest = VmManifest::new(
            "dev",
            VmMode::Fast,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "arm64".to_string(),
            },
            "80GiB",
        );
        manifest.shared_folders = vec![SharedFolder {
            name: "workspace".to_string(),
            host_path: "/Users/me/project".to_string(),
            read_only: false,
            host_path_token: Some("share-token-workspace".to_string()),
        }];

        let missing =
            guest_tools_mount_approved_share_envelope_from_manifest(&manifest, "downloads", None)
                .unwrap_err();
        assert!(missing.contains("approved shared folder 'downloads' was not found"));

        manifest.integration.shared_folders = false;
        let disabled =
            guest_tools_mount_approved_share_envelope_from_manifest(&manifest, "workspace", None)
                .unwrap_err();
        assert_eq!(disabled, "manifest.integration.sharedFolders is disabled");
    }
}
