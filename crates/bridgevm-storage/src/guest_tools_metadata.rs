//! Guest-tools token, runner and runtime metadata types.

use serde::Deserialize;
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuestToolsTokenMetadata {
    pub token: String,
    pub created_at_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuestToolsRunnerMetadata {
    pub transport: String,
    pub channel_name: String,
    pub socket_path: PathBuf,
    pub token_path: PathBuf,
    pub token_created_at_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuestToolsRuntimeMetadata {
    pub connected: bool,
    pub guest_os: Option<String>,
    pub agent_version: Option<String>,
    pub capabilities: Vec<String>,
    pub last_heartbeat_at_unix: Option<u64>,
    pub guest_ip_addresses: Vec<GuestToolsIpAddressMetadata>,
    #[serde(default)]
    pub shared_folders: Vec<GuestToolsSharedFolderMetadata>,
    pub metrics: Option<GuestToolsMetricsMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_command_result: Option<GuestToolsCommandResultMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_update: Option<GuestToolsAgentUpdateMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clipboard: Option<GuestToolsClipboardMetadata>,
    pub updated_at_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuestToolsIpAddressMetadata {
    pub address: String,
    pub interface: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuestToolsSharedFolderMetadata {
    pub name: String,
    pub host_path_token: String,
    pub mounted_at_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuestToolsMetricsMetadata {
    pub cpu_percent: u8,
    pub memory_used_mib: u64,
    pub updated_at_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuestToolsCommandResultMetadata {
    pub request_id: String,
    pub capability: Option<String>,
    pub ok: bool,
    pub error_code: Option<String>,
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    pub completed_at_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuestToolsAgentUpdateMetadata {
    pub current_version: String,
    pub available_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub download_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    pub observed_at_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuestToolsClipboardMetadata {
    pub text: String,
    pub updated_at_unix: u64,
}
