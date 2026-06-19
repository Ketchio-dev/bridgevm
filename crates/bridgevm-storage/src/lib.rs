use bridgevm_config::{slug, ConfigError, VmManifest, SCHEMA_VERSION};
use bridgevm_qemu::{guest_tools_socket_path, QemuImgCommand, QmpEvent};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{
    env,
    ffi::OsStr,
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Output},
    thread::sleep,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use thiserror::Error;

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("VM already exists: {0}")]
    AlreadyExists(String),
    #[error("VM not found: {0}")]
    NotFound(String),
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

const APPLICATION_CONSISTENT_FREEZE_SEMANTICS: &str =
    "daemon-owned guest-tools fs-freeze request before disk snapshot when the preflight is ready";
const APPLICATION_CONSISTENT_THAW_SEMANTICS: &str =
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiskVerifyMetadata {
    pub active_disk: ActiveDiskMetadata,
    pub command: Vec<String>,
    pub exit_status: String,
    pub report: serde_json::Value,
    pub stdout: String,
    pub stderr: String,
    pub verify_duration_microseconds: u64,
    pub verified_at_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiskCompactMetadata {
    pub preparation: DiskPreparationMetadata,
    pub active_disk: ActiveDiskMetadata,
    pub command: Vec<String>,
    pub temp_path: PathBuf,
    pub backup_path: PathBuf,
    pub exit_status: String,
    pub stdout: String,
    pub stderr: String,
    pub original_size_bytes: u64,
    pub compacted_size_bytes: u64,
    pub compact_duration_microseconds: u64,
    pub compacted_at_unix: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SnapshotKind {
    Disk,
    Suspend,
    ApplicationConsistent,
}

impl std::fmt::Display for SnapshotKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SnapshotKind::Disk => write!(f, "disk"),
            SnapshotKind::Suspend => write!(f, "suspend"),
            SnapshotKind::ApplicationConsistent => write!(f, "application-consistent"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct VmStore {
    root: PathBuf,
}

impl VmStore {
    pub fn default() -> Self {
        let root = env::var_os("BRIDGEVM_HOME")
            .map(PathBuf::from)
            .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".bridgevm")))
            .unwrap_or_else(|| PathBuf::from(".bridgevm"));
        Self::new(root)
    }

    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: absolutize(root.into()),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn vms_dir(&self) -> PathBuf {
        self.root.join("vms")
    }

    pub fn bundle_path(&self, name: &str) -> PathBuf {
        self.vms_dir().join(format!("{}.vmbridge", slug(name)))
    }

    pub fn ensure(&self) -> Result<(), StorageError> {
        fs::create_dir_all(self.vms_dir())?;
        Ok(())
    }

    pub fn create_vm(&self, manifest: &VmManifest) -> Result<PathBuf, StorageError> {
        self.ensure()?;
        let bundle = self.bundle_path(&manifest.name);
        if bundle.exists() {
            return Err(StorageError::AlreadyExists(manifest.name.clone()));
        }
        fs::create_dir_all(bundle.join("disks"))?;
        fs::create_dir_all(bundle.join("logs"))?;
        fs::create_dir_all(bundle.join("metadata"))?;
        manifest.write(&bundle.join("manifest.yaml"))?;
        self.write_state_at(&bundle, VmRuntimeState::Stopped)?;
        self.write_snapshots_at(&bundle, &[])?;
        let active_disk = self.primary_active_disk_at(&bundle, manifest);
        self.write_active_disk_at(&bundle, &active_disk)?;
        self.write_guest_tools_token_at(&bundle, &new_guest_tools_token()?)?;
        Ok(bundle)
    }

    pub fn list_vms(&self) -> Result<Vec<(PathBuf, VmManifest)>, StorageError> {
        self.ensure()?;
        let mut vms = Vec::new();
        for entry in fs::read_dir(self.vms_dir())? {
            let path = entry?.path();
            let manifest_path = path.join("manifest.yaml");
            if manifest_path.exists() && deletion_metadata_at(&path)?.is_none() {
                vms.push((path, VmManifest::read(&manifest_path)?));
            }
        }
        vms.sort_by(|a, b| a.1.name.cmp(&b.1.name));
        Ok(vms)
    }

    pub fn get_vm(&self, name: &str) -> Result<(PathBuf, VmManifest), StorageError> {
        let bundle = self.bundle_path(name);
        let manifest_path = bundle.join("manifest.yaml");
        if !manifest_path.exists() || deletion_metadata_at(&bundle)?.is_some() {
            return Err(StorageError::NotFound(name.to_string()));
        }
        Ok((bundle, VmManifest::read(&manifest_path)?))
    }

    pub fn get_vm_with_active_disk(
        &self,
        name: &str,
    ) -> Result<(PathBuf, VmManifest, ActiveDiskMetadata), StorageError> {
        let (bundle, mut manifest) = self.get_vm(name)?;
        let active_disk = self.active_disk_at(&bundle, &manifest)?;
        manifest.storage.primary.path = active_disk.path.display().to_string();
        manifest.storage.primary.format = active_disk.format.clone();
        Ok((bundle, manifest, active_disk))
    }

    pub fn delete_vm(&self, name: &str) -> Result<PathBuf, StorageError> {
        let (bundle, _) = self.get_vm(name)?;
        fs::remove_dir_all(&bundle)?;
        Ok(bundle)
    }

    pub fn delete_vm_metadata_only(&self, name: &str) -> Result<VmDeletionMetadata, StorageError> {
        let (bundle, manifest) = self.get_vm(name)?;
        let metadata_dir = bundle.join("metadata");
        fs::create_dir_all(&metadata_dir)?;
        let manifest_path = bundle.join("manifest.yaml");
        let manifest_backup = metadata_dir.join("deleted-manifest.yaml");
        fs::copy(&manifest_path, &manifest_backup)?;
        let metadata_path = deletion_metadata_path(&bundle);
        let metadata = VmDeletionMetadata {
            vm: manifest.name,
            bundle,
            manifest_backup,
            metadata_path: metadata_path.clone(),
            deleted_at_unix: now_unix(),
            metadata_only: true,
        };
        write_json_pretty_atomic(&metadata_path, &metadata)?;
        Ok(metadata)
    }

    pub fn export_vm(
        &self,
        name: &str,
        output: impl AsRef<Path>,
    ) -> Result<VmExportMetadata, StorageError> {
        let (bundle, manifest) = self.get_vm(name)?;
        let output = output.as_ref().to_path_buf();
        let source = fs::canonicalize(&bundle)?;
        let resolved_output = resolve_path_for_new(&output)?;
        if is_same_or_descendant(&resolved_output, &source) {
            return Err(StorageError::ExportOutputInsideSource {
                source_bundle: bundle,
                output,
            });
        }
        if output.exists() {
            return Err(StorageError::ExportAlreadyExists(output));
        }
        let archive_format = if is_tar_path(&output) {
            "tar"
        } else if is_unsupported_archive_path(&output) {
            return Err(StorageError::UnsupportedArchiveFormat(output));
        } else {
            "directory"
        };
        let copy_summary = summarize_bundle_copy(&bundle)?;
        let metadata = VmExportMetadata {
            vm: manifest.name,
            source: bundle,
            output: output.clone(),
            archive_format: archive_format.to_string(),
            copied_file_count: copy_summary.file_count,
            copied_files: copy_summary.files,
            manifest_preserved: copy_summary.manifest_preserved,
            metadata_preserved: copy_summary.metadata_preserved,
            exported_at_unix: now_unix(),
        };
        if is_tar_path(&output) {
            export_bundle_tar(&metadata.source, &output, &metadata)?;
        } else {
            copy_dir_all(&metadata.source, &output)?;
            let metadata_dir = output.join("metadata");
            fs::create_dir_all(&metadata_dir)?;
            fs::write(
                metadata_dir.join("export.json"),
                serde_json::to_string_pretty(&metadata)?,
            )?;
        }
        Ok(metadata)
    }

    pub fn import_vm(
        &self,
        input: impl AsRef<Path>,
        name_override: Option<&str>,
    ) -> Result<VmImportMetadata, StorageError> {
        self.ensure()?;
        let input = input.as_ref().to_path_buf();
        if input.is_file() {
            if !is_tar_path(&input) {
                return Err(StorageError::UnsupportedArchiveFormat(input));
            }
            let store_resolved = fs::canonicalize(&self.root)?;
            let input_resolved = fs::canonicalize(&input)?;
            if is_same_or_descendant(&input_resolved, &store_resolved) {
                return Err(StorageError::ImportPathConflict {
                    input,
                    output: self.vms_dir(),
                });
            }
            let staging = unique_temp_path("bridgevm-import-tar");
            let _staging_guard = TempDirGuard::new(staging.clone());
            extract_bundle_tar(&input, &staging)?;
            return self.import_vm_bundle(&staging, &input, name_override);
        }
        self.import_vm_bundle(&input, &input, name_override)
    }

    fn import_vm_bundle(
        &self,
        input: &Path,
        metadata_source: &Path,
        name_override: Option<&str>,
    ) -> Result<VmImportMetadata, StorageError> {
        let manifest_path = input.join("manifest.yaml");
        if !input.is_dir() || !manifest_path.exists() {
            return Err(StorageError::InvalidImportBundle(input.to_path_buf()));
        }

        let mut manifest = VmManifest::read(&manifest_path)?;
        let original_name = manifest.name.clone();
        let requested_name = name_override.map(str::to_string);
        if let Some(name) = name_override {
            manifest.name = name.to_string();
            manifest.network.hostname = format!("{}.bridgevm.local", slug(name));
        }

        let output = self.bundle_path(&manifest.name);
        let input_resolved = fs::canonicalize(&input)?;
        let output_resolved = resolve_path_for_new(&output)?;
        let store_resolved = fs::canonicalize(&self.root)?;
        if is_same_or_descendant(&output_resolved, &input_resolved)
            || is_same_or_descendant(&input_resolved, &output_resolved)
            || is_same_or_descendant(&input_resolved, &store_resolved)
        {
            return Err(StorageError::ImportPathConflict {
                input: input.to_path_buf(),
                output,
            });
        }
        if output.exists() {
            return Err(StorageError::AlreadyExists(manifest.name));
        }

        let copy_summary = copy_dir_all(&input, &output)?;
        manifest.write(&output.join("manifest.yaml"))?;
        let metadata = VmImportMetadata {
            vm: manifest.name,
            original_name,
            requested_name,
            source: metadata_source.to_path_buf(),
            output: output.clone(),
            archive_format: if is_tar_path(metadata_source) {
                "tar".to_string()
            } else {
                "directory".to_string()
            },
            copied_file_count: copy_summary.file_count,
            copied_files: copy_summary.files,
            manifest_preserved: copy_summary.manifest_preserved,
            metadata_preserved: copy_summary.metadata_preserved,
            manifest_identity_rewritten: name_override.is_some(),
            imported_at_unix: now_unix(),
        };
        let metadata_dir = output.join("metadata");
        fs::create_dir_all(&metadata_dir)?;
        fs::write(
            metadata_dir.join("import.json"),
            serde_json::to_string_pretty(&metadata)?,
        )?;
        Ok(metadata)
    }

    pub fn clone_vm(
        &self,
        name: &str,
        new_name: &str,
        linked: bool,
    ) -> Result<VmCloneMetadata, StorageError> {
        self.clone_vm_with(name, new_name, linked, run_command)
    }

    fn clone_vm_with<F>(
        &self,
        name: &str,
        new_name: &str,
        linked: bool,
        mut run: F,
    ) -> Result<VmCloneMetadata, StorageError>
    where
        F: FnMut(&str, &[String]) -> Result<Output, std::io::Error>,
    {
        self.ensure()?;
        let (source, mut manifest) = self.get_vm(name)?;
        let source_active_disk = if linked {
            Some(self.active_disk_at(&source, &manifest)?)
        } else {
            None
        };
        manifest.name = new_name.to_string();
        manifest.network.hostname = format!("{}.bridgevm.local", slug(new_name));

        let output = self.bundle_path(new_name);
        if output.exists() {
            return Err(StorageError::AlreadyExists(new_name.to_string()));
        }

        if let Err(error) = copy_dir_all(&source, &output) {
            let _ = fs::remove_dir_all(&output);
            return Err(error);
        }

        let clone_result: Result<VmCloneMetadata, StorageError> = (|| {
            let mut backing_path = None;
            let mut backing_format = None;
            let mut create_command = None;
            if let Some(source_active_disk) = source_active_disk {
                if !source_active_disk.path.exists() {
                    return Err(StorageError::DiskMissing(source_active_disk.path));
                }
                let disks_dir = output.join("disks");
                if disks_dir.exists() {
                    fs::remove_dir_all(&disks_dir)?;
                }
                fs::create_dir_all(&disks_dir)?;
                let snapshot_disk_metadata_dir = output.join("metadata").join("snapshot-disks");
                if snapshot_disk_metadata_dir.exists() {
                    fs::remove_dir_all(snapshot_disk_metadata_dir)?;
                }
                let suspend_images_dir = output.join("suspend-images");
                if suspend_images_dir.exists() {
                    fs::remove_dir_all(suspend_images_dir)?;
                }

                manifest.storage.primary.path = "disks/root.qcow2".to_string();
                manifest.storage.primary.format = "qcow2".to_string();
                let overlay_path = output.join("disks").join("root.qcow2");
                let command = QemuImgCommand::create_backed_disk(
                    &overlay_path,
                    "qcow2",
                    source_active_disk.format.clone(),
                    &source_active_disk.path,
                )
                .render_shell_words();
                let command_output = run(&command[0], &command[1..]).map_err(|source| {
                    StorageError::LinkedCloneDiskCreateIo {
                        command: command.clone(),
                        source,
                    }
                })?;
                let stderr = String::from_utf8_lossy(&command_output.stderr).to_string();
                if !command_output.status.success() {
                    return Err(StorageError::LinkedCloneDiskCreateFailed {
                        command,
                        status: command_output.status.to_string(),
                        stderr,
                    });
                }
                let active_disk = ActiveDiskMetadata {
                    source: ActiveDiskSource::Primary,
                    snapshot: None,
                    path: overlay_path,
                    format: "qcow2".to_string(),
                    exists: true,
                    activated_at_unix: now_unix(),
                };
                self.write_active_disk_at(&output, &active_disk)?;
                self.write_snapshots_at(&output, &[])?;
                backing_path = Some(source_active_disk.path);
                backing_format = Some(source_active_disk.format);
                create_command = Some(command);
            } else {
                self.rebase_copied_bundle_metadata(&source, &output, &manifest)?;
            }
            // Make the clone an independent VM: drop the source's persisted
            // per-VM identity and transient runtime state so it is not a
            // network/identity duplicate and starts stopped/clean.
            self.reset_clone_runtime_identity(&output)?;
            manifest.write(&output.join("manifest.yaml"))?;
            let metadata = VmCloneMetadata {
                vm: manifest.name,
                source,
                output: output.clone(),
                linked,
                backing_path,
                backing_format,
                create_command,
                cloned_at_unix: now_unix(),
            };
            let metadata_dir = output.join("metadata");
            fs::create_dir_all(&metadata_dir)?;
            write_json_pretty_atomic(&metadata_dir.join("clone.json"), &metadata)?;
            Ok(metadata)
        })();

        if clone_result.is_err() {
            let _ = fs::remove_dir_all(&output);
        }
        clone_result
    }

    pub fn repair_metadata(&self, name: &str) -> Result<VmMetadataRepairMetadata, StorageError> {
        let (bundle, manifest) = self.get_vm(name)?;
        let mut actions = Vec::new();

        self.ensure_dir_for_repair(&bundle.join("metadata"), &mut actions)?;
        self.ensure_dir_for_repair(&bundle.join("disks"), &mut actions)?;
        self.ensure_dir_for_repair(&bundle.join("logs"), &mut actions)?;

        let state_path = bundle.join("metadata").join("state.json");
        match read_json_file::<VmRuntimeMetadata>(&state_path)? {
            Some(_) => {}
            None => {
                self.write_state_at(&bundle, VmRuntimeState::Stopped)?;
                actions.push(metadata_repair_action(
                    &state_path,
                    "repaired",
                    "wrote default stopped runtime state metadata",
                ));
            }
        }

        let snapshots_path = bundle.join("metadata").join("snapshots.json");
        let snapshots = match read_json_file::<Vec<SnapshotMetadata>>(&snapshots_path)? {
            Some(snapshots) => snapshots,
            None => {
                self.write_snapshots_at(&bundle, &[])?;
                actions.push(metadata_repair_action(
                    &snapshots_path,
                    "repaired",
                    "wrote empty snapshot list metadata",
                ));
                Vec::new()
            }
        };

        let active_disk_path = bundle.join("metadata").join("active-disk.json");
        let primary_active_disk = self.primary_active_disk_at(&bundle, &manifest);
        match read_json_file::<ActiveDiskMetadata>(&active_disk_path)? {
            Some(mut active_disk) => {
                let exists = active_disk.path.exists();
                if active_disk.exists != exists {
                    active_disk.exists = exists;
                    self.write_active_disk_at(&bundle, &active_disk)?;
                    actions.push(metadata_repair_action(
                        &active_disk_path,
                        "refreshed",
                        "updated active disk existence flag",
                    ));
                }
            }
            None => {
                self.write_active_disk_at(&bundle, &primary_active_disk)?;
                actions.push(metadata_repair_action(
                    &active_disk_path,
                    "repaired",
                    "wrote primary active disk metadata from manifest",
                ));
            }
        }

        let token_path = guest_tools_token_path(&bundle);
        match read_json_file::<GuestToolsTokenMetadata>(&token_path)? {
            Some(_) => {}
            None => {
                self.write_guest_tools_token_at(&bundle, &new_guest_tools_token()?)?;
                actions.push(metadata_repair_action(
                    &token_path,
                    "repaired",
                    "wrote new guest-tools token metadata",
                ));
            }
        }

        let primary_disk_path = bundle.join("metadata").join("primary-disk.json");
        match read_json_file::<DiskPreparationMetadata>(&primary_disk_path)? {
            Some(_) => {}
            None => {
                let primary_disk = primary_disk_preparation_metadata(&bundle, &manifest);
                write_json_pretty_atomic(&primary_disk_path, &primary_disk)?;
                actions.push(metadata_repair_action(
                    &primary_disk_path,
                    "repaired",
                    "wrote primary disk preparation metadata without creating a disk",
                ));
            }
        }

        for snapshot in snapshots {
            match snapshot.kind {
                SnapshotKind::Disk => {
                    let path = snapshot_disk_metadata_path(&bundle, &snapshot.name);
                    match read_json_file::<SnapshotDiskMetadata>(&path)? {
                        Some(mut metadata) => {
                            let overlay_exists = metadata.overlay_path.exists();
                            let backing_exists = metadata.backing_path.exists();
                            if metadata.overlay_exists != overlay_exists
                                || metadata.backing_exists != backing_exists
                            {
                                metadata.overlay_exists = overlay_exists;
                                metadata.backing_exists = backing_exists;
                                self.write_snapshot_disk_metadata_at(
                                    &bundle,
                                    &snapshot.name,
                                    &metadata,
                                )?;
                                actions.push(metadata_repair_action(
                                    &path,
                                    "refreshed",
                                    "updated snapshot disk existence flags",
                                ));
                            }
                        }
                        None => {
                            self.prepare_snapshot_disk_at(&bundle, &manifest, &snapshot.name)?;
                            actions.push(metadata_repair_action(
                                &path,
                                "repaired",
                                "wrote disk snapshot chain metadata from active disk",
                            ));
                        }
                    }
                }
                SnapshotKind::Suspend => {
                    let path = snapshot_suspend_image_metadata_path(&bundle, &snapshot.name);
                    match read_json_file::<SnapshotSuspendImageMetadata>(&path)? {
                        Some(mut metadata) => {
                            let image_exists = metadata.image_path.exists();
                            if metadata.image_exists != image_exists {
                                metadata.image_exists = image_exists;
                                self.write_snapshot_suspend_image_metadata_at(
                                    &bundle,
                                    &snapshot.name,
                                    &metadata,
                                )?;
                                actions.push(metadata_repair_action(
                                    &path,
                                    "refreshed",
                                    "updated suspend image existence flag",
                                ));
                            }
                        }
                        None => {
                            self.prepare_snapshot_suspend_image_at(&bundle, &snapshot.name)?;
                            actions.push(metadata_repair_action(
                                &path,
                                "repaired",
                                "wrote suspend image metadata",
                            ));
                        }
                    }
                }
                SnapshotKind::ApplicationConsistent => {
                    let path =
                        application_consistent_snapshot_preflight_path(&bundle, &snapshot.name);
                    if read_json_file::<ApplicationConsistentSnapshotPreflightMetadata>(&path)?
                        .is_none()
                    {
                        self.prepare_application_consistent_snapshot_preflight_at(
                            &bundle,
                            &snapshot.name,
                        )?;
                        actions.push(metadata_repair_action(
                            &path,
                            "repaired",
                            "wrote application-consistent snapshot preflight metadata",
                        ));
                    }
                }
            }
        }

        Ok(VmMetadataRepairMetadata {
            vm: manifest.name,
            bundle,
            repaired: !actions.is_empty(),
            actions,
            repaired_at_unix: now_unix(),
        })
    }

    pub fn migrate_manifest(
        &self,
        name: &str,
        dry_run: bool,
    ) -> Result<VmManifestMigrationMetadata, StorageError> {
        let bundle = self.bundle_path(name);
        let manifest_path = bundle.join("manifest.yaml");
        if !manifest_path.exists() || deletion_metadata_at(&bundle)?.is_some() {
            return Err(StorageError::NotFound(name.to_string()));
        }

        let raw_manifest = fs::read_to_string(&manifest_path)?;
        let manifest_value: serde_yaml::Value =
            serde_yaml::from_str(&raw_manifest).map_err(ConfigError::from)?;
        let from_schema = manifest_value
            .get("schemaVersion")
            .and_then(serde_yaml::Value::as_str)
            .unwrap_or("<missing>")
            .to_string();
        if from_schema != SCHEMA_VERSION {
            return Err(StorageError::Config(ConfigError::UnsupportedSchema {
                expected: SCHEMA_VERSION,
                actual: from_schema,
            }));
        }

        let manifest: VmManifest =
            serde_yaml::from_str(&raw_manifest).map_err(ConfigError::from)?;
        manifest.validate()?;

        let metadata_dir = bundle.join("metadata");
        let backup_path = metadata_dir.join("manifest-before-migration.yaml");
        let receipt_path = metadata_dir.join("manifest-migration.json");
        let migrated_at_unix = now_unix();

        let mut actions = vec![metadata_repair_action(
            &manifest_path,
            "validated",
            "manifest already uses the current schema",
        )];
        if dry_run {
            actions.push(metadata_repair_action(
                &receipt_path,
                "planned",
                "dry-run did not write migration receipt or manifest backup",
            ));
            return Ok(VmManifestMigrationMetadata {
                vm: manifest.name,
                bundle,
                manifest_path,
                from_schema,
                to_schema: SCHEMA_VERSION.to_string(),
                dry_run,
                migrated: false,
                backup_path: None,
                receipt_path: None,
                actions,
                migrated_at_unix,
            });
        }

        fs::create_dir_all(&metadata_dir)?;
        fs::copy(&manifest_path, &backup_path)?;
        actions.push(metadata_repair_action(
            &backup_path,
            "backed-up",
            "copied manifest before migration",
        ));

        let metadata = VmManifestMigrationMetadata {
            vm: manifest.name,
            bundle,
            manifest_path,
            from_schema,
            to_schema: SCHEMA_VERSION.to_string(),
            dry_run,
            migrated: false,
            backup_path: Some(backup_path),
            receipt_path: Some(receipt_path.clone()),
            actions,
            migrated_at_unix,
        };
        write_json_pretty_atomic(&receipt_path, &metadata)?;
        Ok(metadata)
    }

    fn ensure_dir_for_repair(
        &self,
        path: &Path,
        actions: &mut Vec<MetadataRepairAction>,
    ) -> Result<(), StorageError> {
        if !path.exists() {
            fs::create_dir_all(path)?;
            actions.push(metadata_repair_action(
                path,
                "created",
                "created missing VM bundle directory",
            ));
        }
        Ok(())
    }

    pub fn prepare_primary_disk(
        &self,
        name: &str,
    ) -> Result<DiskPreparationMetadata, StorageError> {
        let (bundle, manifest) = self.get_vm(name)?;
        let path = resolve_bundle_path(&bundle, &manifest.storage.primary.path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let format = manifest.storage.primary.format.clone();
        let size = manifest.storage.primary.size.clone();
        let size_bytes = parse_size_bytes(&size);
        let mut exists = path.exists();
        let mut created = false;
        let mut create_command = None;

        if !exists {
            if format == "raw" {
                let file = fs::File::create(&path)?;
                if let Some(bytes) = size_bytes {
                    file.set_len(bytes)?;
                }
                exists = true;
                created = true;
            } else {
                create_command = Some(
                    QemuImgCommand::create_disk(&path, format.clone(), size.clone())
                        .render_shell_words(),
                );
            }
        }

        let metadata = DiskPreparationMetadata {
            path,
            format,
            size,
            size_bytes,
            exists,
            created,
            create_command,
            prepared_at_unix: now_unix(),
        };
        let metadata_dir = bundle.join("metadata");
        fs::create_dir_all(&metadata_dir)?;
        fs::write(
            metadata_dir.join("primary-disk.json"),
            serde_json::to_string_pretty(&metadata)?,
        )?;
        Ok(metadata)
    }

    pub fn active_disk(&self, name: &str) -> Result<ActiveDiskMetadata, StorageError> {
        let (bundle, manifest) = self.get_vm(name)?;
        self.active_disk_at(&bundle, &manifest)
    }

    pub fn snapshot_chain(&self, name: &str) -> Result<SnapshotChainMetadata, StorageError> {
        let (bundle, manifest) = self.get_vm(name)?;
        let active_disk = self.active_disk_at(&bundle, &manifest)?;
        let mut disks = Vec::new();
        let dir = bundle.join("metadata").join("snapshot-disks");
        if dir.exists() {
            for entry in fs::read_dir(dir)? {
                let path = entry?.path();
                if path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.ends_with("-create.json"))
                {
                    continue;
                }
                if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                    continue;
                }
                let mut disk: SnapshotDiskMetadata =
                    serde_json::from_str(&fs::read_to_string(path)?)?;
                disk.overlay_exists = disk.overlay_path.exists();
                disk.backing_exists = disk.backing_path.exists();
                disks.push(disk);
            }
        }
        disks.sort_by(|a, b| a.snapshot.cmp(&b.snapshot));
        Ok(SnapshotChainMetadata { active_disk, disks })
    }

    pub fn prepare_active_disk(
        &self,
        name: &str,
    ) -> Result<(DiskPreparationMetadata, ActiveDiskMetadata), StorageError> {
        let (bundle, manifest) = self.get_vm(name)?;
        let active_disk = self.active_disk_at(&bundle, &manifest)?;
        if active_disk.source == ActiveDiskSource::Primary {
            let disk = self.prepare_primary_disk(name)?;
            let active_disk = ActiveDiskMetadata {
                exists: disk.exists,
                path: disk.path.clone(),
                format: disk.format.clone(),
                ..active_disk
            };
            self.write_active_disk_at(&bundle, &active_disk)?;
            return Ok((disk, active_disk));
        }

        let disk = DiskPreparationMetadata {
            path: active_disk.path.clone(),
            format: active_disk.format.clone(),
            size: manifest.storage.primary.size,
            size_bytes: None,
            exists: active_disk.path.exists(),
            created: false,
            create_command: None,
            prepared_at_unix: now_unix(),
        };
        let active_disk = ActiveDiskMetadata {
            exists: disk.exists,
            ..active_disk
        };
        self.write_active_disk_at(&bundle, &active_disk)?;
        Ok((disk, active_disk))
    }

    pub fn create_primary_disk(&self, name: &str) -> Result<DiskCreateMetadata, StorageError> {
        self.create_primary_disk_with(name, run_command)
    }

    pub fn inspect_primary_disk(&self, name: &str) -> Result<DiskInspectMetadata, StorageError> {
        self.inspect_primary_disk_with(name, run_command)
    }

    pub fn verify_active_disk(&self, name: &str) -> Result<DiskVerifyMetadata, StorageError> {
        self.verify_active_disk_with(name, run_command)
    }

    pub fn compact_active_disk(&self, name: &str) -> Result<DiskCompactMetadata, StorageError> {
        self.compact_active_disk_with(name, run_command)
    }

    fn verify_active_disk_with<F>(
        &self,
        name: &str,
        mut run: F,
    ) -> Result<DiskVerifyMetadata, StorageError>
    where
        F: FnMut(&str, &[String]) -> Result<Output, std::io::Error>,
    {
        let (_preparation, active_disk) = self.prepare_active_disk(name)?;
        if !active_disk.exists {
            return Err(StorageError::DiskMissing(active_disk.path));
        }
        if active_disk.format == "raw" {
            return Err(StorageError::DiskVerifyUnsupportedRaw(active_disk.path));
        }

        let command = QemuImgCommand::check_json(&active_disk.path).render_shell_words();
        let verify_started = Instant::now();
        let output =
            run(&command[0], &command[1..]).map_err(|source| StorageError::DiskVerifyIo {
                command: command.clone(),
                source,
            })?;
        let verify_duration_microseconds = duration_micros_u64(verify_started.elapsed());
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let status = output.status.to_string();
        if !output.status.success() {
            return Err(StorageError::DiskVerifyFailed {
                command,
                status,
                stderr,
            });
        }
        let report = serde_json::from_str(&stdout)?;
        let metadata = DiskVerifyMetadata {
            active_disk,
            command,
            exit_status: status,
            report,
            stdout,
            stderr,
            verify_duration_microseconds,
            verified_at_unix: now_unix(),
        };
        self.write_disk_verify_metadata(name, &metadata)?;
        Ok(metadata)
    }

    fn compact_active_disk_with<F>(
        &self,
        name: &str,
        mut run: F,
    ) -> Result<DiskCompactMetadata, StorageError>
    where
        F: FnMut(&str, &[String]) -> Result<Output, std::io::Error>,
    {
        let (bundle, _) = self.get_vm(name)?;
        let (preparation, active_disk) = self.prepare_active_disk(name)?;
        if !active_disk.exists {
            return Err(StorageError::DiskMissing(active_disk.path));
        }
        if active_disk.format == "raw" {
            return Err(StorageError::DiskCompactUnsupportedRaw(active_disk.path));
        }

        let original_size_bytes = fs::metadata(&active_disk.path)?.len();
        let compacted_at_unix = now_unix();
        let temp_path = active_disk
            .path
            .with_extension(format!("{}.compact.tmp", active_disk.format));
        let backup_path = active_disk.path.with_extension(format!(
            "{}.precompact-{compacted_at_unix}",
            active_disk.format
        ));
        if temp_path.exists() {
            fs::remove_file(&temp_path)?;
        }

        let command =
            QemuImgCommand::convert_compact(&active_disk.path, &temp_path, &active_disk.format)
                .render_shell_words();
        let compact_started = Instant::now();
        let output =
            run(&command[0], &command[1..]).map_err(|source| StorageError::DiskCompactIo {
                command: command.clone(),
                source,
            })?;
        let compact_duration_microseconds = duration_micros_u64(compact_started.elapsed());
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let status = output.status.to_string();
        if !output.status.success() {
            return Err(StorageError::DiskCompactFailed {
                command,
                status,
                stderr,
            });
        }
        if !temp_path.exists() {
            return Err(StorageError::DiskMissing(temp_path));
        }

        fs::rename(&active_disk.path, &backup_path)?;
        fs::rename(&temp_path, &active_disk.path)?;
        let compacted_size_bytes = fs::metadata(&active_disk.path)?.len();

        let active_disk = ActiveDiskMetadata {
            exists: true,
            ..active_disk
        };
        self.write_active_disk_at(&bundle, &active_disk)?;

        let metadata = DiskCompactMetadata {
            preparation: DiskPreparationMetadata {
                exists: active_disk.path.exists(),
                ..preparation
            },
            active_disk,
            command,
            temp_path,
            backup_path,
            exit_status: status,
            stdout,
            stderr,
            original_size_bytes,
            compacted_size_bytes,
            compact_duration_microseconds,
            compacted_at_unix,
        };
        self.write_disk_compact_metadata(name, &metadata)?;
        Ok(metadata)
    }

    fn inspect_primary_disk_with<F>(
        &self,
        name: &str,
        mut run: F,
    ) -> Result<DiskInspectMetadata, StorageError>
    where
        F: FnMut(&str, &[String]) -> Result<Output, std::io::Error>,
    {
        let preparation = self.prepare_primary_disk(name)?;
        if !preparation.exists {
            return Err(StorageError::DiskMissing(preparation.path));
        }
        if preparation.format == "raw" {
            return Err(StorageError::DiskInspectUnsupportedRaw(preparation.path));
        }

        let command = QemuImgCommand::info_json(&preparation.path).render_shell_words();
        let inspect_started = Instant::now();
        let output =
            run(&command[0], &command[1..]).map_err(|source| StorageError::DiskInspectIo {
                command: command.clone(),
                source,
            })?;
        let inspect_duration_microseconds = duration_micros_u64(inspect_started.elapsed());
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let status = output.status.to_string();
        if !output.status.success() {
            return Err(StorageError::DiskInspectFailed {
                command,
                status,
                stderr,
            });
        }
        let info = serde_json::from_str(&stdout)?;
        let metadata = DiskInspectMetadata {
            preparation,
            command,
            exit_status: status,
            info,
            stdout,
            stderr,
            inspect_duration_microseconds,
            inspected_at_unix: now_unix(),
        };
        self.write_disk_inspect_metadata(name, &metadata)?;
        Ok(metadata)
    }

    fn create_primary_disk_with<F>(
        &self,
        name: &str,
        mut run: F,
    ) -> Result<DiskCreateMetadata, StorageError>
    where
        F: FnMut(&str, &[String]) -> Result<Output, std::io::Error>,
    {
        let mut preparation = self.prepare_primary_disk(name)?;
        let command = preparation.create_command.clone();
        let Some(command_words) = command.clone() else {
            let metadata = DiskCreateMetadata {
                preparation,
                command,
                executed: false,
                exit_status: None,
                stdout: String::new(),
                stderr: String::new(),
                created_at_unix: now_unix(),
            };
            self.write_disk_create_metadata(name, &metadata)?;
            return Ok(metadata);
        };

        let output = run(&command_words[0], &command_words[1..]).map_err(|source| {
            StorageError::DiskCreateIo {
                command: command_words.clone(),
                source,
            }
        })?;
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let status = output.status.to_string();
        if !output.status.success() {
            return Err(StorageError::DiskCreateFailed {
                command: command_words,
                status,
                stderr,
            });
        }

        preparation = self.prepare_primary_disk(name)?;
        let metadata = DiskCreateMetadata {
            preparation,
            command,
            executed: true,
            exit_status: Some(status),
            stdout,
            stderr,
            created_at_unix: now_unix(),
        };
        self.write_disk_create_metadata(name, &metadata)?;
        Ok(metadata)
    }

    pub fn state(&self, name: &str) -> Result<VmRuntimeMetadata, StorageError> {
        let (bundle, _) = self.get_vm(name)?;
        self.state_at(&bundle)
    }

    pub fn transition_state(
        &self,
        name: &str,
        to: VmRuntimeState,
    ) -> Result<VmRuntimeMetadata, StorageError> {
        let (bundle, _) = self.get_vm(name)?;
        let current = self.state_at(&bundle)?;
        validate_transition(current.state, to)?;
        self.write_state_at(&bundle, to)
    }

    /// Write the runtime state UNCONDITIONALLY, bypassing the transition-validity
    /// check. For use only after an irreversible action has already made the new
    /// state the ground truth (e.g. the backend process has been killed, or a
    /// suspend snapshot committed): the recorded state must then reflect reality
    /// even if the prior state was unexpected — refusing the write here is what
    /// strands a dead backend recorded as `Running`.
    pub fn force_transition_state(
        &self,
        name: &str,
        to: VmRuntimeState,
    ) -> Result<VmRuntimeMetadata, StorageError> {
        let (bundle, _) = self.get_vm(name)?;
        self.write_state_at(&bundle, to)
    }

    pub fn create_snapshot(
        &self,
        vm_name: &str,
        snapshot_name: &str,
        kind: SnapshotKind,
    ) -> Result<SnapshotMetadata, StorageError> {
        let (bundle, manifest) = self.get_vm(vm_name)?;
        let _lock = MetadataLock::acquire(&bundle, "snapshots.lock")?;
        let state = self.state_at(&bundle)?.state;
        let mut snapshots = self.snapshots(vm_name)?;
        if snapshots
            .iter()
            .any(|snapshot| snapshot.name == snapshot_name)
        {
            return Err(StorageError::SnapshotAlreadyExists {
                vm: vm_name.to_string(),
                snapshot: snapshot_name.to_string(),
            });
        }
        let snapshot = SnapshotMetadata {
            name: snapshot_name.to_string(),
            kind,
            created_at_unix: now_unix(),
            vm_state: state,
        };
        snapshots.push(snapshot.clone());
        snapshots.sort_by(|a, b| a.name.cmp(&b.name));
        self.write_snapshots_at(&bundle, &snapshots)?;
        match kind {
            SnapshotKind::Disk => {
                self.prepare_snapshot_disk_at(&bundle, &manifest, snapshot_name)?;
            }
            SnapshotKind::Suspend => {
                self.prepare_snapshot_suspend_image_at(&bundle, snapshot_name)?;
            }
            SnapshotKind::ApplicationConsistent => {
                self.prepare_application_consistent_snapshot_preflight_at(&bundle, snapshot_name)?;
            }
        }
        Ok(snapshot)
    }

    pub fn snapshot_disk_metadata(
        &self,
        vm_name: &str,
        snapshot_name: &str,
    ) -> Result<Option<SnapshotDiskMetadata>, StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        let path = snapshot_disk_metadata_path(&bundle, snapshot_name);
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(serde_json::from_str(&fs::read_to_string(path)?)?))
    }

    pub fn snapshot_suspend_image_metadata(
        &self,
        vm_name: &str,
        snapshot_name: &str,
    ) -> Result<Option<SnapshotSuspendImageMetadata>, StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        let path = snapshot_suspend_image_metadata_path(&bundle, snapshot_name);
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(serde_json::from_str(&fs::read_to_string(path)?)?))
    }

    /// Record that a Fast Mode suspend image now exists at `image_path`,
    /// writing the VM-scoped suspend-image metadata. Used after a successful
    /// Fast Mode suspend.
    pub fn mark_fast_suspend_image_exists(
        &self,
        vm_name: &str,
        image_path: &Path,
    ) -> Result<FastSuspendImageMetadata, StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        let metadata = FastSuspendImageMetadata {
            vm: vm_name.to_string(),
            image_path: image_path.to_path_buf(),
            image_format: "apple-vz-saved-state-v1".to_string(),
            image_exists: image_path.exists(),
            updated_at_unix: now_unix(),
        };
        write_json_pretty_atomic(
            &fast_suspend_image_metadata_path(&bundle, vm_name),
            &metadata,
        )?;
        Ok(metadata)
    }

    /// Read the VM-scoped Fast Mode suspend-image metadata, if present.
    pub fn fast_suspend_image_metadata(
        &self,
        vm_name: &str,
    ) -> Result<Option<FastSuspendImageMetadata>, StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        let path = fast_suspend_image_metadata_path(&bundle, vm_name);
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(serde_json::from_str(&fs::read_to_string(path)?)?))
    }

    pub fn application_consistent_snapshot_preflight_metadata(
        &self,
        vm_name: &str,
        snapshot_name: &str,
    ) -> Result<Option<ApplicationConsistentSnapshotPreflightMetadata>, StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        let path = application_consistent_snapshot_preflight_path(&bundle, snapshot_name);
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(serde_json::from_str(&fs::read_to_string(path)?)?))
    }

    pub fn create_snapshot_disk(
        &self,
        vm_name: &str,
        snapshot_name: &str,
    ) -> Result<SnapshotDiskCreateMetadata, StorageError> {
        self.create_snapshot_disk_with(vm_name, snapshot_name, run_command)
    }

    fn create_snapshot_disk_with<F>(
        &self,
        vm_name: &str,
        snapshot_name: &str,
        mut run: F,
    ) -> Result<SnapshotDiskCreateMetadata, StorageError>
    where
        F: FnMut(&str, &[String]) -> Result<Output, std::io::Error>,
    {
        let (bundle, _) = self.get_vm(vm_name)?;
        let mut disk = self
            .snapshot_disk_metadata(vm_name, snapshot_name)?
            .ok_or_else(|| StorageError::SnapshotDiskMetadataNotFound {
                vm: vm_name.to_string(),
                snapshot: snapshot_name.to_string(),
            })?;
        disk.backing_exists = disk.backing_path.exists();
        disk.overlay_exists = disk.overlay_path.exists();
        if !disk.backing_exists {
            self.write_snapshot_disk_metadata_at(&bundle, snapshot_name, &disk)?;
            return Err(StorageError::SnapshotDiskBackingMissing(
                disk.backing_path.clone(),
            ));
        }

        let command = disk.create_command.clone();
        if disk.overlay_exists {
            let active_disk =
                self.snapshot_overlay_active_disk_at(snapshot_name, &disk, disk.overlay_exists);
            self.write_active_disk_at(&bundle, &active_disk)?;
            let metadata = SnapshotDiskCreateMetadata {
                snapshot: snapshot_name.to_string(),
                disk,
                command,
                executed: false,
                exit_status: None,
                stdout: String::new(),
                stderr: String::new(),
                created_at_unix: now_unix(),
            };
            self.write_snapshot_disk_create_metadata_at(&bundle, snapshot_name, &metadata)?;
            return Ok(metadata);
        }

        let output = run(&command[0], &command[1..]).map_err(|source| {
            StorageError::SnapshotDiskCreateIo {
                command: command.clone(),
                source,
            }
        })?;
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let status = output.status.to_string();
        if !output.status.success() {
            return Err(StorageError::SnapshotDiskCreateFailed {
                command,
                status,
                stderr,
            });
        }

        disk.overlay_exists = disk.overlay_path.exists();
        disk.backing_exists = disk.backing_path.exists();
        self.write_snapshot_disk_metadata_at(&bundle, snapshot_name, &disk)?;
        let active_disk =
            self.snapshot_overlay_active_disk_at(snapshot_name, &disk, disk.overlay_exists);
        self.write_active_disk_at(&bundle, &active_disk)?;
        let metadata = SnapshotDiskCreateMetadata {
            snapshot: snapshot_name.to_string(),
            disk,
            command,
            executed: true,
            exit_status: Some(status),
            stdout,
            stderr,
            created_at_unix: now_unix(),
        };
        self.write_snapshot_disk_create_metadata_at(&bundle, snapshot_name, &metadata)?;
        Ok(metadata)
    }

    pub fn snapshots(&self, vm_name: &str) -> Result<Vec<SnapshotMetadata>, StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        let path = bundle.join("metadata").join("snapshots.json");
        if !path.exists() {
            return Ok(Vec::new());
        }
        Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
    }

    pub fn restore_snapshot(
        &self,
        vm_name: &str,
        snapshot_name: &str,
    ) -> Result<SnapshotRestoreMetadata, StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        let snapshot = self
            .snapshots(vm_name)?
            .into_iter()
            .find(|snapshot| snapshot.name == snapshot_name)
            .ok_or_else(|| StorageError::SnapshotNotFound {
                vm: vm_name.to_string(),
                snapshot: snapshot_name.to_string(),
            })?;
        let active_disk = if snapshot.kind == SnapshotKind::Disk {
            let disk = self
                .snapshot_disk_metadata(vm_name, snapshot_name)?
                .ok_or_else(|| StorageError::SnapshotDiskMetadataNotFound {
                    vm: vm_name.to_string(),
                    snapshot: snapshot_name.to_string(),
                })?;
            if !disk.backing_path.exists() {
                return Err(StorageError::SnapshotDiskBackingMissing(disk.backing_path));
            }
            let active_disk = self.snapshot_backing_active_disk_at(snapshot_name, &disk, true);
            self.write_active_disk_at(&bundle, &active_disk)?;
            Some(active_disk)
        } else {
            None
        };
        let suspend_image = if snapshot.kind == SnapshotKind::Suspend {
            let mut suspend_image = self
                .snapshot_suspend_image_metadata(vm_name, snapshot_name)?
                .ok_or_else(|| StorageError::SnapshotSuspendImageMetadataNotFound {
                    vm: vm_name.to_string(),
                    snapshot: snapshot_name.to_string(),
                })?;
            suspend_image.image_exists = suspend_image.image_path.exists();
            self.write_snapshot_suspend_image_metadata_at(&bundle, snapshot_name, &suspend_image)?;
            if !suspend_image.image_exists {
                return Err(StorageError::SnapshotSuspendImageMissing(
                    suspend_image.image_path,
                ));
            }
            Some(suspend_image)
        } else {
            None
        };
        let restore = SnapshotRestoreMetadata {
            snapshot: snapshot.name,
            restored_at_unix: now_unix(),
            restored_state: snapshot.vm_state,
            active_disk,
            suspend_image,
        };
        self.write_state_at(&bundle, restore.restored_state)?;
        let dir = bundle.join("metadata");
        fs::create_dir_all(&dir)?;
        fs::write(
            dir.join("last-restore.json"),
            serde_json::to_string_pretty(&restore)?,
        )?;
        Ok(restore)
    }

    pub fn last_restore(
        &self,
        vm_name: &str,
    ) -> Result<Option<SnapshotRestoreMetadata>, StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        let path = bundle.join("metadata").join("last-restore.json");
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(serde_json::from_str(&fs::read_to_string(path)?)?))
    }

    pub fn runner_metadata(&self, vm_name: &str) -> Result<Option<RunnerMetadata>, StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        let path = bundle.join("metadata").join("runner.json");
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(serde_json::from_str(&fs::read_to_string(path)?)?))
    }

    pub fn guest_tools_token(
        &self,
        vm_name: &str,
    ) -> Result<GuestToolsTokenMetadata, StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        self.guest_tools_token_at(&bundle)
    }

    pub fn guest_tools_runner_metadata(
        &self,
        vm_name: &str,
    ) -> Result<GuestToolsRunnerMetadata, StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        let token = self.guest_tools_token_at(&bundle)?;
        Ok(GuestToolsRunnerMetadata {
            transport: "virtio-serial".to_string(),
            channel_name: GUEST_TOOLS_CHANNEL_NAME.to_string(),
            socket_path: guest_tools_socket_path(&bundle),
            token_path: guest_tools_token_path(&bundle),
            token_created_at_unix: token.created_at_unix,
        })
    }

    pub fn guest_tools_runtime_metadata(
        &self,
        vm_name: &str,
    ) -> Result<Option<GuestToolsRuntimeMetadata>, StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        let path = guest_tools_runtime_path(&bundle);
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(serde_json::from_str(&fs::read_to_string(path)?)?))
    }

    pub fn write_guest_tools_runtime_metadata(
        &self,
        vm_name: &str,
        metadata: &GuestToolsRuntimeMetadata,
    ) -> Result<(), StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        write_json_pretty_atomic(&guest_tools_runtime_path(&bundle), metadata)
    }

    pub fn runtime_resource_policy_metadata(
        &self,
        vm_name: &str,
    ) -> Result<Option<RuntimeResourcePolicyMetadata>, StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        let path = runtime_resource_policy_path(&bundle);
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(serde_json::from_str(&fs::read_to_string(path)?)?))
    }

    pub fn write_runtime_resource_policy_metadata(
        &self,
        vm_name: &str,
        metadata: &RuntimeResourcePolicyMetadata,
    ) -> Result<(), StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        write_json_pretty_atomic(&runtime_resource_policy_path(&bundle), metadata)
    }

    pub fn qmp_supervisor_metadata(
        &self,
        vm_name: &str,
    ) -> Result<Option<QmpSupervisorMetadata>, StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        let path = qmp_supervisor_path(&bundle);
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(serde_json::from_str(&fs::read_to_string(path)?)?))
    }

    pub fn write_qmp_supervisor_metadata(
        &self,
        vm_name: &str,
        metadata: &QmpSupervisorMetadata,
    ) -> Result<(), StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        write_json_pretty_atomic(&qmp_supervisor_path(&bundle), metadata)
    }

    pub fn live_evidence_metadata(
        &self,
        vm_name: &str,
    ) -> Result<Option<VmLiveEvidenceMetadata>, StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        let path = live_evidence_metadata_path(&bundle);
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(serde_json::from_str(&fs::read_to_string(path)?)?))
    }

    pub fn import_live_evidence_bundle(
        &self,
        vm_name: &str,
        source: &Path,
    ) -> Result<VmLiveEvidenceMetadata, StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        let source = absolutize(source.to_path_buf());
        let preserved_path = live_evidence_preserved_path(&bundle);
        let source_canonical = fs::canonicalize(&source)?;
        let preserved_parent = preserved_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        fs::create_dir_all(&preserved_parent)?;
        let preserved_parent_canonical = fs::canonicalize(&preserved_parent)?;
        if source_canonical.starts_with(&preserved_parent_canonical) {
            return Err(StorageError::UnsupportedBundleEntry(source));
        }
        if preserved_path.exists() {
            fs::remove_dir_all(&preserved_path)?;
        }
        let copy_summary = copy_dir_all(&source, &preserved_path)?;
        let metadata = VmLiveEvidenceMetadata {
            vm: vm_name.to_string(),
            source,
            preserved_path: preserved_path.clone(),
            copied_file_count: copy_summary.file_count,
            copied_files: copy_summary.files,
            recorded_at_unix: now_unix(),
        };
        write_json_pretty_atomic(&live_evidence_metadata_path(&bundle), &metadata)?;
        Ok(metadata)
    }

    pub fn clear_live_evidence_metadata(&self, vm_name: &str) -> Result<(), StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        let metadata_path = live_evidence_metadata_path(&bundle);
        if metadata_path.exists() {
            fs::remove_file(metadata_path)?;
        }
        let preserved_dir = bundle.join("metadata").join("live-evidence");
        if preserved_dir.exists() {
            fs::remove_dir_all(preserved_dir)?;
        }
        Ok(())
    }

    pub fn write_runner_metadata(
        &self,
        vm_name: &str,
        metadata: &RunnerMetadata,
    ) -> Result<(), StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        let dir = bundle.join("metadata");
        fs::create_dir_all(&dir)?;
        // Atomic (temp + rename): a torn write of runner.json would otherwise
        // leave invalid JSON that bricks every later lifecycle read.
        write_json_pretty_atomic(&dir.join("runner.json"), metadata)?;
        Ok(())
    }

    fn rebase_copied_bundle_metadata(
        &self,
        source: &Path,
        output: &Path,
        manifest: &VmManifest,
    ) -> Result<(), StorageError> {
        let mut active_disk = self.active_disk_at(output, manifest)?;
        active_disk.path = rebase_copied_path(&active_disk.path, source, output);
        active_disk.exists = active_disk.path.exists();
        self.write_active_disk_at(output, &active_disk)?;

        let snapshot_disk_metadata_dir = output.join("metadata").join("snapshot-disks");
        if snapshot_disk_metadata_dir.exists() {
            for entry in fs::read_dir(&snapshot_disk_metadata_dir)? {
                let path = entry?.path();
                if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                    continue;
                }
                if path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.ends_with("-create.json"))
                {
                    let mut metadata: SnapshotDiskCreateMetadata =
                        serde_json::from_str(&fs::read_to_string(&path)?)?;
                    rebase_snapshot_disk_metadata(&mut metadata.disk, source, output);
                    write_json_pretty_atomic(&path, &metadata)?;
                } else {
                    let mut metadata: SnapshotDiskMetadata =
                        serde_json::from_str(&fs::read_to_string(&path)?)?;
                    rebase_snapshot_disk_metadata(&mut metadata, source, output);
                    write_json_pretty_atomic(&path, &metadata)?;
                }
            }
        }

        let suspend_image_metadata_dir = output.join("metadata").join("suspend-images");
        if suspend_image_metadata_dir.exists() {
            for entry in fs::read_dir(&suspend_image_metadata_dir)? {
                let path = entry?.path();
                if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                    continue;
                }
                let mut metadata: SnapshotSuspendImageMetadata =
                    serde_json::from_str(&fs::read_to_string(&path)?)?;
                metadata.image_path = rebase_copied_path(&metadata.image_path, source, output);
                metadata.image_exists = metadata.image_path.exists();
                write_json_pretty_atomic(&path, &metadata)?;
            }
        }

        Ok(())
    }

    /// Reset a freshly-copied clone bundle so it is an independent VM rather than
    /// a duplicate of the source on the network/host.
    ///
    /// A bundle copy duplicates everything, including per-VM identity and
    /// transient runtime state. For a clone we must:
    /// - drop the persisted per-VM identity (Apple VZ machine identifier and the
    ///   NAT MAC address) so the clone regenerates fresh identity on next launch
    ///   and is not a network/identity duplicate of the source;
    /// - issue a fresh guest-tools token (the source's credential must not be
    ///   reused);
    /// - drop transient runner metadata and reset runtime state to Stopped so the
    ///   clone starts clean (no inherited pid/log pointers into the source's run);
    /// - drop any inherited Fast Mode saved-state suspend image, which is keyed
    ///   by the source's name and identity and would otherwise leave the clone
    ///   marked suspended against state it can never restore.
    ///
    /// Snapshot/disk overlay metadata is rebased separately (full clone) or
    /// dropped (linked clone) by the callers.
    fn reset_clone_runtime_identity(&self, output: &Path) -> Result<(), StorageError> {
        let metadata_dir = output.join("metadata");

        // Per-VM identity persisted by the Apple VZ runner (machine identifier +
        // NAT MAC). Removing them makes the runner mint fresh identity on the
        // clone's next launch instead of cloning the source's.
        for identity_file in [
            metadata_dir.join("machine-identifier.bin"),
            metadata_dir.join("network-mac-address.txt"),
        ] {
            if identity_file.exists() {
                fs::remove_file(&identity_file)?;
            }
        }

        // Fresh guest-tools token: the clone must not share the source's credential.
        self.write_guest_tools_token_at(output, &new_guest_tools_token()?)?;

        // Transient runner metadata points at the source's run (pid, log files).
        let runner_path = metadata_dir.join("runner.json");
        if runner_path.exists() {
            fs::remove_file(&runner_path)?;
        }

        // Fast Mode saved-state images live under metadata/suspend-images and are
        // keyed by the source's name/identity; the clone cannot restore them.
        let fast_suspend_dir = metadata_dir.join("suspend-images");
        if fast_suspend_dir.exists() {
            fs::remove_dir_all(&fast_suspend_dir)?;
        }

        // The clone starts stopped/clean regardless of the source's live state.
        self.write_state_at(output, VmRuntimeState::Stopped)?;

        Ok(())
    }

    pub fn clear_runner_metadata(&self, vm_name: &str) -> Result<(), StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        let path = bundle.join("metadata").join("runner.json");
        if path.exists() {
            fs::remove_file(path)?;
        }
        Ok(())
    }

    fn write_disk_create_metadata(
        &self,
        vm_name: &str,
        metadata: &DiskCreateMetadata,
    ) -> Result<(), StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        let dir = bundle.join("metadata");
        write_json_pretty_atomic(&dir.join("last-disk-create.json"), metadata)?;
        Ok(())
    }

    fn write_disk_inspect_metadata(
        &self,
        vm_name: &str,
        metadata: &DiskInspectMetadata,
    ) -> Result<(), StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        let dir = bundle.join("metadata");
        write_json_pretty_atomic(&dir.join("last-disk-inspect.json"), metadata)?;
        Ok(())
    }

    fn write_disk_verify_metadata(
        &self,
        vm_name: &str,
        metadata: &DiskVerifyMetadata,
    ) -> Result<(), StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        let dir = bundle.join("metadata");
        write_json_pretty_atomic(&dir.join("last-disk-verify.json"), metadata)?;
        Ok(())
    }

    fn write_disk_compact_metadata(
        &self,
        vm_name: &str,
        metadata: &DiskCompactMetadata,
    ) -> Result<(), StorageError> {
        let (bundle, _) = self.get_vm(vm_name)?;
        let dir = bundle.join("metadata");
        write_json_pretty_atomic(&dir.join("last-disk-compact.json"), metadata)?;
        Ok(())
    }

    fn state_at(&self, bundle: &Path) -> Result<VmRuntimeMetadata, StorageError> {
        let path = bundle.join("metadata").join("state.json");
        if !path.exists() {
            return self.write_state_at(bundle, VmRuntimeState::Stopped);
        }
        Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
    }

    fn write_state_at(
        &self,
        bundle: &Path,
        state: VmRuntimeState,
    ) -> Result<VmRuntimeMetadata, StorageError> {
        let metadata = VmRuntimeMetadata {
            state,
            updated_at_unix: now_unix(),
        };
        let dir = bundle.join("metadata");
        fs::create_dir_all(&dir)?;
        // Atomic (temp + rename): a torn write of state.json would otherwise
        // leave invalid JSON that bricks lifecycle reads.
        write_json_pretty_atomic(&dir.join("state.json"), &metadata)?;
        Ok(metadata)
    }

    fn write_snapshots_at(
        &self,
        bundle: &Path,
        snapshots: &[SnapshotMetadata],
    ) -> Result<(), StorageError> {
        let dir = bundle.join("metadata");
        fs::create_dir_all(&dir)?;
        write_json_pretty_atomic(&dir.join("snapshots.json"), snapshots)?;
        Ok(())
    }

    fn prepare_snapshot_disk_at(
        &self,
        bundle: &Path,
        manifest: &VmManifest,
        snapshot_name: &str,
    ) -> Result<SnapshotDiskMetadata, StorageError> {
        let active_disk = self.active_disk_at(bundle, manifest)?;
        let backing_path = active_disk.path;
        let overlay_path = bundle
            .join("disks")
            .join("snapshots")
            .join(format!("{}.qcow2", slug(snapshot_name)));
        if let Some(parent) = overlay_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let backing_format = active_disk.format;
        let create_command = QemuImgCommand::create_backed_disk(
            &overlay_path,
            "qcow2",
            backing_format.clone(),
            &backing_path,
        )
        .render_shell_words();
        let metadata = SnapshotDiskMetadata {
            snapshot: snapshot_name.to_string(),
            overlay_exists: overlay_path.exists(),
            overlay_path,
            overlay_format: "qcow2".to_string(),
            backing_path: backing_path.clone(),
            backing_format,
            backing_exists: backing_path.exists(),
            create_command,
            prepared_at_unix: now_unix(),
        };

        let path = snapshot_disk_metadata_path(bundle, snapshot_name);
        write_json_pretty_atomic(&path, &metadata)?;
        Ok(metadata)
    }

    fn write_snapshot_disk_metadata_at(
        &self,
        bundle: &Path,
        snapshot_name: &str,
        metadata: &SnapshotDiskMetadata,
    ) -> Result<(), StorageError> {
        write_json_pretty_atomic(
            &snapshot_disk_metadata_path(bundle, snapshot_name),
            metadata,
        )
    }

    fn write_snapshot_disk_create_metadata_at(
        &self,
        bundle: &Path,
        snapshot_name: &str,
        metadata: &SnapshotDiskCreateMetadata,
    ) -> Result<(), StorageError> {
        write_json_pretty_atomic(
            &snapshot_disk_create_metadata_path(bundle, snapshot_name),
            metadata,
        )
    }

    fn prepare_snapshot_suspend_image_at(
        &self,
        bundle: &Path,
        snapshot_name: &str,
    ) -> Result<SnapshotSuspendImageMetadata, StorageError> {
        let image_path = bundle
            .join("suspend-images")
            .join(format!("{}.bin", slug(snapshot_name)));
        if let Some(parent) = image_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let metadata = SnapshotSuspendImageMetadata {
            snapshot: snapshot_name.to_string(),
            image_path,
            image_format: "bridgevm-suspend-image-v1".to_string(),
            image_exists: false,
            prepared_at_unix: now_unix(),
        };
        self.write_snapshot_suspend_image_metadata_at(bundle, snapshot_name, &metadata)?;
        Ok(metadata)
    }

    fn write_snapshot_suspend_image_metadata_at(
        &self,
        bundle: &Path,
        snapshot_name: &str,
        metadata: &SnapshotSuspendImageMetadata,
    ) -> Result<(), StorageError> {
        write_json_pretty_atomic(
            &snapshot_suspend_image_metadata_path(bundle, snapshot_name),
            metadata,
        )
    }

    fn prepare_application_consistent_snapshot_preflight_at(
        &self,
        bundle: &Path,
        snapshot_name: &str,
    ) -> Result<ApplicationConsistentSnapshotPreflightMetadata, StorageError> {
        let runtime_path = guest_tools_runtime_path(bundle);
        let runtime: Option<GuestToolsRuntimeMetadata> = if runtime_path.exists() {
            Some(serde_json::from_str(&fs::read_to_string(runtime_path)?)?)
        } else {
            None
        };
        let required_capabilities = application_consistent_snapshot_required_capabilities();
        let available_capabilities = runtime
            .as_ref()
            .map(|runtime| runtime.capabilities.clone())
            .unwrap_or_default();
        let missing_capabilities = required_capabilities
            .iter()
            .filter(|required| {
                !available_capabilities
                    .iter()
                    .any(|available| available == *required)
            })
            .cloned()
            .collect::<Vec<_>>();
        let connected = runtime.as_ref().is_some_and(|runtime| runtime.connected);
        let ready = connected && missing_capabilities.is_empty();
        let metadata = ApplicationConsistentSnapshotPreflightMetadata {
            snapshot: snapshot_name.to_string(),
            connected,
            required_capabilities,
            available_capabilities,
            missing_capabilities,
            ready,
            planned_freeze_semantics: APPLICATION_CONSISTENT_FREEZE_SEMANTICS.to_string(),
            planned_thaw_semantics: APPLICATION_CONSISTENT_THAW_SEMANTICS.to_string(),
            runtime_updated_at_unix: runtime.as_ref().map(|runtime| runtime.updated_at_unix),
            prepared_at_unix: now_unix(),
        };
        write_json_pretty_atomic(
            &application_consistent_snapshot_preflight_path(bundle, snapshot_name),
            &metadata,
        )?;
        Ok(metadata)
    }

    fn active_disk_at(
        &self,
        bundle: &Path,
        manifest: &VmManifest,
    ) -> Result<ActiveDiskMetadata, StorageError> {
        let path = bundle.join("metadata").join("active-disk.json");
        if !path.exists() {
            return Ok(self.primary_active_disk_at(bundle, manifest));
        }

        let mut active_disk: ActiveDiskMetadata = serde_json::from_str(&fs::read_to_string(path)?)?;
        active_disk.exists = active_disk.path.exists();
        Ok(active_disk)
    }

    fn primary_active_disk_at(&self, bundle: &Path, manifest: &VmManifest) -> ActiveDiskMetadata {
        let path = resolve_bundle_path(bundle, &manifest.storage.primary.path);
        ActiveDiskMetadata {
            source: ActiveDiskSource::Primary,
            snapshot: None,
            exists: path.exists(),
            path,
            format: manifest.storage.primary.format.clone(),
            activated_at_unix: now_unix(),
        }
    }

    fn snapshot_overlay_active_disk_at(
        &self,
        snapshot_name: &str,
        disk: &SnapshotDiskMetadata,
        exists: bool,
    ) -> ActiveDiskMetadata {
        ActiveDiskMetadata {
            source: ActiveDiskSource::SnapshotOverlay,
            snapshot: Some(snapshot_name.to_string()),
            path: disk.overlay_path.clone(),
            format: disk.overlay_format.clone(),
            exists,
            activated_at_unix: now_unix(),
        }
    }

    fn snapshot_backing_active_disk_at(
        &self,
        snapshot_name: &str,
        disk: &SnapshotDiskMetadata,
        exists: bool,
    ) -> ActiveDiskMetadata {
        ActiveDiskMetadata {
            source: ActiveDiskSource::SnapshotBacking,
            snapshot: Some(snapshot_name.to_string()),
            path: disk.backing_path.clone(),
            format: disk.backing_format.clone(),
            exists,
            activated_at_unix: now_unix(),
        }
    }

    fn write_active_disk_at(
        &self,
        bundle: &Path,
        metadata: &ActiveDiskMetadata,
    ) -> Result<(), StorageError> {
        write_json_pretty_atomic(&bundle.join("metadata").join("active-disk.json"), metadata)
    }

    fn guest_tools_token_at(&self, bundle: &Path) -> Result<GuestToolsTokenMetadata, StorageError> {
        let path = guest_tools_token_path(bundle);
        if path.exists() {
            return Ok(serde_json::from_str(&fs::read_to_string(path)?)?);
        }

        let metadata = new_guest_tools_token()?;
        self.write_guest_tools_token_at(bundle, &metadata)?;
        Ok(metadata)
    }

    fn write_guest_tools_token_at(
        &self,
        bundle: &Path,
        metadata: &GuestToolsTokenMetadata,
    ) -> Result<(), StorageError> {
        write_json_pretty_atomic(&guest_tools_token_path(bundle), metadata)
    }
}

