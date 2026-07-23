//! Split out of is_false.rs to keep files under 600 lines.

use super::*;

use bridgevm_config::ConfigError;
use bridgevm_qemu::QmpEvent;
use serde::Deserialize;
use serde::Serialize;
use std::path::PathBuf;
use thiserror::Error;

pub(crate) fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("VM already exists: {0}")]
    AlreadyExists(String),
    #[error("VM not found: {0}")]
    NotFound(String),
    #[error("metadata file {path:?} is {actual} bytes, exceeding the {maximum}-byte limit")]
    MetadataTooLarge {
        path: PathBuf,
        actual: u64,
        maximum: u64,
    },
    #[error("snapshot already exists for {vm}: {snapshot}")]
    SnapshotAlreadyExists { vm: String, snapshot: String },
    #[error("snapshot not found for {vm}: {snapshot}")]
    SnapshotNotFound { vm: String, snapshot: String },
    #[error("disk snapshot metadata not found for {vm}: {snapshot}")]
    SnapshotDiskMetadataNotFound { vm: String, snapshot: String },
    #[error("suspend image metadata not found for {vm}: {snapshot}")]
    SnapshotSuspendImageMetadataNotFound { vm: String, snapshot: String },
    #[error("suspend image is missing: {0}")]
    SnapshotSuspendImageMissing(PathBuf),
    #[error("disk snapshot backing image is missing: {0}")]
    SnapshotDiskBackingMissing(PathBuf),
    #[error("disk snapshot overlay image is missing: {0}")]
    SnapshotDiskOverlayMissing(PathBuf),
    #[error("snapshot disk creation command failed ({status}): {command:?}: {stderr}")]
    SnapshotDiskCreateFailed {
        command: Vec<String>,
        status: String,
        stderr: String,
    },
    #[error("failed to execute snapshot disk creation command {command:?}: {source}")]
    SnapshotDiskCreateIo {
        command: Vec<String>,
        #[source]
        source: std::io::Error,
    },
    #[error("export output already exists: {0}")]
    ExportAlreadyExists(PathBuf),
    #[error("export output must not be the source bundle or inside it: source={source_bundle:?}, output={output:?}")]
    ExportOutputInsideSource {
        source_bundle: PathBuf,
        output: PathBuf,
    },
    #[error("import input is not a valid VM bundle: {0}")]
    InvalidImportBundle(PathBuf),
    #[error(
        "import input conflicts with the destination store: input={input:?}, output={output:?}"
    )]
    ImportPathConflict { input: PathBuf, output: PathBuf },
    #[error("unsupported VM bundle archive format: {0}")]
    UnsupportedArchiveFormat(PathBuf),
    #[error("VM bundle archive contains an unsafe path: {0}")]
    UnsafeArchiveEntry(PathBuf),
    #[error("VM bundle copy rejected unsupported file type: {0}")]
    UnsupportedBundleEntry(PathBuf),
    #[error("invalid VM state transition from {from:?} to {to:?}")]
    InvalidStateTransition {
        from: VmRuntimeState,
        to: VmRuntimeState,
    },
    #[error("disk creation command failed ({status}): {command:?}: {stderr}")]
    DiskCreateFailed {
        command: Vec<String>,
        status: String,
        stderr: String,
    },
    #[error("failed to execute disk creation command {command:?}: {source}")]
    DiskCreateIo {
        command: Vec<String>,
        #[source]
        source: std::io::Error,
    },
    #[error("primary disk is missing: {0}")]
    DiskMissing(PathBuf),
    #[error(
        "disk inspection requires qemu-img-managed formats; raw disk is prepared directly: {0}"
    )]
    DiskInspectUnsupportedRaw(PathBuf),
    #[error("disk inspection command failed ({status}): {command:?}: {stderr}")]
    DiskInspectFailed {
        command: Vec<String>,
        status: String,
        stderr: String,
    },
    #[error("failed to execute disk inspection command {command:?}: {source}")]
    DiskInspectIo {
        command: Vec<String>,
        #[source]
        source: std::io::Error,
    },
    #[error(
        "disk verification requires qemu-img-managed formats; raw disk is prepared directly: {0}"
    )]
    DiskVerifyUnsupportedRaw(PathBuf),
    #[error("disk verification command failed ({status}): {command:?}: {stderr}")]
    DiskVerifyFailed {
        command: Vec<String>,
        status: String,
        stderr: String,
    },
    #[error("failed to execute disk verification command {command:?}: {source}")]
    DiskVerifyIo {
        command: Vec<String>,
        #[source]
        source: std::io::Error,
    },
    #[error("linked clone disk creation command failed ({status}): {command:?}: {stderr}")]
    LinkedCloneDiskCreateFailed {
        command: Vec<String>,
        status: String,
        stderr: String,
    },
    #[error("failed to execute linked clone disk creation command {command:?}: {source}")]
    LinkedCloneDiskCreateIo {
        command: Vec<String>,
        #[source]
        source: std::io::Error,
    },
    #[error(
        "disk compaction requires qemu-img-managed formats; raw disk is prepared directly: {0}"
    )]
    DiskCompactUnsupportedRaw(PathBuf),
    #[error("disk compaction command failed ({status}): {command:?}: {stderr}")]
    DiskCompactFailed {
        command: Vec<String>,
        status: String,
        stderr: String,
    },
    #[error("failed to execute disk compaction command {command:?}: {source}")]
    DiskCompactIo {
        command: Vec<String>,
        #[source]
        source: std::io::Error,
    },
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("config error: {0}")]
    Config(#[from] ConfigError),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VmRuntimeState {
    Running,
    Suspended,
    Stopped,
}

