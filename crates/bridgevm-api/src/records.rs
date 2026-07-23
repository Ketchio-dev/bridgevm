//! Split out of lib.rs by responsibility.

use crate::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VmLogKind {
    Qemu,
    Serial,
}

impl VmLogKind {
    pub fn file_name(self) -> &'static str {
        match self {
            VmLogKind::Qemu => "qemu.log",
            VmLogKind::Serial => "serial.log",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VmRecord {
    pub name: String,
    pub mode: String,
    pub guest_os: String,
    pub guest_arch: String,
    pub state: String,
    pub path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub qmp_supervisor: Option<QmpSupervisorMetadata>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmLogViewRecord {
    pub vm: String,
    pub kind: VmLogKind,
    pub path: PathBuf,
    pub exists: bool,
    pub bytes: u64,
    pub returned_bytes: u64,
    pub truncated: bool,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeControlCommandRecord {
    pub vm: String,
    pub kind: String,
    pub socket_path: PathBuf,
    pub command: String,
    pub response: serde_json::Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SnapshotConsistency {
    CrashConsistent,
    ApplicationConsistent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotPreflightStatusRecord {
    pub vm: String,
    pub consistency: SnapshotConsistency,
    pub backend_freeze_thaw_supported: bool,
    pub guest_tools_connected: bool,
    pub capabilities: Vec<String>,
    pub ready: bool,
    pub blockers: Vec<SnapshotPreflightBlockerRecord>,
    pub checked_at_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotPreflightBlockerRecord {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplicationConsistentSnapshotExecutionRecord {
    pub vm: String,
    pub snapshot: String,
    pub freeze_request_id: String,
    pub thaw_request_id: String,
    pub pending_commands_after_freeze: usize,
    pub pending_commands_after_thaw: usize,
    pub snapshot_created_at_unix: u64,
    pub freeze_result: ApplicationConsistentSnapshotCommandResultRecord,
    pub thaw_result: ApplicationConsistentSnapshotCommandResultRecord,
    pub preflight_ready: bool,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplicationConsistentSnapshotCommandResultRecord {
    pub request_id: String,
    pub capability: Option<String>,
    pub ok: bool,
    pub error_code: Option<String>,
    pub message: Option<String>,
    pub completed_at_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticBundleMetadata {
    pub vm: String,
    pub source: PathBuf,
    pub output: PathBuf,
    pub files: Vec<PathBuf>,
    pub created_at_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PerformanceBaselineMetadata {
    pub vm: String,
    pub source: PathBuf,
    pub output: PathBuf,
    pub artifact: PathBuf,
    pub created_at_unix: u64,
    pub metadata_only: bool,
    pub state: VmRuntimeMetadata,
    pub runner: Option<RunnerMetadata>,
    pub guest_tools: GuestToolsStatusRecord,
    pub metrics: Option<GuestToolsMetricsMetadata>,
    pub measurements: Vec<PerformanceMeasurementRecord>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PerformanceMeasurementRecord {
    pub name: String,
    pub value: u64,
    pub unit: String,
    pub source: String,
    pub metadata_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PerformanceSampleMetadata {
    pub vm: String,
    pub source: PathBuf,
    pub output: PathBuf,
    pub artifact: PathBuf,
    pub probe: PathBuf,
    pub probes: Vec<PathBuf>,
    pub artifact_bytes: u64,
    pub iterations: u16,
    pub sync: bool,
    pub iteration_results: Vec<PerformanceSampleIterationRecord>,
    pub created_at_unix: u64,
    pub state: VmRuntimeMetadata,
    pub runner: Option<RunnerMetadata>,
    pub guest_tools: GuestToolsStatusRecord,
    pub metrics: Option<GuestToolsMetricsMetadata>,
    pub measurements: Vec<PerformanceMeasurementRecord>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PerformanceSampleIterationRecord {
    pub iteration: u16,
    pub probe: PathBuf,
    pub bytes: u64,
    pub write_latency_microseconds: u64,
    pub sync: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QmpStatusRecord {
    pub socket_path: PathBuf,
    pub available: bool,
    pub status: Option<String>,
    pub running: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supervisor: Option<QmpSupervisorMetadata>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QmpCommandRecord {
    pub vm: String,
    pub socket_path: PathBuf,
    pub command: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleAction {
    Suspend,
    Resume,
}

impl LifecycleAction {
    pub(crate) fn target_state(self) -> VmRuntimeState {
        match self {
            LifecycleAction::Suspend => VmRuntimeState::Suspended,
            LifecycleAction::Resume => VmRuntimeState::Running,
        }
    }

    pub(crate) fn qmp_command(self) -> &'static str {
        match self {
            LifecycleAction::Suspend => "stop",
            LifecycleAction::Resume => "cont",
        }
    }
}

impl std::fmt::Display for LifecycleAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LifecycleAction::Suspend => write!(f, "suspend"),
            LifecycleAction::Resume => write!(f, "resume"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LifecyclePlanRecord {
    pub vm: String,
    pub action: LifecycleAction,
    pub current_state: VmRuntimeState,
    pub target_state: VmRuntimeState,
    pub backend: String,
    pub metadata_only: bool,
    pub executable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub qmp_command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub socket_path: Option<PathBuf>,
    pub socket_available: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub qmp_supervisor: Option<QmpSupervisorMetadata>,
    pub blockers: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VmReadinessReport {
    pub vm: String,
    pub mode: VmMode,
    pub state: VmRuntimeState,
    pub metadata_only: bool,
    pub live_e2e_required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub live_evidence: Option<VmLiveEvidenceVerification>,
    pub evidence_requirements: Vec<VmEvidenceRequirement>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boot_media: Option<BootMediaStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boot_media_error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot_chain: Option<SnapshotChainMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot_chain_error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runner: Option<RunnerMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pre_run_launch_readiness: Option<LaunchReadinessMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub qmp_supervisor: Option<QmpSupervisorMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runner_error: Option<String>,
    pub blockers: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmEvidenceRequirement {
    pub kind: String,
    pub required: bool,
    pub proven: bool,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmLiveEvidenceVerification {
    pub path: PathBuf,
    pub backend: String,
    pub vm_name: String,
    pub boot_mode: String,
    pub disk_format: String,
    pub network: String,
    pub serial_sentinel_required: bool,
    pub serial_sentinel_proven: bool,
    #[serde(default)]
    pub graphical_boot_progress_proven: bool,
    #[serde(default)]
    pub viewer_evidence_proven: bool,
    #[serde(default)]
    pub qmp_evidence_proven: bool,
    #[serde(default)]
    pub guest_tools_effects_proven: bool,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuestToolsStatusRecord {
    pub vm: String,
    pub tools: String,
    pub token_created_at_unix: u64,
    pub capabilities: Vec<GuestToolsCapabilityRecord>,
    pub approved_shared_folders: Vec<GuestToolsApprovedSharedFolderRecord>,
    pub runtime: Option<GuestToolsRuntimeMetadata>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuestToolsCapabilityRecord {
    pub name: String,
    pub max_version: u16,
    pub enabled_by: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuestToolsApprovedSharedFolderRecord {
    pub name: String,
    pub host_path: String,
    pub host_path_token: String,
    pub read_only: bool,
    pub approval: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuestToolsSessionRecord {
    pub vm: String,
    pub guest_os: String,
    pub agent_version: Option<String>,
    pub capabilities: Vec<AgentCapability>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuestToolsTokenRecord {
    pub vm: String,
    pub token: String,
    pub created_at_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuestToolsCommandRecord {
    pub vm: String,
    pub request_id: Option<String>,
    pub pending_commands: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortForwardListRecord {
    pub vm: String,
    pub forwards: Vec<PortForwardRecord>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortForwardRecord {
    pub host: u16,
    pub guest: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkPlanRecord {
    pub vm: String,
    pub backend: String,
    pub mode: String,
    pub hostname: String,
    pub dry_run: bool,
    pub executable: bool,
    pub port_forwards: Vec<PortForwardRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<NetworkCapabilitiesRecord>,
    pub blockers: Vec<NetworkPlanBlockerRecord>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkCapabilitiesRecord {
    pub guest_outbound: bool,
    pub host_to_guest: bool,
    pub guest_to_host: bool,
    pub host_visible_hostname: bool,
    pub supports_port_forwarding: bool,
    pub requires_privileged_helper: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkPlanBlockerRecord {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SharedFolderListRecord {
    pub vm: String,
    pub shared_folders: Vec<SharedFolderRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SharedFolderRecord {
    pub name: String,
    pub host_path: String,
    pub read_only: bool,
    pub host_path_token: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SshPlanRecord {
    pub vm: String,
    pub user: String,
    pub host: String,
    pub port: u16,
    pub source: SshPlanSource,
    pub command: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SshPlanSource {
    GuestToolsIp,
    PortForward,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenPortPlanRecord {
    pub vm: String,
    pub scheme: String,
    pub host: String,
    pub guest_port: u16,
    pub host_port: u16,
    pub url: String,
    pub command: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GuestToolsLinuxCommandTransport {
    Socket,
    Device,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuestToolsLinuxCommandRecord {
    pub vm: String,
    pub transport: GuestToolsLinuxCommandTransport,
    pub command: Vec<String>,
    pub token_file: PathBuf,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BootMediaKind {
    InstallerImage,
    Kernel,
    Initrd,
    MacosRestoreImage,
}

impl std::fmt::Display for BootMediaKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BootMediaKind::InstallerImage => write!(f, "installer-image"),
            BootMediaKind::Kernel => write!(f, "kernel"),
            BootMediaKind::Initrd => write!(f, "initrd"),
            BootMediaKind::MacosRestoreImage => write!(f, "macos-restore-image"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BootMediaImportMetadata {
    pub vm: String,
    pub kind: BootMediaKind,
    pub source: PathBuf,
    pub destination: PathBuf,
    pub bytes: u64,
    pub replaced: bool,
    pub imported_at_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BootMediaStatus {
    pub vm: String,
    pub entries: Vec<BootMediaStatusEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BootMediaStatusEntry {
    pub kind: BootMediaKind,
    pub path: PathBuf,
    pub exists: bool,
    pub bytes: Option<u64>,
    pub last_import: Option<BootMediaImportMetadata>,
    pub last_verification: Option<BootMediaVerificationMetadata>,
    pub last_download_plan: Option<BootMediaDownloadPlanMetadata>,
    pub last_download: Option<BootMediaDownloadResultMetadata>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BootMediaVerificationMetadata {
    pub vm: String,
    pub kind: BootMediaKind,
    pub path: PathBuf,
    pub bytes: u64,
    pub expected_sha256: String,
    pub actual_sha256: String,
    pub verified: bool,
    pub verified_at_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BootMediaDownloadPlanMetadata {
    pub vm: String,
    pub kind: BootMediaKind,
    pub url: String,
    pub destination: PathBuf,
    pub exists: bool,
    pub bytes: Option<u64>,
    pub expected_sha256: Option<String>,
    pub last_import: Option<BootMediaImportMetadata>,
    pub last_verification: Option<BootMediaVerificationMetadata>,
    pub planned_at_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BootMediaDownloadResultMetadata {
    pub vm: String,
    pub kind: BootMediaKind,
    pub url: String,
    pub destination: PathBuf,
    pub temp_path: PathBuf,
    pub command: Vec<String>,
    pub exit_status: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub bytes: Option<u64>,
    pub replaced: bool,
    pub expected_sha256: Option<String>,
    pub actual_sha256: Option<String>,
    pub verified: Option<bool>,
    pub downloaded: bool,
    pub downloaded_at_unix: u64,
}

#[cfg(test)]
#[path = "records_tests/mod.rs"]
mod tests;