fn validate_transition(from: VmRuntimeState, to: VmRuntimeState) -> Result<(), StorageError> {
    let valid = matches!(
        (from, to),
        (VmRuntimeState::Stopped, VmRuntimeState::Running)
            | (VmRuntimeState::Running, VmRuntimeState::Stopped)
            | (VmRuntimeState::Running, VmRuntimeState::Suspended)
            | (VmRuntimeState::Suspended, VmRuntimeState::Running)
            | (VmRuntimeState::Suspended, VmRuntimeState::Stopped)
    ) || from == to;

    if valid {
        Ok(())
    } else {
        Err(StorageError::InvalidStateTransition { from, to })
    }
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn duration_micros_u64(duration: Duration) -> u64 {
    duration.as_micros().min(u128::from(u64::MAX)) as u64
}

fn read_json_file<T: DeserializeOwned>(path: &Path) -> Result<Option<T>, StorageError> {
    if !path.exists() {
        return Ok(None);
    }
    Ok(Some(serde_json::from_str(&fs::read_to_string(path)?)?))
}

fn metadata_repair_action(
    path: &Path,
    action: impl Into<String>,
    detail: impl Into<String>,
) -> MetadataRepairAction {
    MetadataRepairAction {
        path: path.to_path_buf(),
        action: action.into(),
        detail: detail.into(),
    }
}

fn primary_disk_preparation_metadata(
    bundle: &Path,
    manifest: &VmManifest,
) -> DiskPreparationMetadata {
    let path = resolve_bundle_path(bundle, &manifest.storage.primary.path);
    let format = manifest.storage.primary.format.clone();
    let size = manifest.storage.primary.size.clone();
    let exists = path.exists();
    let create_command = if !exists && format != "raw" {
        Some(QemuImgCommand::create_disk(&path, format.clone(), size.clone()).render_shell_words())
    } else {
        None
    };
    DiskPreparationMetadata {
        path,
        format,
        size: size.clone(),
        size_bytes: parse_size_bytes(&size),
        exists,
        created: false,
        create_command,
        prepared_at_unix: now_unix(),
    }
}

fn new_guest_tools_token() -> Result<GuestToolsTokenMetadata, StorageError> {
    let mut bytes = [0_u8; 32];
    fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(GuestToolsTokenMetadata {
        token: hex_encode(&bytes),
        created_at_unix: now_unix(),
    })
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BundleCopySummary {
    file_count: u64,
    files: Vec<String>,
    manifest_preserved: bool,
    metadata_preserved: bool,
}

fn summarize_bundle_copy(from: &Path) -> Result<BundleCopySummary, StorageError> {
    let mut files = Vec::new();
    collect_regular_files(from, from, &mut files)?;
    files.sort();
    Ok(BundleCopySummary {
        file_count: files.len() as u64,
        manifest_preserved: files.iter().any(|path| path == "manifest.yaml"),
        metadata_preserved: files.iter().any(|path| path.starts_with("metadata/")),
        files,
    })
}

fn collect_regular_files(
    root: &Path,
    current: &Path,
    files: &mut Vec<String>,
) -> Result<(), StorageError> {
    let metadata = fs::symlink_metadata(current)?;
    if !metadata.file_type().is_dir() {
        return Err(StorageError::UnsupportedBundleEntry(current.to_path_buf()));
    }
    let mut entries = fs::read_dir(current)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        if should_skip_bundle_copy_path(&path) {
            continue;
        }
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_regular_files(root, &path, files)?;
        } else if file_type.is_file() {
            let relative = path
                .strip_prefix(root)
                .map_err(|_| StorageError::UnsupportedBundleEntry(path.clone()))?;
            files.push(relative.to_string_lossy().replace('\\', "/"));
        } else {
            return Err(StorageError::UnsupportedBundleEntry(path));
        }
    }
    Ok(())
}