impl std::fmt::Display for VmRuntimeState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VmRuntimeState::Running => write!(f, "running"),
            VmRuntimeState::Suspended => write!(f, "suspended"),
            VmRuntimeState::Stopped => write!(f, "stopped"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmRuntimeMetadata {
    pub state: VmRuntimeState,
    pub updated_at_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunnerMetadata {
    pub engine: String,
    pub pid: Option<u32>,
    pub command: Vec<String>,
    pub log_path: PathBuf,
    pub started_at_unix: u64,
    pub dry_run: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launch_spec_path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guest_tools: Option<GuestToolsRunnerMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disk: Option<DiskPreparationMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_disk: Option<ActiveDiskMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launch_readiness: Option<LaunchReadinessMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_control: Option<RuntimeControlMetadata>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeControlMetadata {
    pub kind: String,
    pub socket_path: PathBuf,
    pub commands: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeResourceVisibility {
    Foreground,
    Background,
}

impl std::fmt::Display for RuntimeResourceVisibility {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RuntimeResourceVisibility::Foreground => write!(f, "foreground"),
            RuntimeResourceVisibility::Background => write!(f, "background"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeResourcePolicyMetadata {
    pub vm: String,
    pub mode: String,
    pub profile: String,
    pub visibility: RuntimeResourceVisibility,
    pub state: VmRuntimeState,
    pub on_battery: bool,
    pub memory: String,
    pub cpu: String,
    pub display_fps_cap: String,
    pub rationale: String,
    pub live_applied: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub runtime_control_acknowledged: bool,
    pub live_apply_blockers: Vec<RuntimeResourcePolicyBlocker>,
    pub updated_at_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeResourcePolicyBlocker {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchReadinessMetadata {
    pub ready: bool,
    pub blockers: Vec<LaunchReadinessBlockerMetadata>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchReadinessBlockerMetadata {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotMetadata {
    pub name: String,
    pub kind: SnapshotKind,
    pub created_at_unix: u64,
    pub vm_state: VmRuntimeState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotDiskMetadata {
    pub snapshot: String,
    pub overlay_path: PathBuf,
    pub overlay_format: String,
    pub overlay_exists: bool,
    pub backing_path: PathBuf,
    pub backing_format: String,
    pub backing_exists: bool,
    pub create_command: Vec<String>,
    pub prepared_at_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotDiskCreateMetadata {
    pub snapshot: String,
    pub disk: SnapshotDiskMetadata,
    pub command: Vec<String>,
    pub executed: bool,
    pub exit_status: Option<String>,
    pub stdout: String,
    pub stderr: String,
    pub created_at_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotSuspendImageMetadata {
    pub snapshot: String,
    pub image_path: PathBuf,
    pub image_format: String,
    pub image_exists: bool,
    pub prepared_at_unix: u64,
}

/// Metadata describing a VM-scoped Fast Mode suspend image (the Apple VZ saved
/// machine state written when a Fast VM is suspended). Distinct from
/// [`SnapshotSuspendImageMetadata`], which is keyed by snapshot name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FastSuspendImageMetadata {
    pub vm: String,
    pub image_path: PathBuf,
    pub image_format: String,
    pub image_exists: bool,
    pub updated_at_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplicationConsistentSnapshotPreflightMetadata {
    pub snapshot: String,
    pub connected: bool,
    pub required_capabilities: Vec<String>,
    pub available_capabilities: Vec<String>,
    pub missing_capabilities: Vec<String>,
    pub ready: bool,
    pub planned_freeze_semantics: String,
    pub planned_thaw_semantics: String,
    pub runtime_updated_at_unix: Option<u64>,
    pub prepared_at_unix: u64,
}

pub(crate) const APPLICATION_CONSISTENT_FREEZE_SEMANTICS: &str =
    "daemon-owned guest-tools fs-freeze request before disk snapshot when the preflight is ready";
pub(crate) const APPLICATION_CONSISTENT_THAW_SEMANTICS: &str =
    "daemon-owned guest-tools fs-thaw request after the snapshot attempt when freeze was dispatched";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotChainMetadata {
    pub active_disk: ActiveDiskMetadata,
    pub disks: Vec<SnapshotDiskMetadata>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ActiveDiskSource {
    Primary,
    SnapshotOverlay,
    SnapshotBacking,
}

impl std::fmt::Display for ActiveDiskSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ActiveDiskSource::Primary => write!(f, "primary"),
            ActiveDiskSource::SnapshotOverlay => write!(f, "snapshot-overlay"),
            ActiveDiskSource::SnapshotBacking => write!(f, "snapshot-backing"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveDiskMetadata {
    pub source: ActiveDiskSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<String>,
    pub path: PathBuf,
    pub format: String,
    pub exists: bool,
    pub activated_at_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotRestoreMetadata {
    pub snapshot: String,
    pub restored_at_unix: u64,
    pub restored_state: VmRuntimeState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_disk: Option<ActiveDiskMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suspend_image: Option<SnapshotSuspendImageMetadata>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmExportMetadata {
    pub vm: String,
    pub source: PathBuf,
    pub output: PathBuf,
    pub archive_format: String,
    pub copied_file_count: u64,
    pub copied_files: Vec<String>,
    pub manifest_preserved: bool,
    pub metadata_preserved: bool,
    pub exported_at_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmImportMetadata {
    pub vm: String,
    pub original_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requested_name: Option<String>,
    pub source: PathBuf,
    pub output: PathBuf,
    pub archive_format: String,
    pub copied_file_count: u64,
    pub copied_files: Vec<String>,
    pub manifest_preserved: bool,
    pub metadata_preserved: bool,
    pub manifest_identity_rewritten: bool,
    pub imported_at_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmCloneMetadata {
    pub vm: String,
    pub source: PathBuf,
    pub output: PathBuf,
    #[serde(default)]
    pub linked: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backing_path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backing_format: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub create_command: Option<Vec<String>>,
    pub cloned_at_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmDeletionMetadata {
    pub vm: String,
    pub bundle: PathBuf,
    pub manifest_backup: PathBuf,
    pub metadata_path: PathBuf,
    pub deleted_at_unix: u64,
    pub metadata_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmMetadataRepairMetadata {
    pub vm: String,
    pub bundle: PathBuf,
    pub repaired: bool,
    pub actions: Vec<MetadataRepairAction>,
    pub repaired_at_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmLiveEvidenceMetadata {
    pub vm: String,
    pub source: PathBuf,
    pub preserved_path: PathBuf,
    pub copied_file_count: u64,
    pub copied_files: Vec<String>,
    pub recorded_at_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmManifestMigrationMetadata {
    pub vm: String,
    pub bundle: PathBuf,
    pub manifest_path: PathBuf,
    pub from_schema: String,
    pub to_schema: String,
    pub dry_run: bool,
    pub migrated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backup_path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipt_path: Option<PathBuf>,
    pub actions: Vec<MetadataRepairAction>,
    pub migrated_at_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetadataRepairAction {
    pub path: PathBuf,
    pub action: String,
    pub detail: String,
}

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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QmpSupervisorMetadata {
    pub events: Vec<QmpEvent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_event: Option<QmpEvent>,
    pub envelopes_read: usize,
    pub limit_reached: bool,
    pub updated_at_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiskPreparationMetadata {
    pub path: PathBuf,
    pub format: String,
    pub size: String,
    pub size_bytes: Option<u64>,
    pub exists: bool,
    pub created: bool,
    pub create_command: Option<Vec<String>>,
    pub prepared_at_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiskCreateMetadata {
    pub preparation: DiskPreparationMetadata,
    pub command: Option<Vec<String>>,
    pub executed: bool,
    pub exit_status: Option<String>,
    pub stdout: String,
    pub stderr: String,
    pub created_at_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiskInspectMetadata {
    pub preparation: DiskPreparationMetadata,
    pub command: Vec<String>,
    pub exit_status: String,
    pub info: serde_json::Value,
    pub stdout: String,
    pub stderr: String,
    pub inspect_duration_microseconds: u64,
    pub inspected_at_unix: u64,
}