fn copy_dir_all(from: &Path, to: &Path) -> Result<BundleCopySummary, StorageError> {
    let metadata = fs::symlink_metadata(from)?;
    if !metadata.file_type().is_dir() {
        return Err(StorageError::UnsupportedBundleEntry(from.to_path_buf()));
    }
    fs::create_dir_all(to)?;
    let mut copied_files = Vec::new();
    copy_dir_all_inner(from, from, to, &mut copied_files)?;
    copied_files.sort();
    Ok(BundleCopySummary {
        file_count: copied_files.len() as u64,
        manifest_preserved: copied_files.iter().any(|path| path == "manifest.yaml"),
        metadata_preserved: copied_files
            .iter()
            .any(|path| path.starts_with("metadata/")),
        files: copied_files,
    })
}

fn copy_dir_all_inner(
    root: &Path,
    from: &Path,
    to: &Path,
    copied_files: &mut Vec<String>,
) -> Result<(), StorageError> {
    let mut entries = fs::read_dir(from)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let from_path = entry.path();
        if should_skip_bundle_copy_path(&from_path) {
            continue;
        }
        let to_path = to.join(entry.file_name());
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            fs::create_dir_all(&to_path)?;
            copy_dir_all_inner(root, &from_path, &to_path, copied_files)?;
        } else if file_type.is_file() {
            fs::copy(&from_path, &to_path)?;
            let relative = from_path
                .strip_prefix(root)
                .map_err(|_| StorageError::UnsupportedBundleEntry(from_path.clone()))?;
            copied_files.push(relative.to_string_lossy().replace('\\', "/"));
        } else {
            return Err(StorageError::UnsupportedBundleEntry(from_path));
        }
    }
    Ok(())
}

fn should_skip_bundle_copy_path(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with(".sock") || name.ends_with(".lock"))
}

fn rebase_copied_path(path: &Path, source: &Path, output: &Path) -> PathBuf {
    path.strip_prefix(source)
        .map(|relative| output.join(relative))
        .unwrap_or_else(|_| path.to_path_buf())
}

fn rebase_snapshot_disk_metadata(
    metadata: &mut SnapshotDiskMetadata,
    source: &Path,
    output: &Path,
) {
    metadata.overlay_path = rebase_copied_path(&metadata.overlay_path, source, output);
    metadata.backing_path = rebase_copied_path(&metadata.backing_path, source, output);
    metadata.overlay_exists = metadata.overlay_path.exists();
    metadata.backing_exists = metadata.backing_path.exists();
    metadata.create_command = QemuImgCommand::create_backed_disk(
        &metadata.overlay_path,
        metadata.overlay_format.clone(),
        metadata.backing_format.clone(),
        &metadata.backing_path,
    )
    .render_shell_words();
}

fn export_bundle_tar(
    source: &Path,
    output: &Path,
    metadata: &VmExportMetadata,
) -> Result<(), StorageError> {
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    let staging = unique_temp_path("bridgevm-export-tar");
    let _staging_guard = TempDirGuard::new(staging.clone());
    copy_dir_all(source, &staging)?;
    let metadata_dir = staging.join("metadata");
    fs::create_dir_all(&metadata_dir)?;
    fs::write(
        metadata_dir.join("export.json"),
        serde_json::to_string_pretty(metadata)?,
    )?;

    let file = fs::File::create(output)?;
    let mut builder = tar::Builder::new(file);
    builder.append_dir_all(".", &staging)?;
    builder.finish()?;
    Ok(())
}

fn extract_bundle_tar(input: &Path, output: &Path) -> Result<(), StorageError> {
    fs::create_dir_all(output)?;
    let file = fs::File::open(input)?;
    let mut archive = tar::Archive::new(file);
    for entry in archive.entries()? {
        let mut entry = entry?;
        let raw_path = entry.path()?.into_owned();
        let Some(relative_path) = safe_archive_path(&raw_path) else {
            return Err(StorageError::UnsafeArchiveEntry(raw_path));
        };
        if relative_path.as_os_str().is_empty() {
            continue;
        }
        let destination = output.join(&relative_path);
        let entry_type = entry.header().entry_type();
        if entry_type.is_dir() {
            fs::create_dir_all(&destination)?;
        } else if entry_type.is_file() {
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)?;
            }
            entry.unpack(&destination)?;
        } else {
            return Err(StorageError::UnsupportedBundleEntry(raw_path));
        }
    }
    Ok(())
}

fn safe_archive_path(path: &Path) -> Option<PathBuf> {
    let mut safe = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::Normal(name) => safe.push(name),
            std::path::Component::ParentDir
            | std::path::Component::RootDir
            | std::path::Component::Prefix(_) => return None,
        }
    }
    Some(safe)
}

fn is_tar_path(path: &Path) -> bool {
    path.extension().and_then(|extension| extension.to_str()) == Some("tar")
}

fn is_unsupported_archive_path(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("zip" | "tgz" | "gz")
    ) || name.ends_with(".tar.gz")
}

fn unique_temp_path(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    std::env::temp_dir().join(format!("{prefix}-{}-{nanos}", std::process::id()))
}

struct TempDirGuard {
    path: PathBuf,
}

impl TempDirGuard {
    fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn resolve_path_for_new(path: &Path) -> Result<PathBuf, StorageError> {
    if path.exists() {
        return Ok(fs::canonicalize(path)?);
    }

    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()?.join(path)
    };
    let mut existing = absolute.as_path();
    let mut missing = Vec::new();
    while !existing.exists() {
        if let Some(name) = existing.file_name() {
            missing.push(name.to_os_string());
        }
        existing = existing.parent().unwrap_or_else(|| Path::new("."));
    }

    let mut resolved = fs::canonicalize(existing)?;
    for component in missing.iter().rev() {
        if component == OsStr::new(".") {
            continue;
        }
        if component == OsStr::new("..") {
            resolved.pop();
        } else {
            resolved.push(component);
        }
    }
    Ok(resolved)
}

fn is_same_or_descendant(path: &Path, ancestor: &Path) -> bool {
    path == ancestor || path.starts_with(ancestor)
}

fn write_json_pretty_atomic<T: Serialize + ?Sized>(
    path: &Path,
    value: &T,
) -> Result<(), StorageError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let tmp = parent.join(format!(
        ".{}.tmp-{}-{}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("metadata"),
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::write(&tmp, serde_json::to_string_pretty(value)?)?;
    fs::rename(tmp, path)?;
    Ok(())
}

fn resolve_bundle_path(bundle_path: &Path, relative_or_absolute: &str) -> PathBuf {
    let path = PathBuf::from(relative_or_absolute);
    if path.is_absolute() {
        path
    } else {
        bundle_path.join(path)
    }
}

fn absolutize(path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        env::current_dir()
            .map(|cwd| cwd.join(&path))
            .unwrap_or(path)
    }
}

fn snapshot_disk_metadata_path(bundle: &Path, snapshot_name: &str) -> PathBuf {
    bundle
        .join("metadata")
        .join("snapshot-disks")
        .join(format!("{}.json", slug(snapshot_name)))
}

fn snapshot_disk_create_metadata_path(bundle: &Path, snapshot_name: &str) -> PathBuf {
    bundle
        .join("metadata")
        .join("snapshot-disks")
        .join(format!("{}-create.json", slug(snapshot_name)))
}

fn snapshot_suspend_image_metadata_path(bundle: &Path, snapshot_name: &str) -> PathBuf {
    bundle
        .join("metadata")
        .join("suspend-images")
        .join(format!("{}.json", slug(snapshot_name)))
}

fn fast_suspend_image_metadata_path(bundle: &Path, vm_name: &str) -> PathBuf {
    bundle
        .join("metadata")
        .join("suspend-images")
        .join(format!("{}.fast.json", slug(vm_name)))
}

fn application_consistent_snapshot_preflight_path(bundle: &Path, snapshot_name: &str) -> PathBuf {
    bundle
        .join("metadata")
        .join("application-consistent-snapshots")
        .join(format!("{}.json", slug(snapshot_name)))
}

fn guest_tools_token_path(bundle: &Path) -> PathBuf {
    bundle.join("metadata").join("guest-tools-token.json")
}

fn guest_tools_runtime_path(bundle: &Path) -> PathBuf {
    bundle.join("metadata").join("guest-tools-runtime.json")
}

fn runtime_resource_policy_path(bundle: &Path) -> PathBuf {
    bundle.join("metadata").join("runtime-resources.json")
}

fn deletion_metadata_path(bundle: &Path) -> PathBuf {
    bundle.join("metadata").join("deletion.json")
}

fn deletion_metadata_at(bundle: &Path) -> Result<Option<VmDeletionMetadata>, StorageError> {
    read_json_file(&deletion_metadata_path(bundle))
}

fn qmp_supervisor_path(bundle: &Path) -> PathBuf {
    bundle.join("metadata").join("qmp-supervisor.json")
}

fn live_evidence_metadata_path(bundle: &Path) -> PathBuf {
    bundle.join("metadata").join("live-evidence.json")
}

fn live_evidence_preserved_path(bundle: &Path) -> PathBuf {
    bundle.join("metadata").join("live-evidence").join("latest")
}

fn application_consistent_snapshot_required_capabilities() -> Vec<String> {
    vec!["fs-freeze".to_string(), "fs-thaw".to_string()]
}

const GUEST_TOOLS_CHANNEL_NAME: &str = "org.bridgevm.guest-tools.0";

fn parse_size_bytes(value: &str) -> Option<u64> {
    let trimmed = value.trim();
    let units = [
        ("GiB", 1024_u64.pow(3)),
        ("G", 1024_u64.pow(3)),
        ("MiB", 1024_u64.pow(2)),
        ("M", 1024_u64.pow(2)),
        ("KiB", 1024),
        ("K", 1024),
        ("B", 1),
    ];
    for (suffix, multiplier) in units {
        if let Some(number) = trimmed.strip_suffix(suffix) {
            // checked_mul: a huge value would otherwise panic (debug) or wrap
            // (release) into a wrong set_len size. Overflow -> None.
            return number
                .trim()
                .parse::<u64>()
                .ok()
                .and_then(|n| n.checked_mul(multiplier));
        }
    }
    trimmed.parse::<u64>().ok()
}

fn run_command(program: &str, args: &[String]) -> Result<Output, std::io::Error> {
    Command::new(program).args(args).output()
}

struct MetadataLock {
    path: PathBuf,
}

impl MetadataLock {
    fn acquire(bundle: &Path, name: &str) -> Result<Self, StorageError> {
        let path = bundle.join("metadata").join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        for _ in 0..100 {
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(_) => return Ok(Self { path }),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    sleep(Duration::from_millis(10));
                }
                Err(error) => return Err(StorageError::Io(error)),
            }
        }
        Err(StorageError::Io(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            format!("timed out waiting for metadata lock {}", path.display()),
        )))
    }
}

impl Drop for MetadataLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bridgevm_config::{Guest, VmManifest, VmMode};
    use std::io::Write;
    use std::os::unix::process::ExitStatusExt;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_ID: AtomicU64 = AtomicU64::new(0);

    fn temp_store() -> VmStore {
        let mut path = std::env::temp_dir();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let id = TEST_ID.fetch_add(1, Ordering::Relaxed);
        path.push(format!(
            "bridgevm-storage-test-{}-{}-{}",
            std::process::id(),
            nanos,
            id
        ));
        VmStore::new(path)
    }

    fn manifest(name: &str) -> VmManifest {
        VmManifest::new(
            name,
            VmMode::Fast,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "arm64".to_string(),
            },
            "80GiB",
        )
    }

    fn write_raw_tar_entry(
        path: &Path,
        entry_name: &str,
        typeflag: u8,
        link_name: Option<&str>,
        contents: &[u8],
    ) {
        let mut header = [0_u8; 512];
        write_tar_field(&mut header[0..100], entry_name.as_bytes());
        write_tar_octal(&mut header[100..108], 0o644);
        write_tar_octal(&mut header[108..116], 0);
        write_tar_octal(&mut header[116..124], 0);
        write_tar_octal(&mut header[124..136], contents.len() as u64);
        write_tar_octal(&mut header[136..148], 0);
        header[148..156].fill(b' ');
        header[156] = typeflag;
        if let Some(link_name) = link_name {
            write_tar_field(&mut header[157..257], link_name.as_bytes());
        }
        write_tar_field(&mut header[257..263], b"ustar\0");
        write_tar_field(&mut header[263..265], b"00");
        let checksum: u32 = header.iter().map(|byte| u32::from(*byte)).sum();
        write_tar_checksum(&mut header[148..156], checksum);

        let mut file = fs::File::create(path).unwrap();
        file.write_all(&header).unwrap();
        file.write_all(contents).unwrap();
        let padding = (512 - (contents.len() % 512)) % 512;
        file.write_all(&vec![0_u8; padding]).unwrap();
        file.write_all(&[0_u8; 1024]).unwrap();
    }

    fn write_tar_field(field: &mut [u8], value: &[u8]) {
        let len = value.len().min(field.len());
        field[..len].copy_from_slice(&value[..len]);
    }

    fn write_tar_octal(field: &mut [u8], value: u64) {
        let encoded = format!("{:0width$o}\0", value, width = field.len() - 1);
        write_tar_field(field, encoded.as_bytes());
    }

    fn write_tar_checksum(field: &mut [u8], value: u32) {
        let encoded = format!("{value:06o}\0 ");
        write_tar_field(field, encoded.as_bytes());
    }

    #[test]
    fn creates_state_and_snapshot_metadata() {
        let store = temp_store();
        store.create_vm(&manifest("dev")).unwrap();

        let state = store.state("dev").unwrap();
        assert_eq!(state.state, VmRuntimeState::Stopped);
        let chain = store.snapshot_chain("dev").unwrap();
        assert_eq!(chain.active_disk.source, ActiveDiskSource::Primary);
        assert!(chain.disks.is_empty());
        let token = store.guest_tools_token("dev").unwrap();
        assert_eq!(token.token.len(), 64);
        assert!(token
            .token
            .chars()
            .all(|character| character.is_ascii_hexdigit()));
        assert_eq!(store.guest_tools_token("dev").unwrap(), token);
        assert!(store
            .bundle_path("dev")
            .join("metadata")
            .join("guest-tools-token.json")
            .exists());

        let state = store
            .transition_state("dev", VmRuntimeState::Running)
            .unwrap();
        assert_eq!(state.state, VmRuntimeState::Running);

        let snapshot = store
            .create_snapshot("dev", "before-upgrade", SnapshotKind::Disk)
            .unwrap();
        assert_eq!(snapshot.vm_state, VmRuntimeState::Running);
        assert_eq!(store.snapshots("dev").unwrap().len(), 1);
        let disk = store
            .snapshot_disk_metadata("dev", "before-upgrade")
            .unwrap()
            .expect("disk snapshot metadata");
        assert_eq!(disk.snapshot, "before-upgrade");
        assert_eq!(disk.overlay_format, "qcow2");
        assert!(!disk.overlay_exists);
        assert_eq!(disk.backing_format, "qcow2");
        assert!(disk
            .overlay_path
            .ends_with("disks/snapshots/before-upgrade.qcow2"));
        assert_eq!(
            disk.create_command[..7],
            ["qemu-img", "create", "-f", "qcow2", "-F", "qcow2", "-b"]
        );
        fs::write(&disk.backing_path, b"fake backing").unwrap();

        let restore = store.restore_snapshot("dev", "before-upgrade").unwrap();
        assert_eq!(restore.snapshot, "before-upgrade");
        assert_eq!(restore.restored_state, VmRuntimeState::Running);
        assert_eq!(
            restore.active_disk.as_ref().map(|disk| disk.source),
            Some(ActiveDiskSource::SnapshotBacking)
        );
        assert_eq!(store.last_restore("dev").unwrap(), Some(restore));
    }

    #[test]
    fn metadata_only_delete_preserves_bundle_manifest_and_hides_vm() {
        let store = temp_store();
        let bundle = store.create_vm(&manifest("dev")).unwrap();
        fs::write(bundle.join("disks").join("primary.img"), "disk").unwrap();
        fs::write(bundle.join("logs").join("serial.log"), "log").unwrap();

        let deletion = store.delete_vm_metadata_only("dev").unwrap();

        assert_eq!(deletion.vm, "dev");
        assert!(deletion.metadata_only);
        assert_eq!(deletion.bundle, bundle);
        assert!(bundle.exists());
        assert!(bundle.join("manifest.yaml").exists());
        assert!(bundle.join("disks").join("primary.img").exists());
        assert!(bundle.join("logs").join("serial.log").exists());
        assert!(bundle
            .join("metadata")
            .join("deleted-manifest.yaml")
            .exists());
        assert!(bundle.join("metadata").join("deletion.json").exists());
        assert!(store.list_vms().unwrap().is_empty());
        assert!(matches!(
            store.get_vm("dev").unwrap_err(),
            StorageError::NotFound(name) if name == "dev"
        ));
    }

    #[test]
    fn corrupt_deletion_metadata_surfaces_on_list_and_get() {
        let store = temp_store();
        let bundle = store.create_vm(&manifest("dev")).unwrap();
        fs::write(deletion_metadata_path(&bundle), "{not json").unwrap();

        assert!(matches!(store.list_vms(), Err(StorageError::Json(_))));
        assert!(matches!(store.get_vm("dev"), Err(StorageError::Json(_))));
    }

    #[test]
    fn guest_tools_runner_metadata_points_at_transport_files_without_token_value() {
        let store = temp_store();
        store.create_vm(&manifest("dev")).unwrap();

        let token = store.guest_tools_token("dev").unwrap();
        let metadata = store.guest_tools_runner_metadata("dev").unwrap();
        let bundle = store.bundle_path("dev");

        assert_eq!(metadata.transport, "virtio-serial");
        assert_eq!(metadata.channel_name, "org.bridgevm.guest-tools.0");
        assert_eq!(
            metadata.socket_path,
            bundle.join("metadata").join("guest-tools.sock")
        );
        assert_eq!(
            metadata.token_path,
            bundle.join("metadata").join("guest-tools-token.json")
        );
        assert_eq!(metadata.token_created_at_unix, token.created_at_unix);
        assert_ne!(metadata.socket_path.display().to_string(), token.token);
        assert_ne!(metadata.token_path.display().to_string(), token.token);
    }

    #[test]
    fn writes_guest_tools_runtime_metadata() {
        let store = temp_store();
        store.create_vm(&manifest("dev")).unwrap();
        let metadata = GuestToolsRuntimeMetadata {
            connected: true,
            guest_os: Some("linux".to_string()),
            agent_version: Some("1.0.0".to_string()),
            capabilities: vec!["heartbeat".to_string(), "guest-ip".to_string()],
            last_heartbeat_at_unix: Some(now_unix()),
            guest_ip_addresses: vec![GuestToolsIpAddressMetadata {
                address: "10.0.2.15".to_string(),
                interface: Some("eth0".to_string()),
            }],
            shared_folders: vec![GuestToolsSharedFolderMetadata {
                name: "workspace".to_string(),
                host_path_token: "share-token-1".to_string(),
                mounted_at_unix: now_unix(),
            }],
            metrics: Some(GuestToolsMetricsMetadata {
                cpu_percent: 12,
                memory_used_mib: 256,
                updated_at_unix: now_unix(),
            }),
            last_command_result: Some(GuestToolsCommandResultMetadata {
                request_id: "clipboard-1".to_string(),
                capability: Some("clipboard".to_string()),
                ok: true,
                error_code: None,
                message: Some("accepted".to_string()),
                result: Some(serde_json::json!({
                    "text_length": 8,
                    "changed": true
                })),
                metadata: Some(serde_json::json!({
                    "handler": "clipboard"
                })),
                completed_at_unix: now_unix(),
            }),
            agent_update: Some(GuestToolsAgentUpdateMetadata {
                current_version: "1.0.0".to_string(),
                available_version: "1.1.0".to_string(),
                download_url: Some("https://updates.example/bridgevm-tools".to_string()),
                signature: Some("sig".to_string()),
                observed_at_unix: now_unix(),
            }),
            clipboard: Some(GuestToolsClipboardMetadata {
                text: "guest text".to_string(),
                updated_at_unix: now_unix(),
            }),
            updated_at_unix: now_unix(),
        };

        assert_eq!(store.guest_tools_runtime_metadata("dev").unwrap(), None);
        store
            .write_guest_tools_runtime_metadata("dev", &metadata)
            .unwrap();
        assert_eq!(
            store.guest_tools_runtime_metadata("dev").unwrap(),
            Some(metadata)
        );
    }

    #[test]
    fn imports_live_evidence_bundle_into_vm_metadata() {
        let store = temp_store();
        let bundle = store.create_vm(&manifest("dev")).unwrap();
        let source = store.root().join("source-live-evidence");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("SUMMARY.txt"), "live evidence").unwrap();
        fs::write(source.join("viewer-frame.png"), "frame").unwrap();

        assert_eq!(store.live_evidence_metadata("dev").unwrap(), None);
        let metadata = store.import_live_evidence_bundle("dev", &source).unwrap();

        assert_eq!(metadata.vm, "dev");
        assert_eq!(metadata.source, source);
        assert_eq!(
            metadata.preserved_path,
            bundle.join("metadata").join("live-evidence").join("latest")
        );
        assert!(metadata.preserved_path.join("SUMMARY.txt").exists());
        assert!(metadata.preserved_path.join("viewer-frame.png").exists());
        assert!(metadata.copied_files.contains(&"SUMMARY.txt".to_string()));
        assert_eq!(store.live_evidence_metadata("dev").unwrap(), Some(metadata));

        store.clear_live_evidence_metadata("dev").unwrap();
        assert_eq!(store.live_evidence_metadata("dev").unwrap(), None);
        assert!(!bundle.join("metadata").join("live-evidence").exists());
    }

    #[test]
    fn reads_legacy_guest_tools_runtime_without_shared_folders() {
        let store = temp_store();
        let bundle = store.create_vm(&manifest("dev")).unwrap();
        fs::create_dir_all(bundle.join("metadata")).unwrap();
        fs::write(
            guest_tools_runtime_path(&bundle),
            r#"{
  "connected": true,
  "guest_os": "linux",
  "agent_version": "1.0.0",
  "capabilities": ["heartbeat"],
  "last_heartbeat_at_unix": 1,
  "guest_ip_addresses": [],
  "metrics": null,
  "updated_at_unix": 2
}"#,
        )
        .unwrap();

        let runtime = store
            .guest_tools_runtime_metadata("dev")
            .unwrap()
            .expect("runtime metadata");

        assert!(runtime.connected);
        assert!(runtime.shared_folders.is_empty());
        assert!(runtime.last_command_result.is_none());
        assert!(runtime.clipboard.is_none());
    }

    #[test]
    fn writes_qmp_supervisor_metadata() {
        let store = temp_store();
        store.create_vm(&manifest("dev")).unwrap();
        let metadata = QmpSupervisorMetadata {
            events: vec![QmpEvent {
                name: "BLOCK_JOB_COMPLETED".to_string(),
                data: Some(serde_json::json!({"device":"drive0"})),
            }],
            terminal_event: None,
            envelopes_read: 1,
            limit_reached: false,
            updated_at_unix: now_unix(),
        };

        assert_eq!(store.qmp_supervisor_metadata("dev").unwrap(), None);
        store
            .write_qmp_supervisor_metadata("dev", &metadata)
            .unwrap();
        assert_eq!(
            store.qmp_supervisor_metadata("dev").unwrap(),
            Some(metadata)
        );
        assert!(store
            .bundle_path("dev")
            .join("metadata")
            .join("qmp-supervisor.json")
            .exists());
    }

    #[test]
    fn rejects_duplicate_and_missing_snapshots() {
        let store = temp_store();
        store.create_vm(&manifest("dev")).unwrap();
        store
            .create_snapshot("dev", "before-upgrade", SnapshotKind::Disk)
            .unwrap();

        let duplicate = store
            .create_snapshot("dev", "before-upgrade", SnapshotKind::Disk)
            .unwrap_err();
        assert!(matches!(
            duplicate,
            StorageError::SnapshotAlreadyExists { .. }
        ));

        let missing = store.restore_snapshot("dev", "missing").unwrap_err();
        assert!(matches!(missing, StorageError::SnapshotNotFound { .. }));
    }

    #[test]
    fn suspend_snapshot_does_not_prepare_disk_chain_metadata() {
        let store = temp_store();
        store.create_vm(&manifest("dev")).unwrap();
        store
            .create_snapshot("dev", "paused", SnapshotKind::Suspend)
            .unwrap();

        assert!(store
            .snapshot_disk_metadata("dev", "paused")
            .unwrap()
            .is_none());
        let image = store
            .snapshot_suspend_image_metadata("dev", "paused")
            .unwrap()
            .expect("suspend image metadata");
        assert_eq!(image.snapshot, "paused");
        assert_eq!(image.image_format, "bridgevm-suspend-image-v1");
        assert!(!image.image_exists);
        assert!(image.image_path.ends_with("suspend-images/paused.bin"));
        assert!(store
            .bundle_path("dev")
            .join("metadata")
            .join("suspend-images")
            .join("paused.json")
            .exists());
    }

    #[test]
    fn application_consistent_snapshot_records_not_ready_preflight_without_runtime() {
        let store = temp_store();
        store.create_vm(&manifest("dev")).unwrap();
        let snapshot = store
            .create_snapshot("dev", "app-ready", SnapshotKind::ApplicationConsistent)
            .unwrap();

        assert_eq!(snapshot.kind, SnapshotKind::ApplicationConsistent);
        assert!(store
            .snapshot_disk_metadata("dev", "app-ready")
            .unwrap()
            .is_none());
        let preflight = store
            .application_consistent_snapshot_preflight_metadata("dev", "app-ready")
            .unwrap()
            .expect("application-consistent preflight metadata");
        assert!(!preflight.connected);
        assert!(!preflight.ready);
        assert_eq!(
            preflight.required_capabilities,
            vec!["fs-freeze".to_string(), "fs-thaw".to_string()]
        );
        assert_eq!(
            preflight.missing_capabilities,
            vec!["fs-freeze".to_string(), "fs-thaw".to_string()]
        );
        assert!(preflight.available_capabilities.is_empty());
        assert!(preflight.runtime_updated_at_unix.is_none());
        assert!(preflight
            .planned_freeze_semantics
            .contains("daemon-owned guest-tools fs-freeze request"));
        assert!(preflight
            .planned_thaw_semantics
            .contains("daemon-owned guest-tools fs-thaw request"));
        assert!(store
            .bundle_path("dev")
            .join("metadata")
            .join("application-consistent-snapshots")
            .join("app-ready.json")
            .exists());
    }

    #[test]
    fn application_consistent_snapshot_records_ready_preflight_from_runtime() {
        let store = temp_store();
        store.create_vm(&manifest("dev")).unwrap();
        let runtime = GuestToolsRuntimeMetadata {
            connected: true,
            guest_os: Some("linux".to_string()),
            agent_version: Some("1.0.0".to_string()),
            capabilities: vec![
                "heartbeat".to_string(),
                "fs-freeze".to_string(),
                "fs-thaw".to_string(),
            ],
            last_heartbeat_at_unix: Some(3),
            guest_ip_addresses: Vec::new(),
            shared_folders: Vec::new(),
            metrics: None,
            last_command_result: None,
            agent_update: None,
            clipboard: None,
            updated_at_unix: 4,
        };
        store
            .write_guest_tools_runtime_metadata("dev", &runtime)
            .unwrap();

        store
            .create_snapshot("dev", "app-ready", SnapshotKind::ApplicationConsistent)
            .unwrap();
        let preflight = store
            .application_consistent_snapshot_preflight_metadata("dev", "app-ready")
            .unwrap()
            .expect("application-consistent preflight metadata");

        assert!(preflight.connected);
        assert!(preflight.ready);
        assert!(preflight.missing_capabilities.is_empty());
        assert_eq!(preflight.runtime_updated_at_unix, Some(4));
        assert_eq!(preflight.available_capabilities, runtime.capabilities);
    }

    #[test]
    fn restore_suspend_snapshot_requires_recorded_image() {
        let store = temp_store();
        store.create_vm(&manifest("dev")).unwrap();
        store
            .transition_state("dev", VmRuntimeState::Running)
            .unwrap();
        store
            .transition_state("dev", VmRuntimeState::Suspended)
            .unwrap();
        store
            .create_snapshot("dev", "paused", SnapshotKind::Suspend)
            .unwrap();

        let missing = store.restore_snapshot("dev", "paused").unwrap_err();
        assert!(matches!(
            missing,
            StorageError::SnapshotSuspendImageMissing(_)
        ));
        assert!(
            !store
                .snapshot_suspend_image_metadata("dev", "paused")
                .unwrap()
                .unwrap()
                .image_exists
        );
    }

    #[test]
    fn restore_suspend_snapshot_records_suspend_image_metadata() {
        let store = temp_store();
        store.create_vm(&manifest("dev")).unwrap();
        store
            .transition_state("dev", VmRuntimeState::Running)
            .unwrap();
        store
            .transition_state("dev", VmRuntimeState::Suspended)
            .unwrap();
        store
            .create_snapshot("dev", "paused", SnapshotKind::Suspend)
            .unwrap();
        let image = store
            .snapshot_suspend_image_metadata("dev", "paused")
            .unwrap()
            .unwrap();
        fs::write(&image.image_path, b"fake suspend image").unwrap();

        let restore = store.restore_snapshot("dev", "paused").unwrap();
        assert_eq!(restore.snapshot, "paused");
        assert_eq!(restore.restored_state, VmRuntimeState::Suspended);
        assert!(restore.active_disk.is_none());
        let restored_image = restore.suspend_image.as_ref().expect("suspend image");
        assert!(restored_image.image_exists);
        assert_eq!(restored_image.image_path, image.image_path);
        assert_eq!(store.last_restore("dev").unwrap(), Some(restore));
    }

    #[test]
    fn repairs_missing_core_and_snapshot_metadata_without_creating_disks() {
        let store = temp_store();
        let bundle = store.create_vm(&manifest("dev")).unwrap();
        store
            .create_snapshot("dev", "disk-snap", SnapshotKind::Disk)
            .unwrap();
        store
            .create_snapshot("dev", "suspend-snap", SnapshotKind::Suspend)
            .unwrap();
        store
            .create_snapshot("dev", "app-snap", SnapshotKind::ApplicationConsistent)
            .unwrap();

        let state_path = bundle.join("metadata").join("state.json");
        let snapshots_path = bundle.join("metadata").join("snapshots.json");
        let active_disk_path = bundle.join("metadata").join("active-disk.json");
        let token_path = guest_tools_token_path(&bundle);
        let primary_disk_path = bundle.join("metadata").join("primary-disk.json");
        let disk_snapshot_path = snapshot_disk_metadata_path(&bundle, "disk-snap");
        let suspend_path = snapshot_suspend_image_metadata_path(&bundle, "suspend-snap");
        let app_path = application_consistent_snapshot_preflight_path(&bundle, "app-snap");
        let disk_path = bundle.join("disks").join("root.qcow2");

        fs::remove_file(&state_path).unwrap();
        fs::remove_file(&active_disk_path).unwrap();
        fs::remove_file(&token_path).unwrap();
        fs::remove_file(&disk_snapshot_path).unwrap();
        fs::remove_file(&suspend_path).unwrap();
        fs::remove_file(&app_path).unwrap();
        assert!(snapshots_path.exists());
        assert!(!disk_path.exists());

        let repair = store.repair_metadata("dev").unwrap();

        assert!(repair.repaired);
        assert_eq!(repair.vm, "dev");
        assert_eq!(repair.bundle, bundle);
        assert!(repair
            .actions
            .iter()
            .any(|action| action.path == state_path));
        assert!(repair
            .actions
            .iter()
            .any(|action| action.path == active_disk_path));
        assert!(repair
            .actions
            .iter()
            .any(|action| action.path == token_path));
        assert!(repair
            .actions
            .iter()
            .any(|action| action.path == primary_disk_path));
        assert!(repair
            .actions
            .iter()
            .any(|action| action.path == disk_snapshot_path));
        assert!(repair
            .actions
            .iter()
            .any(|action| action.path == suspend_path));
        assert!(repair.actions.iter().any(|action| action.path == app_path));

        assert_eq!(store.state("dev").unwrap().state, VmRuntimeState::Stopped);
        assert!(store
            .active_disk("dev")
            .unwrap()
            .path
            .ends_with("disks/root.qcow2"));
        assert_eq!(store.guest_tools_token("dev").unwrap().token.len(), 64);
        assert!(store
            .snapshot_disk_metadata("dev", "disk-snap")
            .unwrap()
            .is_some());
        assert!(store
            .snapshot_suspend_image_metadata("dev", "suspend-snap")
            .unwrap()
            .is_some());
        assert!(store
            .application_consistent_snapshot_preflight_metadata("dev", "app-snap")
            .unwrap()
            .is_some());
        assert!(primary_disk_path.exists());
        assert!(!disk_path.exists());
    }

    #[test]
    fn repair_metadata_is_noop_when_metadata_is_healthy() {
        let store = temp_store();
        store.create_vm(&manifest("dev")).unwrap();
        store.prepare_primary_disk("dev").unwrap();
        store
            .create_snapshot("dev", "disk-snap", SnapshotKind::Disk)
            .unwrap();

        let repair = store.repair_metadata("dev").unwrap();

        assert!(!repair.repaired);
        assert!(repair.actions.is_empty());
    }

    #[test]
    fn repair_metadata_reports_corrupt_json_without_replacing_it() {
        let store = temp_store();
        let bundle = store.create_vm(&manifest("dev")).unwrap();
        let token_path = guest_tools_token_path(&bundle);
        fs::write(&token_path, b"not json").unwrap();

        let error = store.repair_metadata("dev").unwrap_err();

        assert!(matches!(error, StorageError::Json(_)));
        assert_eq!(fs::read(&token_path).unwrap(), b"not json");
    }

    #[test]
    fn concurrent_snapshot_creates_keep_valid_snapshot_metadata() {
        let store = temp_store();
        store.create_vm(&manifest("dev")).unwrap();

        let first = store.clone();
        let second = store.clone();
        let first = std::thread::spawn(move || {
            first
                .create_snapshot("dev", "first", SnapshotKind::Disk)
                .unwrap();
        });
        let second = std::thread::spawn(move || {
            second
                .create_snapshot("dev", "second", SnapshotKind::Suspend)
                .unwrap();
        });

        first.join().unwrap();
        second.join().unwrap();

        let snapshots = store.snapshots("dev").unwrap();
        assert_eq!(snapshots.len(), 2);
        assert!(snapshots.iter().any(|snapshot| snapshot.name == "first"));
        assert!(snapshots.iter().any(|snapshot| snapshot.name == "second"));
    }

    #[test]
    fn refuses_snapshot_disk_create_when_backing_is_missing() {
        let store = temp_store();
        store.create_vm(&manifest("dev")).unwrap();
        store
            .create_snapshot("dev", "before-upgrade", SnapshotKind::Disk)
            .unwrap();

        let error = store
            .create_snapshot_disk_with("dev", "before-upgrade", |_program, _args| {
                panic!("missing backing should fail before qemu-img")
            })
            .unwrap_err();

        assert!(matches!(error, StorageError::SnapshotDiskBackingMissing(_)));
    }

    #[test]
    fn creates_snapshot_overlay_with_injected_qemu_img_runner() {
        let store = temp_store();
        store.create_vm(&manifest("dev")).unwrap();
        let primary = store.prepare_primary_disk("dev").unwrap();
        fs::write(&primary.path, b"fake backing").unwrap();
        store
            .create_snapshot("dev", "before-upgrade", SnapshotKind::Disk)
            .unwrap();

        let create = store
            .create_snapshot_disk_with("dev", "before-upgrade", |program, args| {
                assert_eq!(program, "qemu-img");
                assert_eq!(args[..6], ["create", "-f", "qcow2", "-F", "qcow2", "-b"]);
                fs::write(&args[7], b"fake overlay")?;
                Ok(Output {
                    status: std::process::ExitStatus::from_raw(0),
                    stdout: b"overlay created\n".to_vec(),
                    stderr: Vec::new(),
                })
            })
            .unwrap();

        assert!(create.executed);
        assert!(create.disk.overlay_exists);
        assert!(create.disk.backing_exists);
        assert_eq!(create.stdout, "overlay created\n");
        assert!(store
            .bundle_path("dev")
            .join("metadata")
            .join("snapshot-disks")
            .join("before-upgrade-create.json")
            .exists());
        let active = store.active_disk("dev").unwrap();
        assert_eq!(active.source, ActiveDiskSource::SnapshotOverlay);
        assert_eq!(active.snapshot.as_deref(), Some("before-upgrade"));
        assert_eq!(active.path, create.disk.overlay_path);
        let chain = store.snapshot_chain("dev").unwrap();
        assert_eq!(chain.active_disk, active);
        assert_eq!(chain.disks.len(), 1);
        assert_eq!(chain.disks[0].snapshot, "before-upgrade");
    }

    #[test]
    fn skips_snapshot_disk_create_when_overlay_exists() {
        let store = temp_store();
        store.create_vm(&manifest("dev")).unwrap();
        let primary = store.prepare_primary_disk("dev").unwrap();
        fs::write(&primary.path, b"fake backing").unwrap();
        store
            .create_snapshot("dev", "before-upgrade", SnapshotKind::Disk)
            .unwrap();
        let disk = store
            .snapshot_disk_metadata("dev", "before-upgrade")
            .unwrap()
            .unwrap();
        fs::write(&disk.overlay_path, b"fake overlay").unwrap();

        let create = store
            .create_snapshot_disk_with("dev", "before-upgrade", |_program, _args| {
                panic!("existing overlay should skip qemu-img")
            })
            .unwrap();

        assert!(!create.executed);
        assert!(create.disk.overlay_exists);
        let active = store.active_disk("dev").unwrap();
        assert_eq!(active.source, ActiveDiskSource::SnapshotOverlay);
        assert_eq!(active.path, disk.overlay_path);
    }

    #[test]
    fn snapshot_chain_uses_active_disk_and_restore_rewinds_to_backing() {
        let store = temp_store();
        store.create_vm(&manifest("dev")).unwrap();
        let primary = store.prepare_primary_disk("dev").unwrap();
        fs::write(&primary.path, b"fake backing").unwrap();
        store
            .create_snapshot("dev", "base", SnapshotKind::Disk)
            .unwrap();
        let base = store
            .create_snapshot_disk_with("dev", "base", |_program, args| {
                fs::write(&args[7], b"fake base overlay")?;
                Ok(Output {
                    status: std::process::ExitStatus::from_raw(0),
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                })
            })
            .unwrap();

        store
            .create_snapshot("dev", "after-base", SnapshotKind::Disk)
            .unwrap();
        let after_base = store
            .snapshot_disk_metadata("dev", "after-base")
            .unwrap()
            .unwrap();
        assert_eq!(after_base.backing_path, base.disk.overlay_path);
        assert_eq!(after_base.backing_format, "qcow2");

        let restore = store.restore_snapshot("dev", "after-base").unwrap();
        let active = restore.active_disk.expect("disk restore active disk");
        assert_eq!(active.source, ActiveDiskSource::SnapshotBacking);
        assert_eq!(active.snapshot.as_deref(), Some("after-base"));
        assert_eq!(active.path, base.disk.overlay_path);
        assert_eq!(store.active_disk("dev").unwrap(), active);
    }

    #[test]
    fn full_clone_rebases_copied_active_and_snapshot_disk_metadata() {
        let store = temp_store();
        store.create_vm(&manifest("dev")).unwrap();
        let primary = store.prepare_primary_disk("dev").unwrap();
        fs::write(&primary.path, b"fake backing").unwrap();
        store
            .create_snapshot("dev", "base", SnapshotKind::Disk)
            .unwrap();
        let source_overlay = store
            .create_snapshot_disk_with("dev", "base", |_program, args| {
                fs::write(&args[7], b"fake overlay")?;
                Ok(Output {
                    status: std::process::ExitStatus::from_raw(0),
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                })
            })
            .unwrap()
            .disk
            .overlay_path;

        let clone = store.clone_vm("dev", "dev-copy", false).unwrap();
        assert!(!clone.linked);
        let clone_bundle = store.bundle_path("dev-copy");
        let active = store.active_disk("dev-copy").unwrap();
        assert_eq!(active.source, ActiveDiskSource::SnapshotOverlay);
        assert!(active.path.starts_with(&clone_bundle));
        assert!(active.path.ends_with("disks/snapshots/base.qcow2"));
        assert!(active.exists);
        assert_ne!(active.path, source_overlay);

        let disk = store
            .snapshot_disk_metadata("dev-copy", "base")
            .unwrap()
            .expect("copied snapshot disk metadata");
        assert!(disk.overlay_path.starts_with(&clone_bundle));
        assert!(disk.backing_path.starts_with(&clone_bundle));
        assert!(disk.overlay_exists);
        assert!(disk.backing_exists);
        assert_eq!(
            disk.create_command[7],
            disk.backing_path.display().to_string()
        );
        assert_eq!(
            disk.create_command[8],
            disk.overlay_path.display().to_string()
        );
    }

    #[test]
    fn linked_clone_creates_overlay_backed_by_source_active_disk() {
        let store = temp_store();
        store.create_vm(&manifest("dev")).unwrap();
        let primary = store.prepare_primary_disk("dev").unwrap();
        fs::write(&primary.path, b"fake backing").unwrap();
        store
            .create_snapshot("dev", "source-only", SnapshotKind::Disk)
            .unwrap();

        let clone = store
            .clone_vm_with("dev", "dev-linked", true, |program, args| {
                assert_eq!(program, "qemu-img");
                assert_eq!(args[..6], ["create", "-f", "qcow2", "-F", "qcow2", "-b"]);
                assert_eq!(args[6], primary.path.display().to_string());
                fs::write(&args[7], b"fake linked overlay")?;
                Ok(Output {
                    status: std::process::ExitStatus::from_raw(0),
                    stdout: b"linked overlay created\n".to_vec(),
                    stderr: Vec::new(),
                })
            })
            .unwrap();

        let clone_bundle = store.bundle_path("dev-linked");
        assert!(clone.linked);
        assert_eq!(clone.backing_path.as_ref(), Some(&primary.path));
        assert_eq!(clone.backing_format.as_deref(), Some("qcow2"));
        assert_eq!(
            clone.create_command.as_ref().unwrap()[..7],
            ["qemu-img", "create", "-f", "qcow2", "-F", "qcow2", "-b"]
        );
        let (_, manifest) = store.get_vm("dev-linked").unwrap();
        assert_eq!(manifest.name, "dev-linked");
        assert_eq!(manifest.network.hostname, "dev-linked.bridgevm.local");
        assert_eq!(manifest.storage.primary.path, "disks/root.qcow2");
        assert_eq!(manifest.storage.primary.format, "qcow2");

        let active = store.active_disk("dev-linked").unwrap();
        assert_eq!(active.source, ActiveDiskSource::Primary);
        assert_eq!(active.path, clone_bundle.join("disks").join("root.qcow2"));
        assert_eq!(fs::read(&active.path).unwrap(), b"fake linked overlay");
        assert!(store.snapshots("dev-linked").unwrap().is_empty());
        assert!(store.snapshot_chain("dev-linked").unwrap().disks.is_empty());
        assert!(!clone_bundle
            .join("metadata")
            .join("snapshot-disks")
            .exists());
        assert!(!clone_bundle.join("suspend-images").exists());
    }

    #[test]
    fn linked_clone_requires_existing_source_active_disk() {
        let store = temp_store();
        store.create_vm(&manifest("dev")).unwrap();

        let error = store
            .clone_vm_with("dev", "dev-linked", true, |_program, _args| {
                panic!("missing backing should fail before qemu-img")
            })
            .unwrap_err();

        assert!(matches!(error, StorageError::DiskMissing(_)));
        assert!(!store.bundle_path("dev-linked").exists());
    }

    #[test]
    fn linked_clone_reports_qemu_img_failure() {
        let store = temp_store();
        store.create_vm(&manifest("dev")).unwrap();
        let primary = store.prepare_primary_disk("dev").unwrap();
        fs::write(&primary.path, b"fake backing").unwrap();

        let error = store
            .clone_vm_with("dev", "dev-linked", true, |_program, _args| {
                Ok(Output {
                    status: std::process::ExitStatus::from_raw(1 << 8),
                    stdout: Vec::new(),
                    stderr: b"qemu-img failed".to_vec(),
                })
            })
            .unwrap_err();

        let StorageError::LinkedCloneDiskCreateFailed {
            command,
            status,
            stderr,
        } = error
        else {
            panic!("expected linked clone qemu-img failure");
        };
        assert_eq!(
            command[..7],
            ["qemu-img", "create", "-f", "qcow2", "-F", "qcow2", "-b"]
        );
        assert!(status.contains('1'));
        assert_eq!(stderr, "qemu-img failed");
        assert!(!store.bundle_path("dev-linked").exists());
    }

    #[test]
    fn full_clone_copies_disk_and_manifest_with_new_name_and_hostname() {
        let store = temp_store();
        store.create_vm(&manifest("dev")).unwrap();
        let primary = store.prepare_primary_disk("dev").unwrap();
        fs::write(&primary.path, b"fake disk contents").unwrap();

        let clone = store.clone_vm("dev", "dev-copy", false).unwrap();
        assert!(!clone.linked);

        let (clone_bundle, clone_manifest) = store.get_vm("dev-copy").unwrap();
        assert_eq!(clone_manifest.name, "dev-copy");
        assert_eq!(clone_manifest.network.hostname, "dev-copy.bridgevm.local");

        // Primary disk file was copied into the clone bundle (independent file).
        let clone_disk = resolve_bundle_path(&clone_bundle, &clone_manifest.storage.primary.path);
        assert!(clone_disk.starts_with(&clone_bundle));
        assert_eq!(fs::read(&clone_disk).unwrap(), b"fake disk contents");
        assert_ne!(clone_disk, primary.path);

        // Source is untouched.
        let (_, source_manifest) = store.get_vm("dev").unwrap();
        assert_eq!(source_manifest.name, "dev");
        assert_eq!(source_manifest.network.hostname, "dev.bridgevm.local");
    }

    #[test]
    fn full_clone_resets_per_vm_identity_and_runtime_state() {
        let store = temp_store();
        let source_bundle = store.create_vm(&manifest("dev")).unwrap();
        let primary = store.prepare_primary_disk("dev").unwrap();
        fs::write(&primary.path, b"fake disk").unwrap();

        // Persisted per-VM identity written by the Apple VZ runner.
        let source_metadata = source_bundle.join("metadata");
        fs::write(source_metadata.join("machine-identifier.bin"), b"source-id").unwrap();
        fs::write(
            source_metadata.join("network-mac-address.txt"),
            b"52:54:00:aa:bb:cc",
        )
        .unwrap();
        // Transient runtime state that must not be inherited.
        fs::write(source_metadata.join("runner.json"), b"{}").unwrap();
        let fast_suspend = source_metadata.join("suspend-images");
        fs::create_dir_all(&fast_suspend).unwrap();
        fs::write(fast_suspend.join("dev.bin"), b"saved-state").unwrap();
        store
            .write_state_at(&source_bundle, VmRuntimeState::Suspended)
            .unwrap();
        let source_token = store.guest_tools_token("dev").unwrap();

        let clone_bundle = store.clone_vm("dev", "dev-copy", false).unwrap().output;
        let clone_metadata = clone_bundle.join("metadata");

        // Identity dropped: regenerated fresh on next launch, not the source's.
        assert!(!clone_metadata.join("machine-identifier.bin").exists());
        assert!(!clone_metadata.join("network-mac-address.txt").exists());
        // Transient runtime state excluded.
        assert!(!clone_metadata.join("runner.json").exists());
        assert!(!clone_metadata.join("suspend-images").exists());

        // Guest-tools token regenerated (distinct credential).
        let clone_token = store.guest_tools_token("dev-copy").unwrap();
        assert_ne!(clone_token.token, source_token.token);

        // Clone starts stopped/clean even though the source was suspended.
        let clone_state = store.state("dev-copy").unwrap();
        assert_eq!(clone_state.state, VmRuntimeState::Stopped);

        // Source identity is preserved.
        assert!(source_metadata.join("machine-identifier.bin").exists());
        assert!(source_metadata.join("network-mac-address.txt").exists());
    }

    #[test]
    fn full_clone_is_independent_of_source() {
        let store = temp_store();
        store.create_vm(&manifest("dev")).unwrap();
        let primary = store.prepare_primary_disk("dev").unwrap();
        fs::write(&primary.path, b"fake disk").unwrap();

        let clone_bundle = store.clone_vm("dev", "dev-copy", false).unwrap().output;

        // Mutate the clone's manifest and confirm the source is unaffected.
        let (_, mut clone_manifest) = store.get_vm("dev-copy").unwrap();
        clone_manifest.guest.os = "windows".to_string();
        clone_manifest
            .write(&clone_bundle.join("manifest.yaml"))
            .unwrap();

        let (_, source_manifest) = store.get_vm("dev").unwrap();
        assert_eq!(source_manifest.guest.os, "ubuntu");
        let (_, reread_clone) = store.get_vm("dev-copy").unwrap();
        assert_eq!(reread_clone.guest.os, "windows");
    }

    #[test]
    fn full_clone_rejects_existing_destination() {
        let store = temp_store();
        store.create_vm(&manifest("dev")).unwrap();
        store.create_vm(&manifest("taken")).unwrap();

        let error = store.clone_vm("dev", "taken", false).unwrap_err();
        assert!(matches!(error, StorageError::AlreadyExists(name) if name == "taken"));
        // Existing destination bundle is left intact.
        assert!(store.get_vm("taken").is_ok());
    }

    #[test]
    fn full_clone_rejects_missing_source() {
        let store = temp_store();
        store.ensure().unwrap();

        let error = store.clone_vm("ghost", "dev-copy", false).unwrap_err();
        assert!(matches!(error, StorageError::NotFound(name) if name == "ghost"));
        assert!(!store.bundle_path("dev-copy").exists());
    }

    #[test]
    fn exports_vm_bundle_copy_with_metadata() {
        let store = temp_store();
        let bundle = store.create_vm(&manifest("dev")).unwrap();
        store
            .create_snapshot("dev", "before-upgrade", SnapshotKind::Disk)
            .unwrap();
        fs::write(
            bundle.join("metadata").join("qmp.sock"),
            b"socket placeholder",
        )
        .unwrap();
        fs::write(bundle.join("metadata").join("export.lock"), b"locked").unwrap();
        let output = store.root().join("exports").join("dev.vmbridge");

        let export = store.export_vm("dev", &output).unwrap();
        assert_eq!(export.vm, "dev");
        assert_eq!(export.archive_format, "directory");
        assert!(export.copied_file_count >= 2);
        assert!(export.copied_files.contains(&"manifest.yaml".to_string()));
        assert!(export
            .copied_files
            .contains(&"metadata/snapshots.json".to_string()));
        assert!(export.manifest_preserved);
        assert!(export.metadata_preserved);
        assert!(output.join("manifest.yaml").exists());
        assert!(output.join("metadata").join("snapshots.json").exists());
        assert!(output.join("metadata").join("export.json").exists());
        assert!(!export
            .copied_files
            .contains(&"metadata/qmp.sock".to_string()));
        assert!(!export
            .copied_files
            .contains(&"metadata/export.lock".to_string()));
        assert!(!output.join("metadata").join("qmp.sock").exists());
        assert!(!output.join("metadata").join("export.lock").exists());

        let duplicate = store.export_vm("dev", &output).unwrap_err();
        assert!(matches!(duplicate, StorageError::ExportAlreadyExists(_)));
    }

    #[test]
    fn rejects_export_output_at_or_inside_source_bundle() {
        let store = temp_store();
        let bundle = store.create_vm(&manifest("dev")).unwrap();

        let self_export = store.export_vm("dev", &bundle).unwrap_err();
        assert!(matches!(
            self_export,
            StorageError::ExportOutputInsideSource { .. }
        ));

        let nested_export = store
            .export_vm("dev", bundle.join("exports").join("dev.vmbridge"))
            .unwrap_err();
        assert!(matches!(
            nested_export,
            StorageError::ExportOutputInsideSource { .. }
        ));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinks_while_exporting_vm_bundle() {
        use std::os::unix::fs::symlink;

        let store = temp_store();
        let bundle = store.create_vm(&manifest("dev")).unwrap();
        symlink("manifest.yaml", bundle.join("manifest-link.yaml")).unwrap();
        let output = store.root().join("exports").join("dev.vmbridge");

        let error = store.export_vm("dev", &output).unwrap_err();
        assert!(matches!(error, StorageError::UnsupportedBundleEntry(_)));
    }

    #[test]
    fn imports_exported_vm_bundle_with_optional_rename() {
        let source = temp_store();
        source.create_vm(&manifest("dev")).unwrap();
        source
            .create_snapshot("dev", "before-upgrade", SnapshotKind::Disk)
            .unwrap();
        let output = source.root().join("exports").join("dev.vmbridge");
        source.export_vm("dev", &output).unwrap();

        let target = temp_store();
        let import = target.import_vm(&output, Some("dev-copy")).unwrap();
        assert_eq!(import.vm, "dev-copy");
        assert_eq!(import.original_name, "dev");
        assert_eq!(import.requested_name.as_deref(), Some("dev-copy"));
        assert_eq!(import.archive_format, "directory");
        assert!(import.manifest_identity_rewritten);
        assert!(import.manifest_preserved);
        assert!(import.metadata_preserved);
        assert!(import.copied_files.contains(&"manifest.yaml".to_string()));
        assert!(import.output.join("manifest.yaml").exists());
        assert!(import.output.join("metadata").join("import.json").exists());

        let (_, manifest) = target.get_vm("dev-copy").unwrap();
        assert_eq!(manifest.name, "dev-copy");
        assert_eq!(manifest.network.hostname, "dev-copy.bridgevm.local");
        assert_eq!(target.snapshots("dev-copy").unwrap().len(), 1);
    }

    #[test]
    fn exports_and_imports_tar_vm_bundle_with_optional_rename() {
        let source = temp_store();
        let bundle = source.create_vm(&manifest("dev")).unwrap();
        source
            .create_snapshot("dev", "before-upgrade", SnapshotKind::Disk)
            .unwrap();
        fs::write(
            bundle.join("metadata").join("qmp.sock"),
            b"socket placeholder",
        )
        .unwrap();
        fs::write(bundle.join("metadata").join("export.lock"), b"locked").unwrap();
        let output = source.root().join("exports").join("dev.tar");
        let export = source.export_vm("dev", &output).unwrap();
        assert_eq!(export.output, output);
        assert_eq!(export.archive_format, "tar");
        assert!(output.is_file());
        assert!(!export
            .copied_files
            .contains(&"metadata/qmp.sock".to_string()));
        assert!(!export
            .copied_files
            .contains(&"metadata/export.lock".to_string()));

        let target = temp_store();
        let import = target.import_vm(&output, Some("dev-copy")).unwrap();
        assert_eq!(import.vm, "dev-copy");
        assert_eq!(import.source, output);
        assert_eq!(import.original_name, "dev");
        assert_eq!(import.requested_name.as_deref(), Some("dev-copy"));
        assert_eq!(import.archive_format, "tar");
        assert!(import.manifest_identity_rewritten);
        assert!(import.copied_files.contains(&"manifest.yaml".to_string()));
        assert!(import
            .copied_files
            .contains(&"metadata/export.json".to_string()));
        assert!(import.output.join("manifest.yaml").exists());
        assert!(import.output.join("metadata").join("export.json").exists());
        assert!(import.output.join("metadata").join("import.json").exists());
        assert!(!import.output.join("metadata").join("qmp.sock").exists());
        assert!(!import.output.join("metadata").join("export.lock").exists());

        let (_, manifest) = target.get_vm("dev-copy").unwrap();
        assert_eq!(manifest.name, "dev-copy");
        assert_eq!(manifest.network.hostname, "dev-copy.bridgevm.local");
        assert_eq!(target.snapshots("dev-copy").unwrap().len(), 1);
    }

    #[test]
    fn rejects_duplicate_tar_imports() {
        let source = temp_store();
        source.create_vm(&manifest("dev")).unwrap();
        let output = source.root().join("exports").join("dev.tar");
        source.export_vm("dev", &output).unwrap();

        let target = temp_store();
        target.import_vm(&output, None).unwrap();
        let duplicate = target.import_vm(&output, None).unwrap_err();
        assert!(matches!(duplicate, StorageError::AlreadyExists(_)));
    }

    #[test]
    fn rejects_tar_import_with_parent_directory_entry() {
        let tar_path = unique_temp_path("bridgevm-parent-test").with_extension("tar");
        write_raw_tar_entry(&tar_path, "../manifest.yaml", b'0', None, b"name: evil\n");

        let store = temp_store();
        let error = store.import_vm(&tar_path, None).unwrap_err();
        let _ = fs::remove_file(&tar_path);
        assert!(matches!(error, StorageError::UnsafeArchiveEntry(_)));
    }

    #[test]
    fn rejects_tar_import_with_symlink_entry() {
        let tar_path = unique_temp_path("bridgevm-symlink-test").with_extension("tar");
        {
            let file = fs::File::create(&tar_path).unwrap();
            let mut builder = tar::Builder::new(file);
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(tar::EntryType::Symlink);
            header.set_size(0);
            header.set_mode(0o777);
            header.set_link_name("manifest.yaml").unwrap();
            header.set_cksum();
            builder
                .append_data(&mut header, "manifest-link.yaml", &[][..])
                .unwrap();
            builder.finish().unwrap();
        }

        let store = temp_store();
        let error = store.import_vm(&tar_path, None).unwrap_err();
        let _ = fs::remove_file(&tar_path);
        assert!(matches!(error, StorageError::UnsupportedBundleEntry(_)));
    }

    #[test]
    fn rejects_duplicate_and_invalid_imports() {
        let source = temp_store();
        source.create_vm(&manifest("dev")).unwrap();
        let output = source.root().join("exports").join("dev.vmbridge");
        source.export_vm("dev", &output).unwrap();

        let target = temp_store();
        target.import_vm(&output, None).unwrap();
        let duplicate = target.import_vm(&output, None).unwrap_err();
        assert!(matches!(duplicate, StorageError::AlreadyExists(_)));

        let invalid = target.import_vm(target.root().join("missing.vmbridge"), None);
        assert!(matches!(invalid, Err(StorageError::InvalidImportBundle(_))));
    }

    #[test]
    fn rejects_imports_from_destination_store_or_same_bundle() {
        let store = temp_store();
        let bundle = store.create_vm(&manifest("dev")).unwrap();

        let self_import = store.import_vm(&bundle, None);
        assert!(matches!(
            self_import,
            Err(StorageError::ImportPathConflict { .. })
        ));

        let output = store.root().join("exports").join("dev.vmbridge");
        store.export_vm("dev", &output).unwrap();
        let internal_import = store.import_vm(&output, Some("dev-copy"));
        assert!(matches!(
            internal_import,
            Err(StorageError::ImportPathConflict { .. })
        ));
    }

    #[test]
    fn writes_runner_metadata() {
        let store = temp_store();
        store.create_vm(&manifest("dev")).unwrap();
        let metadata = RunnerMetadata {
            engine: "fullvm".to_string(),
            pid: Some(42),
            command: vec!["qemu-system-x86_64".to_string()],
            log_path: PathBuf::from("logs/qemu.log"),
            started_at_unix: now_unix(),
            dry_run: false,
            launch_spec_path: None,
            guest_tools: None,
            disk: None,
            active_disk: None,
            launch_readiness: Some(LaunchReadinessMetadata {
                ready: false,
                blockers: vec![LaunchReadinessBlockerMetadata {
                    code: "missing-primary-disk".to_string(),
                    message: "Primary disk is missing.".to_string(),
                    path: Some(PathBuf::from("disks/root.qcow2")),
                    capability: None,
                }],
            }),
            runtime_control: None,
        };

        store.write_runner_metadata("dev", &metadata).unwrap();
        assert_eq!(store.runner_metadata("dev").unwrap(), Some(metadata));

        store.clear_runner_metadata("dev").unwrap();
        assert_eq!(store.runner_metadata("dev").unwrap(), None);
    }

    #[test]
    fn writes_runtime_resource_policy_metadata() {
        let store = temp_store();
        store.create_vm(&manifest("dev")).unwrap();
        let metadata = RuntimeResourcePolicyMetadata {
            vm: "dev".to_string(),
            mode: "fast".to_string(),
            profile: "automatic".to_string(),
            visibility: RuntimeResourceVisibility::Background,
            state: VmRuntimeState::Running,
            on_battery: true,
            memory: "2048".to_string(),
            cpu: "1".to_string(),
            display_fps_cap: "10".to_string(),
            rationale: "Battery or background throttling active.".to_string(),
            live_applied: false,
            runtime_control_acknowledged: false,
            live_apply_blockers: vec![RuntimeResourcePolicyBlocker {
                code: "runtime-control-unavailable".to_string(),
                message: "No live runtime control channel is available.".to_string(),
            }],
            updated_at_unix: now_unix(),
        };

        store
            .write_runtime_resource_policy_metadata("dev", &metadata)
            .unwrap();
        assert_eq!(
            store.runtime_resource_policy_metadata("dev").unwrap(),
            Some(metadata)
        );
    }

    #[test]
    fn prepares_primary_disk_metadata_for_qcow2_and_raw() {
        let store = temp_store();
        store.create_vm(&manifest("dev")).unwrap();

        let qcow2 = store.prepare_primary_disk("dev").unwrap();
        assert_eq!(qcow2.format, "qcow2");
        assert!(!qcow2.exists);
        assert!(!qcow2.created);
        assert_eq!(qcow2.size_bytes, Some(80 * 1024 * 1024 * 1024));
        assert_eq!(
            qcow2.create_command.as_ref().unwrap()[..3],
            ["qemu-img", "create", "-f"]
        );
        assert!(store
            .bundle_path("dev")
            .join("metadata")
            .join("primary-disk.json")
            .exists());

        let mut raw_manifest = manifest("raw-dev");
        raw_manifest.storage.primary.format = "raw".to_string();
        raw_manifest.storage.primary.path = "disks/root.raw".to_string();
        raw_manifest.storage.primary.size = "1MiB".to_string();
        store.create_vm(&raw_manifest).unwrap();

        let raw = store.prepare_primary_disk("raw-dev").unwrap();
        assert!(raw.exists);
        assert!(raw.created);
        assert_eq!(raw.size_bytes, Some(1024 * 1024));
        assert_eq!(fs::metadata(raw.path).unwrap().len(), 1024 * 1024);
    }

    #[test]
    fn creates_primary_qcow2_disk_with_injected_qemu_img_runner() {
        let store = temp_store();
        store.create_vm(&manifest("dev")).unwrap();

        let create = store
            .create_primary_disk_with("dev", |program, args| {
                assert_eq!(program, "qemu-img");
                assert_eq!(args[..3], ["create", "-f", "qcow2"]);
                fs::write(&args[3], b"fake qcow2")?;
                Ok(Output {
                    status: std::process::ExitStatus::from_raw(0),
                    stdout: b"created\n".to_vec(),
                    stderr: Vec::new(),
                })
            })
            .unwrap();

        assert!(create.executed);
        assert_eq!(
            create.command.as_ref().unwrap()[..3],
            ["qemu-img", "create", "-f"]
        );
        assert!(create.preparation.exists);
        assert!(!create.preparation.created);
        assert_eq!(create.stdout, "created\n");
        assert!(store
            .bundle_path("dev")
            .join("metadata")
            .join("last-disk-create.json")
            .exists());
    }

    #[test]
    fn reports_failed_primary_disk_create_command() {
        let store = temp_store();
        store.create_vm(&manifest("dev")).unwrap();

        let error = store
            .create_primary_disk_with("dev", |_program, _args| {
                Ok(Output {
                    status: std::process::ExitStatus::from_raw(1 << 8),
                    stdout: Vec::new(),
                    stderr: b"qemu-img failed".to_vec(),
                })
            })
            .unwrap_err();

        let StorageError::DiskCreateFailed {
            command,
            status,
            stderr,
        } = error
        else {
            panic!("expected disk create failure");
        };
        assert_eq!(command[..3], ["qemu-img", "create", "-f"]);
        assert!(status.contains('1'));
        assert_eq!(stderr, "qemu-img failed");
    }

    #[test]
    fn skips_primary_disk_create_when_prepare_already_made_raw_disk() {
        let store = temp_store();
        let mut raw_manifest = manifest("raw-dev");
        raw_manifest.storage.primary.format = "raw".to_string();
        raw_manifest.storage.primary.path = "disks/root.raw".to_string();
        raw_manifest.storage.primary.size = "1MiB".to_string();
        store.create_vm(&raw_manifest).unwrap();

        let create = store
            .create_primary_disk_with("raw-dev", |_program, _args| {
                panic!("raw disk creation should be handled by prepare")
            })
            .unwrap();

        assert!(!create.executed);
        assert!(create.command.is_none());
        assert!(create.preparation.exists);
        assert!(create.preparation.created);
    }

    #[test]
    fn inspects_existing_primary_disk_with_injected_qemu_img_runner() {
        let store = temp_store();
        store.create_vm(&manifest("dev")).unwrap();
        store
            .create_primary_disk_with("dev", |_program, args| {
                fs::write(&args[3], b"fake qcow2")?;
                Ok(Output {
                    status: std::process::ExitStatus::from_raw(0),
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                })
            })
            .unwrap();

        let inspect = store
            .inspect_primary_disk_with("dev", |program, args| {
                assert_eq!(program, "qemu-img");
                assert_eq!(args[0], "info");
                assert_eq!(args[1], "--output=json");
                Ok(Output {
                    status: std::process::ExitStatus::from_raw(0),
                    stdout: br#"{"format":"qcow2","virtual-size":85899345920}"#.to_vec(),
                    stderr: Vec::new(),
                })
            })
            .unwrap();

        assert_eq!(inspect.command[..2], ["qemu-img", "info"]);
        assert_eq!(inspect.info["format"], "qcow2");
        assert_eq!(inspect.info["virtual-size"], 80 * 1024 * 1024 * 1024_u64);
        let metadata_path = store
            .bundle_path("dev")
            .join("metadata")
            .join("last-disk-inspect.json");
        assert!(metadata_path.exists());
        let recorded: DiskInspectMetadata =
            serde_json::from_str(&fs::read_to_string(metadata_path).unwrap()).unwrap();
        assert_eq!(
            recorded.inspect_duration_microseconds,
            inspect.inspect_duration_microseconds
        );
    }

    #[test]
    fn rejects_inspection_when_primary_disk_is_missing() {
        let store = temp_store();
        store.create_vm(&manifest("dev")).unwrap();

        let error = store
            .inspect_primary_disk_with("dev", |_program, _args| {
                panic!("missing disk should fail before qemu-img info")
            })
            .unwrap_err();

        assert!(matches!(error, StorageError::DiskMissing(_)));
    }

    #[test]
    fn reports_failed_primary_disk_inspection_command() {
        let store = temp_store();
        store.create_vm(&manifest("dev")).unwrap();
        store
            .create_primary_disk_with("dev", |_program, args| {
                fs::write(&args[3], b"fake qcow2")?;
                Ok(Output {
                    status: std::process::ExitStatus::from_raw(0),
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                })
            })
            .unwrap();

        let error = store
            .inspect_primary_disk_with("dev", |_program, _args| {
                Ok(Output {
                    status: std::process::ExitStatus::from_raw(1 << 8),
                    stdout: Vec::new(),
                    stderr: b"bad image".to_vec(),
                })
            })
            .unwrap_err();

        let StorageError::DiskInspectFailed {
            command,
            status,
            stderr,
        } = error
        else {
            panic!("expected disk inspect failure");
        };
        assert_eq!(command[..2], ["qemu-img", "info"]);
        assert!(status.contains('1'));
        assert_eq!(stderr, "bad image");
    }

    #[test]
    fn verifies_active_qcow2_disk_with_injected_qemu_img_runner() {
        let store = temp_store();
        store.create_vm(&manifest("dev")).unwrap();
        store
            .create_primary_disk_with("dev", |_program, args| {
                fs::write(&args[3], b"fake qcow2")?;
                Ok(Output {
                    status: std::process::ExitStatus::from_raw(0),
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                })
            })
            .unwrap();

        let verify = store
            .verify_active_disk_with("dev", |program, args| {
                assert_eq!(program, "qemu-img");
                assert_eq!(args[0], "check");
                assert_eq!(args[1], "--output=json");
                Ok(Output {
                    status: std::process::ExitStatus::from_raw(0),
                    stdout: br#"{"image-end-offset":4096,"check-errors":0}"#.to_vec(),
                    stderr: Vec::new(),
                })
            })
            .unwrap();

        assert_eq!(verify.command[..2], ["qemu-img", "check"]);
        assert_eq!(verify.active_disk.source, ActiveDiskSource::Primary);
        assert_eq!(verify.report["check-errors"], 0);
        assert_eq!(verify.report["image-end-offset"], 4096);
        let metadata_path = store
            .bundle_path("dev")
            .join("metadata")
            .join("last-disk-verify.json");
        assert!(metadata_path.exists());
        let recorded: DiskVerifyMetadata =
            serde_json::from_str(&fs::read_to_string(metadata_path).unwrap()).unwrap();
        assert_eq!(
            recorded.verify_duration_microseconds,
            verify.verify_duration_microseconds
        );
    }

    #[test]
    fn rejects_verification_for_raw_disks() {
        let store = temp_store();
        let mut raw_manifest = manifest("raw-dev");
        raw_manifest.storage.primary.format = "raw".to_string();
        raw_manifest.storage.primary.path = "disks/root.raw".to_string();
        raw_manifest.storage.primary.size = "1MiB".to_string();
        store.create_vm(&raw_manifest).unwrap();

        let error = store
            .verify_active_disk_with("raw-dev", |_program, _args| {
                panic!("raw disk verification should fail before qemu-img check")
            })
            .unwrap_err();

        assert!(matches!(error, StorageError::DiskVerifyUnsupportedRaw(_)));
    }

    #[test]
    fn reports_failed_disk_verification_command() {
        let store = temp_store();
        store.create_vm(&manifest("dev")).unwrap();
        store
            .create_primary_disk_with("dev", |_program, args| {
                fs::write(&args[3], b"fake qcow2")?;
                Ok(Output {
                    status: std::process::ExitStatus::from_raw(0),
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                })
            })
            .unwrap();

        let error = store
            .verify_active_disk_with("dev", |_program, _args| {
                Ok(Output {
                    status: std::process::ExitStatus::from_raw(2 << 8),
                    stdout: br#"{"check-errors":1}"#.to_vec(),
                    stderr: b"check failed".to_vec(),
                })
            })
            .unwrap_err();

        let StorageError::DiskVerifyFailed {
            command,
            status,
            stderr,
        } = error
        else {
            panic!("expected disk verify failure");
        };
        assert_eq!(command[..2], ["qemu-img", "check"]);
        assert!(status.contains('2'));
        assert_eq!(stderr, "check failed");
    }

    #[test]
    fn compacts_active_qcow2_disk_with_injected_qemu_img_runner() {
        let store = temp_store();
        store.create_vm(&manifest("dev")).unwrap();
        store
            .create_primary_disk_with("dev", |_program, args| {
                fs::write(&args[3], b"fake qcow2 image with slack space")?;
                Ok(Output {
                    status: std::process::ExitStatus::from_raw(0),
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                })
            })
            .unwrap();

        let compact = store
            .compact_active_disk_with("dev", |program, args| {
                assert_eq!(program, "qemu-img");
                assert_eq!(args[..3], ["convert", "-O", "qcow2"]);
                fs::write(&args[4], b"small qcow2")?;
                Ok(Output {
                    status: std::process::ExitStatus::from_raw(0),
                    stdout: b"compacted\n".to_vec(),
                    stderr: Vec::new(),
                })
            })
            .unwrap();

        assert_eq!(compact.command[..3], ["qemu-img", "convert", "-O"]);
        assert_eq!(fs::read(&compact.active_disk.path).unwrap(), b"small qcow2");
        assert!(compact.backup_path.exists());
        assert!(!compact.temp_path.exists());
        assert!(compact.original_size_bytes > compact.compacted_size_bytes);
        assert_eq!(compact.stdout, "compacted\n");
        assert!(store
            .bundle_path("dev")
            .join("metadata")
            .join("last-disk-compact.json")
            .exists());
    }

    #[test]
    fn rejects_compaction_for_raw_disks() {
        let store = temp_store();
        let mut raw_manifest = manifest("raw-dev");
        raw_manifest.storage.primary.format = "raw".to_string();
        raw_manifest.storage.primary.path = "disks/root.raw".to_string();
        raw_manifest.storage.primary.size = "1MiB".to_string();
        store.create_vm(&raw_manifest).unwrap();
        store.prepare_primary_disk("raw-dev").unwrap();

        let error = store
            .compact_active_disk_with("raw-dev", |_program, _args| {
                panic!("raw disk should fail before qemu-img convert")
            })
            .unwrap_err();

        assert!(matches!(error, StorageError::DiskCompactUnsupportedRaw(_)));
    }

    #[test]
    fn reports_failed_disk_compaction_command() {
        let store = temp_store();
        store.create_vm(&manifest("dev")).unwrap();
        store
            .create_primary_disk_with("dev", |_program, args| {
                fs::write(&args[3], b"fake qcow2")?;
                Ok(Output {
                    status: std::process::ExitStatus::from_raw(0),
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                })
            })
            .unwrap();

        let error = store
            .compact_active_disk_with("dev", |_program, _args| {
                Ok(Output {
                    status: std::process::ExitStatus::from_raw(1 << 8),
                    stdout: Vec::new(),
                    stderr: b"convert failed".to_vec(),
                })
            })
            .unwrap_err();

        let StorageError::DiskCompactFailed {
            command,
            status,
            stderr,
        } = error
        else {
            panic!("expected disk compact failure");
        };
        assert_eq!(command[..3], ["qemu-img", "convert", "-O"]);
        assert!(status.contains('1'));
        assert_eq!(stderr, "convert failed");
    }

    #[test]
    fn rejects_invalid_lifecycle_transition() {
        let store = temp_store();
        store.create_vm(&manifest("dev")).unwrap();

        let error = store
            .transition_state("dev", VmRuntimeState::Suspended)
            .unwrap_err();
        assert!(matches!(
            error,
            StorageError::InvalidStateTransition {
                from: VmRuntimeState::Stopped,
                to: VmRuntimeState::Suspended
            }
        ));
    }

    #[test]
    fn manifest_migration_dry_run_does_not_write_receipt_or_backup() {
        let store = temp_store();
        store.create_vm(&manifest("dev")).unwrap();

        let migration = store.migrate_manifest("dev", true).unwrap();

        assert!(migration.dry_run);
        assert!(!migration.migrated);
        assert_eq!(migration.from_schema, SCHEMA_VERSION);
        assert_eq!(migration.to_schema, SCHEMA_VERSION);
        assert!(migration.backup_path.is_none());
        assert!(migration.receipt_path.is_none());
        let bundle = store.bundle_path("dev");
        assert!(!bundle
            .join("metadata")
            .join("manifest-before-migration.yaml")
            .exists());
        assert!(!bundle
            .join("metadata")
            .join("manifest-migration.json")
            .exists());
    }

    #[test]
    fn manifest_migration_writes_receipt_and_backup_for_current_schema() {
        let store = temp_store();
        store.create_vm(&manifest("dev")).unwrap();

        let migration = store.migrate_manifest("dev", false).unwrap();

        assert!(!migration.dry_run);
        assert!(!migration.migrated);
        assert_eq!(migration.from_schema, SCHEMA_VERSION);
        assert_eq!(migration.to_schema, SCHEMA_VERSION);
        assert!(migration.backup_path.as_ref().unwrap().exists());
        assert!(migration.receipt_path.as_ref().unwrap().exists());
        let receipt =
            read_json_file::<VmManifestMigrationMetadata>(migration.receipt_path.as_ref().unwrap())
                .unwrap()
                .expect("manifest migration receipt");
        assert_eq!(receipt.vm, "dev");
        assert_eq!(receipt.manifest_path, migration.manifest_path);
    }

    #[test]
    fn manifest_migration_rejects_future_schema_without_receipt() {
        let store = temp_store();
        store.create_vm(&manifest("dev")).unwrap();
        let bundle = store.bundle_path("dev");
        let manifest_path = bundle.join("manifest.yaml");
        let mut raw_manifest = fs::read_to_string(&manifest_path).unwrap();
        raw_manifest = raw_manifest.replace(SCHEMA_VERSION, "bridgevm.io/v99");
        fs::write(&manifest_path, raw_manifest).unwrap();

        let error = store.migrate_manifest("dev", false).unwrap_err();

        assert!(matches!(
            error,
            StorageError::Config(ConfigError::UnsupportedSchema { .. })
        ));
        assert!(!bundle
            .join("metadata")
            .join("manifest-before-migration.yaml")
            .exists());
        assert!(!bundle
            .join("metadata")
            .join("manifest-migration.json")
            .exists());
    }

    #[test]
    fn manifest_migration_rejects_malformed_yaml_without_receipt() {
        let store = temp_store();
        store.create_vm(&manifest("dev")).unwrap();
        let bundle = store.bundle_path("dev");
        let manifest_path = bundle.join("manifest.yaml");
        fs::write(
            &manifest_path,
            "schemaVersion: bridgevm.io/v1\nname: [not-valid-yaml\n",
        )
        .unwrap();

        let error = store.migrate_manifest("dev", false).unwrap_err();

        assert!(matches!(error, StorageError::Config(ConfigError::Yaml(_))));
        assert!(!bundle
            .join("metadata")
            .join("manifest-before-migration.yaml")
            .exists());
        assert!(!bundle
            .join("metadata")
            .join("manifest-migration.json")
            .exists());
    }

    #[test]
    fn manifest_migration_ignores_metadata_deleted_vm() {
        let store = temp_store();
        store.create_vm(&manifest("dev")).unwrap();
        let deletion = store.delete_vm_metadata_only("dev").unwrap();

        let error = store.migrate_manifest("dev", false).unwrap_err();

        assert!(matches!(error, StorageError::NotFound(name) if name == "dev"));
        assert!(deletion.metadata_path.exists());
        let bundle = store.bundle_path("dev");
        assert!(!bundle
            .join("metadata")
            .join("manifest-before-migration.yaml")
            .exists());
        assert!(!bundle
            .join("metadata")
            .join("manifest-migration.json")
            .exists());
    }

    #[test]
    fn allows_stopping_suspended_vm() {
        let store = temp_store();
        store.create_vm(&manifest("dev")).unwrap();
        store
            .transition_state("dev", VmRuntimeState::Running)
            .unwrap();
        store
            .transition_state("dev", VmRuntimeState::Suspended)
            .unwrap();

        let state = store
            .transition_state("dev", VmRuntimeState::Stopped)
            .unwrap();
        assert_eq!(state.state, VmRuntimeState::Stopped);
    }
}
