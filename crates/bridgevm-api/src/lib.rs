use bridgevm_agent_protocol::{AgentCapability, AgentEnvelope, AgentMessage};
use bridgevm_agentd::{accept_guest_hello, AgentPolicy};
use bridgevm_apple_vz::{
    build_fast_plan, write_launch_spec_artifact, AppleVzBootSpec, AppleVzPathSpec,
    AppleVzReadinessSpec,
};
use bridgevm_config::{BootMode, Guest, PortForward, SharedFolder, VmManifest, VmMode};
use bridgevm_core::{
    available_boot_templates, boot_template_by_id, current_engine_descriptor_for_mode,
    recommend_mode, BootTemplate, EngineLane, GuestChoice, ModeRecommendation,
};
use bridgevm_network::{
    plan_network, NetworkBackend, NetworkCapabilities, NetworkMode, NetworkPlanError,
    PortForwardRule,
};
use bridgevm_qemu::{
    assign_free_vnc_display, build_compatibility_command, cont as qmp_cont,
    is_qmp_status_unavailable, qmp_socket_path, query_status, quit as qmp_quit,
    secure_boot_vars_path, stop as qmp_stop, suspend_to_snapshot, swtpm_socket_path, QemuCommand,
    QemuError, COMPAT_SUSPEND_SNAPSHOT_TAG,
};
use bridgevm_storage::{
    ApplicationConsistentSnapshotPreflightMetadata, DiskCompactMetadata, DiskCreateMetadata,
    DiskInspectMetadata, DiskPreparationMetadata, DiskVerifyMetadata, GuestToolsMetricsMetadata,
    GuestToolsRuntimeMetadata, LaunchReadinessBlockerMetadata, LaunchReadinessMetadata,
    QmpSupervisorMetadata, RunnerMetadata, RuntimeControlMetadata, RuntimeResourcePolicyBlocker,
    RuntimeResourcePolicyMetadata, RuntimeResourceVisibility, SnapshotChainMetadata,
    SnapshotDiskCreateMetadata, SnapshotDiskMetadata, SnapshotKind, SnapshotMetadata,
    SnapshotRestoreMetadata, VmCloneMetadata, VmDeletionMetadata, VmExportMetadata,
    VmImportMetadata, VmManifestMigrationMetadata, VmMetadataRepairMetadata, VmRuntimeMetadata,
    VmRuntimeState, VmStore,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::{
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Read, Seek, SeekFrom, Write},
    net::IpAddr,
    os::unix::net::UnixStream,
    path::{Component, Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const DEFAULT_GUEST_TOOLS_LINUX_DEVICE: &str = "/dev/virtio-ports/org.bridgevm.guest-tools.0";
const DEFAULT_PERFORMANCE_SAMPLE_ARTIFACT_BYTES: u64 = 1_048_576;
const MAX_PERFORMANCE_SAMPLE_ARTIFACT_BYTES: u64 = 64 * 1024 * 1024;
const DEFAULT_PERFORMANCE_SAMPLE_ITERATIONS: u16 = 1;
const MAX_PERFORMANCE_SAMPLE_ITERATIONS: u16 = 100;
const MAX_PERFORMANCE_SAMPLE_TOTAL_BYTES: u64 = 256 * 1024 * 1024;
const DEFAULT_LOG_VIEW_BYTES: u64 = 8 * 1024;
const MAX_LOG_VIEW_BYTES: u64 = 1024 * 1024;
const MAX_BOOT_MEDIA_METADATA_BYTES: u64 = 1024 * 1024;
const MAX_EVIDENCE_TEXT_BYTES: u64 = 16 * 1024 * 1024;
const MAX_RUNTIME_CONTROL_RESPONSE_BYTES: u64 = 64 * 1024;

pub const BRIDGEVM_API_SCHEMA_ID: &str = "bridgevm.api/v1";
pub const BRIDGEVM_API_CONTRACT_VERSION: u16 = 1;
pub const BRIDGEVM_API_SERVICE_NAME: &str = "bridgevm.api.v1.BridgeVmService";
pub const BRIDGEVM_API_JSON_OVER_UDS_TRANSPORT: &str = "json-ndjson-over-uds";
pub const BRIDGEVM_API_GRPC_OVER_UDS_TRANSPORT: &str = "grpc-over-uds";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurrentRuntimeEngine {
    AppleVz,
    QemuCompatibility,
}

impl CurrentRuntimeEngine {
    pub fn for_mode(mode: VmMode) -> Self {
        match current_engine_descriptor_for_mode(mode).lane {
            EngineLane::AppleVz => Self::AppleVz,
            EngineLane::QemuCompatibility => Self::QemuCompatibility,
            EngineLane::BridgeHvf => {
                unreachable!("BridgeVM HVF is a target engine, not a current VmMode runtime")
            }
        }
    }

    pub fn for_manifest(manifest: &VmManifest) -> Self {
        Self::for_mode(manifest.mode)
    }

    pub fn network_backend(self) -> NetworkBackend {
        match self {
            Self::AppleVz => NetworkBackend::AppleVz,
            Self::QemuCompatibility => NetworkBackend::Qemu,
        }
    }

    pub fn runner_metadata_engine(self) -> &'static str {
        match self {
            Self::AppleVz => "lightvm",
            Self::QemuCompatibility => "fullvm",
        }
    }

    pub fn uses_qmp(self) -> bool {
        matches!(self, Self::QemuCompatibility)
    }

    pub fn lifecycle_backend_label(self) -> &'static str {
        match self {
            Self::AppleVz => "apple-vz",
            Self::QemuCompatibility => "qemu-qmp",
        }
    }
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct BridgeVmServiceContract {
    pub schema_id: &'static str,
    pub version: u16,
    pub service: &'static str,
    pub request_type: &'static str,
    pub response_type: &'static str,
    pub transport: &'static str,
}

impl BridgeVmServiceContract {
    pub const fn json_over_uds() -> Self {
        Self {
            schema_id: BRIDGEVM_API_SCHEMA_ID,
            version: BRIDGEVM_API_CONTRACT_VERSION,
            service: BRIDGEVM_API_SERVICE_NAME,
            request_type: "BridgeVmRequest",
            response_type: "BridgeVmResponse",
            transport: BRIDGEVM_API_JSON_OVER_UDS_TRANSPORT,
        }
    }

    pub const fn grpc_over_uds() -> Self {
        Self {
            transport: BRIDGEVM_API_GRPC_OVER_UDS_TRANSPORT,
            ..Self::json_over_uds()
        }
    }

    pub fn is_same_contract_as(&self, other: &Self) -> bool {
        self.schema_id == other.schema_id
            && self.version == other.version
            && self.service == other.service
            && self.request_type == other.request_type
            && self.response_type == other.response_type
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateVmRequest {
    pub manifest: VmManifest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmSummary {
    pub name: String,
    pub mode: String,
    pub guest: String,
    pub arch: String,
    pub state: String,
}

pub trait VmService {
    type Error;

    fn list_vms(&self) -> Result<Vec<VmSummary>, Self::Error>;
    fn create_vm(&self, request: CreateVmRequest) -> Result<VmSummary, Self::Error>;
    fn start_vm(&self, name: &str) -> Result<VmSummary, Self::Error>;
    fn suspend_vm(&self, name: &str) -> Result<VmSummary, Self::Error>;
    fn resume_vm(&self, name: &str) -> Result<VmSummary, Self::Error>;
    fn stop_vm(&self, name: &str) -> Result<VmSummary, Self::Error>;
}

pub trait ModeService {
    fn recommend_mode(&self, choice: GuestChoice) -> ModeRecommendation;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BridgeVmRequest {
    Doctor,
    ListVms,
    ListTemplates,
    CreateVm {
        manifest: VmManifest,
    },
    CreateVmFromTemplate {
        name: String,
        template_id: String,
    },
    GetVm {
        name: String,
    },
    DeleteVm {
        name: String,
        #[serde(default, skip_serializing_if = "is_false")]
        metadata_only: bool,
    },
    ExportVm {
        name: String,
        output: PathBuf,
    },
    ImportVm {
        input: PathBuf,
        name: Option<String>,
    },
    CloneVm {
        name: String,
        new_name: String,
        #[serde(default, skip_serializing_if = "is_false")]
        linked: bool,
    },
    RepairMetadata {
        name: String,
    },
    MigrateManifest {
        name: String,
        #[serde(default, skip_serializing_if = "is_false")]
        dry_run: bool,
    },
    CreateDiagnosticBundle {
        name: String,
        output: PathBuf,
    },
    ViewLogs {
        name: String,
        kind: VmLogKind,
        max_bytes: Option<u64>,
    },
    CreatePerformanceBaseline {
        name: String,
        output: PathBuf,
    },
    CreatePerformanceSample {
        name: String,
        output: PathBuf,
        artifact_bytes: Option<u64>,
        iterations: Option<u16>,
        sync: bool,
    },
    ReadinessReport {
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        live_evidence: Option<PathBuf>,
        #[serde(default, skip_serializing_if = "is_false")]
        record_live_evidence: bool,
        #[serde(default, skip_serializing_if = "is_false")]
        clear_live_evidence: bool,
    },
    TransitionVm {
        name: String,
        state: VmRuntimeState,
    },
    RestartVm {
        name: String,
    },
    CreateSnapshot {
        vm: String,
        name: String,
        kind: SnapshotKind,
    },
    ListSnapshots {
        vm: String,
    },
    SnapshotChain {
        vm: String,
    },
    SnapshotPreflightStatus {
        name: String,
        consistency: SnapshotConsistency,
    },
    ExecuteApplicationConsistentSnapshot {
        vm: String,
        name: String,
        freeze_timeout_millis: Option<u64>,
    },
    RestoreSnapshot {
        vm: String,
        name: String,
    },
    CreateSnapshotDisk {
        vm: String,
        name: String,
    },
    QemuArgs {
        name: String,
    },
    PrepareRun {
        name: String,
    },
    InspectBootMedia {
        name: String,
    },
    ImportBootMedia {
        name: String,
        source: PathBuf,
        kind: Option<BootMediaKind>,
    },
    InspectBootMediaStatus {
        name: String,
    },
    VerifyBootMedia {
        name: String,
        expected_sha256: String,
        kind: Option<BootMediaKind>,
    },
    PlanBootMediaDownload {
        name: String,
        url: String,
        expected_sha256: Option<String>,
        kind: Option<BootMediaKind>,
    },
    DownloadBootMedia {
        name: String,
        kind: Option<BootMediaKind>,
    },
    PrepareDisk {
        name: String,
    },
    CreateDisk {
        name: String,
    },
    InspectDisk {
        name: String,
    },
    VerifyDisk {
        name: String,
    },
    CompactDisk {
        name: String,
    },
    ListPorts {
        name: String,
    },
    AddPort {
        name: String,
        host: u16,
        guest: u16,
    },
    RemovePort {
        name: String,
        host: u16,
        guest: u16,
    },
    PlanNetwork {
        name: String,
    },
    ListShares {
        name: String,
    },
    AddShare {
        name: String,
        share: String,
        host_path: String,
        read_only: bool,
        host_path_token: Option<String>,
    },
    RemoveShare {
        name: String,
        share: String,
    },
    SshPlan {
        name: String,
        user: Option<String>,
    },
    OpenPort {
        name: String,
        guest: u16,
        scheme: Option<String>,
    },
    RunBackend {
        name: String,
        spawn: bool,
    },
    SuspendBackend {
        name: String,
    },
    ResumeBackend {
        name: String,
    },
    LifecyclePlan {
        name: String,
        action: LifecycleAction,
    },
    ReapplyRuntimeResources {
        name: String,
        visibility: RuntimeResourceVisibility,
    },
    StopBackend {
        name: String,
    },
    RunnerStatus {
        name: String,
    },
    RuntimeControl {
        name: String,
        command: String,
    },
    QmpSocket {
        name: String,
    },
    QmpStatus {
        name: String,
    },
    QmpStop {
        name: String,
    },
    QmpCont {
        name: String,
    },
    GuestToolsStatus {
        name: String,
    },
    GuestToolsToken {
        name: String,
    },
    GuestToolsAcceptHello {
        name: String,
        envelope: AgentEnvelope,
    },
    GuestToolsSendCommand {
        name: String,
        envelope: AgentEnvelope,
    },
    GuestToolsMountApprovedShare {
        name: String,
        share: String,
        request_id: Option<String>,
    },
    GuestToolsLinuxCommand {
        name: String,
        transport: GuestToolsLinuxCommandTransport,
        token_file: Option<PathBuf>,
        device: Option<PathBuf>,
    },
    RecommendMode {
        choice: GuestChoice,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BridgeVmResponse {
    Doctor {
        store_root: PathBuf,
        vms_dir: PathBuf,
        status: String,
    },
    VmList {
        vms: Vec<VmRecord>,
    },
    BootTemplates {
        templates: Vec<BootTemplate>,
    },
    Vm {
        vm: VmRecord,
    },
    Deleted {
        path: PathBuf,
        #[serde(default, skip_serializing_if = "is_false")]
        metadata_only: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        metadata: Option<VmDeletionMetadata>,
    },
    Exported {
        export: VmExportMetadata,
    },
    Imported {
        import: VmImportMetadata,
    },
    Cloned {
        clone: VmCloneMetadata,
    },
    MetadataRepaired {
        repair: VmMetadataRepairMetadata,
    },
    ManifestMigrated {
        migration: VmManifestMigrationMetadata,
    },
    DiagnosticBundle {
        bundle: DiagnosticBundleMetadata,
    },
    LogsViewed {
        log: VmLogViewRecord,
    },
    PerformanceBaseline {
        baseline: PerformanceBaselineMetadata,
    },
    PerformanceSample {
        sample: PerformanceSampleMetadata,
    },
    ReadinessReport {
        report: VmReadinessReport,
    },
    State {
        name: String,
        metadata: VmRuntimeMetadata,
    },
    Snapshot {
        snapshot: SnapshotMetadata,
        disk: Option<SnapshotDiskMetadata>,
        application_consistent_preflight: Option<ApplicationConsistentSnapshotPreflightMetadata>,
    },
    SnapshotList {
        snapshots: Vec<SnapshotMetadata>,
    },
    SnapshotChain {
        chain: SnapshotChainMetadata,
    },
    SnapshotPreflightStatus {
        preflight: SnapshotPreflightStatusRecord,
    },
    ApplicationConsistentSnapshotExecution {
        execution: ApplicationConsistentSnapshotExecutionRecord,
    },
    SnapshotRestored {
        restore: SnapshotRestoreMetadata,
    },
    SnapshotDiskCreated {
        metadata: SnapshotDiskCreateMetadata,
    },
    QemuCommand {
        command: QemuCommand,
    },
    DiskPrepared {
        metadata: DiskPreparationMetadata,
    },
    DiskCreated {
        metadata: DiskCreateMetadata,
    },
    DiskInspected {
        metadata: DiskInspectMetadata,
    },
    DiskVerified {
        metadata: DiskVerifyMetadata,
    },
    DiskCompacted {
        metadata: DiskCompactMetadata,
    },
    PortForwards {
        ports: PortForwardListRecord,
    },
    NetworkPlanned {
        plan: NetworkPlanRecord,
    },
    SharedFolders {
        shares: SharedFolderListRecord,
    },
    SshPlan {
        plan: SshPlanRecord,
    },
    OpenPortPlan {
        plan: OpenPortPlanRecord,
    },
    RunnerStatus {
        metadata: Option<RunnerMetadata>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        qmp_supervisor: Option<QmpSupervisorMetadata>,
    },
    RuntimeControl {
        control: RuntimeControlCommandRecord,
    },
    LifecyclePlan {
        plan: LifecyclePlanRecord,
    },
    RuntimeResourcePolicy {
        policy: RuntimeResourcePolicyMetadata,
    },
    BootMedia {
        name: String,
        boot: AppleVzBootSpec,
    },
    BootMediaImported {
        import: BootMediaImportMetadata,
    },
    BootMediaStatus {
        status: BootMediaStatus,
    },
    BootMediaVerified {
        verification: BootMediaVerificationMetadata,
    },
    BootMediaDownloadPlanned {
        plan: BootMediaDownloadPlanMetadata,
    },
    BootMediaDownloaded {
        download: BootMediaDownloadResultMetadata,
    },
    QmpSocket {
        path: PathBuf,
    },
    QmpStatus {
        status: QmpStatusRecord,
    },
    QmpCommandExecuted {
        command: QmpCommandRecord,
    },
    GuestToolsStatus {
        status: GuestToolsStatusRecord,
    },
    GuestToolsToken {
        token: GuestToolsTokenRecord,
    },
    GuestToolsSession {
        session: GuestToolsSessionRecord,
    },
    GuestToolsCommand {
        command: GuestToolsCommandRecord,
    },
    GuestToolsLinuxCommand {
        command: GuestToolsLinuxCommandRecord,
    },
    ModeRecommendation {
        recommendation: ModeRecommendation,
    },
    Error {
        message: String,
    },
}

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
    fn target_state(self) -> VmRuntimeState {
        match self {
            LifecycleAction::Suspend => VmRuntimeState::Suspended,
            LifecycleAction::Resume => VmRuntimeState::Running,
        }
    }

    fn qmp_command(self) -> &'static str {
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

impl BridgeVmResponse {
    pub fn into_result(self) -> Result<Self, String> {
        match self {
            BridgeVmResponse::Error { message } => Err(message),
            response => Ok(response),
        }
    }
}

pub fn handle_request(store: &VmStore, request: BridgeVmRequest) -> BridgeVmResponse {
    match handle_request_result(store, request) {
        Ok(response) => response,
        Err(message) => BridgeVmResponse::Error { message },
    }
}

fn handle_request_result(
    store: &VmStore,
    request: BridgeVmRequest,
) -> Result<BridgeVmResponse, String> {
    match request {
        BridgeVmRequest::Doctor => {
            store.ensure().map_err(|error| error.to_string())?;
            Ok(BridgeVmResponse::Doctor {
                store_root: store.root().to_path_buf(),
                vms_dir: store.vms_dir(),
                status: "OK".to_string(),
            })
        }
        BridgeVmRequest::ListTemplates => Ok(BridgeVmResponse::BootTemplates {
            templates: available_boot_templates(),
        }),
        BridgeVmRequest::ListVms => Ok(BridgeVmResponse::VmList {
            vms: records(store).map_err(|error| error.to_string())?,
        }),
        BridgeVmRequest::CreateVm { manifest } => {
            store
                .create_vm(&manifest)
                .map_err(|error| error.to_string())?;
            Ok(BridgeVmResponse::Vm {
                vm: record_for(store, &manifest.name).map_err(|error| error.to_string())?,
            })
        }
        BridgeVmRequest::CreateVmFromTemplate { name, template_id } => {
            let manifest = manifest_from_template(name, &template_id)?;
            store
                .create_vm(&manifest)
                .map_err(|error| error.to_string())?;
            Ok(BridgeVmResponse::Vm {
                vm: record_for(store, &manifest.name).map_err(|error| error.to_string())?,
            })
        }
        BridgeVmRequest::GetVm { name } => Ok(BridgeVmResponse::Vm {
            vm: record_for(store, &name).map_err(|error| error.to_string())?,
        }),
        BridgeVmRequest::DeleteVm {
            name,
            metadata_only,
        } => {
            let state = store.state(&name).map_err(|error| error.to_string())?;
            if state.state == VmRuntimeState::Running {
                return Err("refusing to delete a running VM; stop it first".to_string());
            }
            if metadata_only {
                let metadata = store
                    .delete_vm_metadata_only(&name)
                    .map_err(|error| error.to_string())?;
                return Ok(BridgeVmResponse::Deleted {
                    path: metadata.bundle.clone(),
                    metadata_only: true,
                    metadata: Some(metadata),
                });
            }
            Ok(BridgeVmResponse::Deleted {
                path: store.delete_vm(&name).map_err(|error| error.to_string())?,
                metadata_only: false,
                metadata: None,
            })
        }
        BridgeVmRequest::ExportVm { name, output } => Ok(BridgeVmResponse::Exported {
            export: store
                .export_vm(&name, output)
                .map_err(|error| error.to_string())?,
        }),
        BridgeVmRequest::ImportVm { input, name } => Ok(BridgeVmResponse::Imported {
            import: store
                .import_vm(input, name.as_deref())
                .map_err(|error| error.to_string())?,
        }),
        BridgeVmRequest::CloneVm {
            name,
            new_name,
            linked,
        } => Ok(BridgeVmResponse::Cloned {
            clone: store
                .clone_vm(&name, &new_name, linked)
                .map_err(|error| error.to_string())?,
        }),
        BridgeVmRequest::RepairMetadata { name } => Ok(BridgeVmResponse::MetadataRepaired {
            repair: store
                .repair_metadata(&name)
                .map_err(|error| error.to_string())?,
        }),
        BridgeVmRequest::MigrateManifest { name, dry_run } => {
            Ok(BridgeVmResponse::ManifestMigrated {
                migration: store
                    .migrate_manifest(&name, dry_run)
                    .map_err(|error| error.to_string())?,
            })
        }
        BridgeVmRequest::CreateDiagnosticBundle { name, output } => {
            Ok(BridgeVmResponse::DiagnosticBundle {
                bundle: create_diagnostic_bundle(store, &name, output)?,
            })
        }
        BridgeVmRequest::ViewLogs {
            name,
            kind,
            max_bytes,
        } => Ok(BridgeVmResponse::LogsViewed {
            log: view_vm_log(store, &name, kind, max_bytes)?,
        }),
        BridgeVmRequest::CreatePerformanceBaseline { name, output } => {
            Ok(BridgeVmResponse::PerformanceBaseline {
                baseline: create_performance_baseline(store, &name, output)?,
            })
        }
        BridgeVmRequest::CreatePerformanceSample {
            name,
            output,
            artifact_bytes,
            iterations,
            sync,
        } => Ok(BridgeVmResponse::PerformanceSample {
            sample: create_performance_sample(
                store,
                &name,
                output,
                artifact_bytes,
                iterations,
                sync,
            )?,
        }),
        BridgeVmRequest::ReadinessReport {
            name,
            live_evidence,
            record_live_evidence,
            clear_live_evidence,
        } => Ok(BridgeVmResponse::ReadinessReport {
            report: readiness_report_with_live_evidence_options(
                store,
                &name,
                live_evidence.as_deref(),
                record_live_evidence,
                clear_live_evidence,
            )?,
        }),
        BridgeVmRequest::TransitionVm { name, state } => Ok(BridgeVmResponse::State {
            name: name.clone(),
            metadata: store
                .transition_state(&name, state)
                .map_err(|error| error.to_string())?,
        }),
        BridgeVmRequest::RestartVm { name } => Ok(BridgeVmResponse::State {
            name: name.clone(),
            metadata: restart_vm(store, &name)?,
        }),
        BridgeVmRequest::CreateSnapshot { vm, name, kind } => {
            let snapshot = store
                .create_snapshot(&vm, &name, kind)
                .map_err(|error| error.to_string())?;
            let disk = store
                .snapshot_disk_metadata(&vm, &name)
                .map_err(|error| error.to_string())?;
            let application_consistent_preflight = store
                .application_consistent_snapshot_preflight_metadata(&vm, &name)
                .map_err(|error| error.to_string())?;
            Ok(BridgeVmResponse::Snapshot {
                snapshot,
                disk,
                application_consistent_preflight,
            })
        }
        BridgeVmRequest::ListSnapshots { vm } => Ok(BridgeVmResponse::SnapshotList {
            snapshots: store.snapshots(&vm).map_err(|error| error.to_string())?,
        }),
        BridgeVmRequest::SnapshotChain { vm } => Ok(BridgeVmResponse::SnapshotChain {
            chain: store
                .snapshot_chain(&vm)
                .map_err(|error| error.to_string())?,
        }),
        BridgeVmRequest::SnapshotPreflightStatus { name, consistency } => {
            Ok(BridgeVmResponse::SnapshotPreflightStatus {
                preflight: snapshot_preflight_status(store, &name, consistency)
                    .map_err(|error| error.to_string())?,
            })
        }
        BridgeVmRequest::ExecuteApplicationConsistentSnapshot { .. } => Err(
            "application-consistent snapshot execution requires a bridgevmd-owned running backend"
                .to_string(),
        ),
        BridgeVmRequest::RestoreSnapshot { vm, name } => Ok(BridgeVmResponse::SnapshotRestored {
            restore: store
                .restore_snapshot(&vm, &name)
                .map_err(|error| error.to_string())?,
        }),
        BridgeVmRequest::CreateSnapshotDisk { vm, name } => {
            Ok(BridgeVmResponse::SnapshotDiskCreated {
                metadata: store
                    .create_snapshot_disk(&vm, &name)
                    .map_err(|error| error.to_string())?,
            })
        }
        BridgeVmRequest::QemuArgs { name } => {
            let (bundle, manifest, _) = store
                .get_vm_with_active_disk(&name)
                .map_err(|error| error.to_string())?;
            Ok(BridgeVmResponse::QemuCommand {
                command: build_compatibility_command(&manifest, &bundle)
                    .map_err(compatibility_qemu_command_error)?,
            })
        }
        BridgeVmRequest::PrepareRun { name } => Ok(BridgeVmResponse::RunnerStatus {
            metadata: Some(run_backend(store, &name, false)?),
            qmp_supervisor: store
                .qmp_supervisor_metadata(&name)
                .map_err(|error| error.to_string())?,
        }),
        BridgeVmRequest::InspectBootMedia { name } => {
            let (bundle, manifest, _) = store
                .get_vm_with_active_disk(&name)
                .map_err(|error| error.to_string())?;
            let plan = build_fast_plan(&manifest, &bundle).map_err(|error| error.to_string())?;
            Ok(BridgeVmResponse::BootMedia {
                name,
                boot: plan.launch_spec().boot.clone(),
            })
        }
        BridgeVmRequest::ImportBootMedia { name, source, kind } => {
            Ok(BridgeVmResponse::BootMediaImported {
                import: import_boot_media(store, &name, source, kind)?,
            })
        }
        BridgeVmRequest::InspectBootMediaStatus { name } => Ok(BridgeVmResponse::BootMediaStatus {
            status: inspect_boot_media_status(store, &name)?,
        }),
        BridgeVmRequest::VerifyBootMedia {
            name,
            expected_sha256,
            kind,
        } => Ok(BridgeVmResponse::BootMediaVerified {
            verification: verify_boot_media(store, &name, &expected_sha256, kind)?,
        }),
        BridgeVmRequest::PlanBootMediaDownload {
            name,
            url,
            expected_sha256,
            kind,
        } => Ok(BridgeVmResponse::BootMediaDownloadPlanned {
            plan: plan_boot_media_download(store, &name, &url, expected_sha256.as_deref(), kind)?,
        }),
        BridgeVmRequest::DownloadBootMedia { name, kind } => {
            Ok(BridgeVmResponse::BootMediaDownloaded {
                download: download_boot_media(store, &name, kind)?,
            })
        }
        BridgeVmRequest::PrepareDisk { name } => Ok(BridgeVmResponse::DiskPrepared {
            metadata: store
                .prepare_primary_disk(&name)
                .map_err(|error| error.to_string())?,
        }),
        BridgeVmRequest::CreateDisk { name } => Ok(BridgeVmResponse::DiskCreated {
            metadata: store
                .create_primary_disk(&name)
                .map_err(|error| error.to_string())?,
        }),
        BridgeVmRequest::InspectDisk { name } => Ok(BridgeVmResponse::DiskInspected {
            metadata: store
                .inspect_primary_disk(&name)
                .map_err(|error| error.to_string())?,
        }),
        BridgeVmRequest::VerifyDisk { name } => Ok(BridgeVmResponse::DiskVerified {
            metadata: store
                .verify_active_disk(&name)
                .map_err(|error| error.to_string())?,
        }),
        BridgeVmRequest::CompactDisk { name } => Ok(BridgeVmResponse::DiskCompacted {
            metadata: store
                .compact_active_disk(&name)
                .map_err(|error| error.to_string())?,
        }),
        BridgeVmRequest::ListPorts { name } => Ok(BridgeVmResponse::PortForwards {
            ports: list_ports(store, &name)?,
        }),
        BridgeVmRequest::AddPort { name, host, guest } => Ok(BridgeVmResponse::PortForwards {
            ports: add_port(store, &name, host, guest)?,
        }),
        BridgeVmRequest::RemovePort { name, host, guest } => Ok(BridgeVmResponse::PortForwards {
            ports: remove_port(store, &name, host, guest)?,
        }),
        BridgeVmRequest::PlanNetwork { name } => Ok(BridgeVmResponse::NetworkPlanned {
            plan: network_plan(store, &name)?,
        }),
        BridgeVmRequest::ListShares { name } => Ok(BridgeVmResponse::SharedFolders {
            shares: list_shares(store, &name)?,
        }),
        BridgeVmRequest::AddShare {
            name,
            share,
            host_path,
            read_only,
            host_path_token,
        } => Ok(BridgeVmResponse::SharedFolders {
            shares: add_share(store, &name, share, host_path, read_only, host_path_token)?,
        }),
        BridgeVmRequest::RemoveShare { name, share } => Ok(BridgeVmResponse::SharedFolders {
            shares: remove_share(store, &name, &share)?,
        }),
        BridgeVmRequest::SshPlan { name, user } => Ok(BridgeVmResponse::SshPlan {
            plan: ssh_plan(store, &name, user.as_deref())?,
        }),
        BridgeVmRequest::OpenPort {
            name,
            guest,
            scheme,
        } => Ok(BridgeVmResponse::OpenPortPlan {
            plan: open_port_plan(store, &name, guest, scheme.as_deref())?,
        }),
        BridgeVmRequest::RunBackend { name, spawn } => Ok(BridgeVmResponse::RunnerStatus {
            metadata: Some(run_backend(store, &name, spawn)?),
            qmp_supervisor: store
                .qmp_supervisor_metadata(&name)
                .map_err(|error| error.to_string())?,
        }),
        BridgeVmRequest::SuspendBackend { name } => Ok(BridgeVmResponse::RunnerStatus {
            metadata: Some(suspend_backend(store, &name)?),
            qmp_supervisor: store
                .qmp_supervisor_metadata(&name)
                .map_err(|error| error.to_string())?,
        }),
        BridgeVmRequest::ResumeBackend { name } => Ok(BridgeVmResponse::RunnerStatus {
            metadata: Some(resume_backend(store, &name)?),
            qmp_supervisor: store
                .qmp_supervisor_metadata(&name)
                .map_err(|error| error.to_string())?,
        }),
        BridgeVmRequest::LifecyclePlan { name, action } => Ok(BridgeVmResponse::LifecyclePlan {
            plan: lifecycle_plan(store, &name, action)?,
        }),
        BridgeVmRequest::ReapplyRuntimeResources { name, visibility } => {
            Ok(BridgeVmResponse::RuntimeResourcePolicy {
                policy: reapply_runtime_resources(store, &name, visibility)?,
            })
        }
        BridgeVmRequest::StopBackend { name } => Ok(BridgeVmResponse::RunnerStatus {
            metadata: stop_backend(store, &name)?,
            qmp_supervisor: store
                .qmp_supervisor_metadata(&name)
                .map_err(|error| error.to_string())?,
        }),
        BridgeVmRequest::RunnerStatus { name } => Ok(BridgeVmResponse::RunnerStatus {
            metadata: store
                .runner_metadata(&name)
                .map_err(|error| error.to_string())?,
            qmp_supervisor: store
                .qmp_supervisor_metadata(&name)
                .map_err(|error| error.to_string())?,
        }),
        BridgeVmRequest::RuntimeControl { name, command } => Ok(BridgeVmResponse::RuntimeControl {
            control: runtime_control_command(store, &name, &command)?,
        }),
        BridgeVmRequest::QmpSocket { name } => {
            let (bundle, _) = store.get_vm(&name).map_err(|error| error.to_string())?;
            Ok(BridgeVmResponse::QmpSocket {
                path: qmp_socket_path(&bundle),
            })
        }
        BridgeVmRequest::QmpStatus { name } => {
            let (bundle, _) = store.get_vm(&name).map_err(|error| error.to_string())?;
            let socket_path = qmp_socket_path(&bundle);
            let supervisor = store
                .qmp_supervisor_metadata(&name)
                .map_err(|error| error.to_string())?;
            if !socket_path.exists() {
                return Ok(BridgeVmResponse::QmpStatus {
                    status: QmpStatusRecord {
                        socket_path,
                        available: false,
                        status: None,
                        running: None,
                        supervisor,
                    },
                });
            }
            let status = match query_status(&socket_path) {
                Ok(status) => status,
                Err(error) if is_qmp_status_unavailable(&error) => {
                    return Ok(BridgeVmResponse::QmpStatus {
                        status: QmpStatusRecord {
                            socket_path,
                            available: false,
                            status: None,
                            running: None,
                            supervisor,
                        },
                    });
                }
                Err(error) => return Err(error.to_string()),
            };
            Ok(BridgeVmResponse::QmpStatus {
                status: QmpStatusRecord {
                    socket_path,
                    available: true,
                    status: Some(status.status),
                    running: Some(status.running),
                    supervisor,
                },
            })
        }
        BridgeVmRequest::QmpStop { name } => {
            let command = execute_qmp_control(store, &name, "stop", qmp_stop)?;
            Ok(BridgeVmResponse::QmpCommandExecuted { command })
        }
        BridgeVmRequest::QmpCont { name } => {
            let command = execute_qmp_control(store, &name, "cont", qmp_cont)?;
            Ok(BridgeVmResponse::QmpCommandExecuted { command })
        }
        BridgeVmRequest::GuestToolsStatus { name } => Ok(BridgeVmResponse::GuestToolsStatus {
            status: inspect_guest_tools_status(store, &name)?,
        }),
        BridgeVmRequest::GuestToolsToken { name } => Ok(BridgeVmResponse::GuestToolsToken {
            token: guest_tools_token(store, &name)?,
        }),
        BridgeVmRequest::GuestToolsAcceptHello { name, envelope } => {
            Ok(BridgeVmResponse::GuestToolsSession {
                session: accept_guest_tools_hello(store, &name, &envelope)?,
            })
        }
        BridgeVmRequest::GuestToolsLinuxCommand {
            name,
            transport,
            token_file,
            device,
        } => Ok(BridgeVmResponse::GuestToolsLinuxCommand {
            command: guest_tools_linux_command(store, &name, transport, token_file, device)?,
        }),
        BridgeVmRequest::GuestToolsSendCommand { .. }
        | BridgeVmRequest::GuestToolsMountApprovedShare { .. } => Err(
            "guest-tools command dispatch requires a bridgevmd-owned running backend".to_string(),
        ),
        BridgeVmRequest::RecommendMode { choice } => Ok(BridgeVmResponse::ModeRecommendation {
            recommendation: recommend_mode(&choice),
        }),
    }
}

fn manifest_from_template(name: String, template_id: &str) -> Result<VmManifest, String> {
    let template = boot_template_by_id(template_id)
        .ok_or_else(|| format!("unknown template id: {template_id}"))?;
    let choice = GuestChoice {
        os: template.guest_os.clone(),
        version: template.guest_version.clone(),
        arch: template.guest_arch.clone(),
    };
    let recommendation = recommend_mode(&choice);
    let mut manifest = VmManifest::new(
        name,
        recommendation.mode,
        Guest {
            os: template.guest_os.clone(),
            version: template.guest_version.clone(),
            arch: template.guest_arch.clone(),
        },
        template.primary_disk_size().unwrap_or("80GiB"),
    );
    template.apply_storage_defaults(&mut manifest.storage.primary);
    manifest.boot = Some(template.as_boot());
    Ok(manifest)
}

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

fn application_consistent_snapshot_required_capabilities(
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

fn guest_tools_capabilities(manifest: &VmManifest) -> Vec<GuestToolsCapabilityRecord> {
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

fn guest_tools_capability(name: &str, enabled_by: &str) -> GuestToolsCapabilityRecord {
    GuestToolsCapabilityRecord {
        name: name.to_string(),
        max_version: 1,
        enabled_by: enabled_by.to_string(),
    }
}

fn guest_tools_approved_shared_folders(
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

pub fn list_ports(store: &VmStore, name: &str) -> Result<PortForwardListRecord, String> {
    let (_, manifest) = store.get_vm(name).map_err(|error| error.to_string())?;
    Ok(port_forward_list(name, &manifest.network.forwards))
}

pub fn add_port(
    store: &VmStore,
    name: &str,
    host: u16,
    guest: u16,
) -> Result<PortForwardListRecord, String> {
    let (bundle, mut manifest) = store.get_vm(name).map_err(|error| error.to_string())?;
    manifest.network.forwards.push(PortForward { host, guest });
    validate_network_plan(&manifest)?;
    manifest
        .network
        .forwards
        .sort_by_key(|forward| (forward.host, forward.guest));
    manifest
        .write(&bundle.join("manifest.yaml"))
        .map_err(|error| error.to_string())?;
    Ok(port_forward_list(name, &manifest.network.forwards))
}

pub fn remove_port(
    store: &VmStore,
    name: &str,
    host: u16,
    guest: u16,
) -> Result<PortForwardListRecord, String> {
    validate_port_pair(host, guest)?;
    let (bundle, mut manifest) = store.get_vm(name).map_err(|error| error.to_string())?;
    let initial_len = manifest.network.forwards.len();
    manifest
        .network
        .forwards
        .retain(|forward| !(forward.host == host && forward.guest == guest));
    if manifest.network.forwards.len() == initial_len {
        return Err(format!("port forward {host}:{guest} is not configured"));
    }
    manifest
        .write(&bundle.join("manifest.yaml"))
        .map_err(|error| error.to_string())?;
    Ok(port_forward_list(name, &manifest.network.forwards))
}

pub fn network_plan(store: &VmStore, name: &str) -> Result<NetworkPlanRecord, String> {
    let (_, manifest) = store.get_vm(name).map_err(|error| error.to_string())?;
    Ok(network_plan_for_manifest(&manifest))
}

fn network_plan_for_manifest(manifest: &VmManifest) -> NetworkPlanRecord {
    let backend = CurrentRuntimeEngine::for_manifest(manifest).network_backend();
    let port_forwards = manifest
        .network
        .forwards
        .iter()
        .map(|forward| PortForwardRecord {
            host: forward.host,
            guest: forward.guest,
        })
        .collect::<Vec<_>>();
    let rules = manifest
        .network
        .forwards
        .iter()
        .map(|forward| PortForwardRule {
            host: forward.host,
            guest: forward.guest,
        })
        .collect::<Vec<_>>();
    let hostname = manifest.network.hostname.clone();
    let mut blockers = Vec::new();
    let mut notes = vec![
        "dry-run network plan; no backend launch or host networking mutation was performed"
            .to_string(),
    ];
    let mut capabilities = None;

    match manifest.network.mode.parse::<NetworkMode>() {
        Ok(mode) => match plan_network(backend, mode.clone(), hostname.clone(), rules) {
            Ok(plan) => {
                capabilities = Some(network_capabilities_record(&plan.capabilities));
                blockers.extend(plan.requirements.into_iter().map(|requirement| {
                    NetworkPlanBlockerRecord {
                        code: requirement.blocker,
                        message: requirement.requirement,
                    }
                }));
                notes.extend(plan.notes);
            }
            Err(error) => blockers.push(network_plan_error_blocker(error)),
        },
        Err(error) => blockers.push(network_plan_error_blocker(error)),
    }

    NetworkPlanRecord {
        vm: manifest.name.clone(),
        backend: network_backend_label(backend).to_string(),
        mode: manifest.network.mode.clone(),
        hostname,
        dry_run: true,
        executable: blockers.is_empty(),
        port_forwards,
        capabilities,
        blockers,
        notes,
    }
}

fn network_capabilities_record(capabilities: &NetworkCapabilities) -> NetworkCapabilitiesRecord {
    NetworkCapabilitiesRecord {
        guest_outbound: capabilities.guest_outbound,
        host_to_guest: capabilities.host_to_guest,
        guest_to_host: capabilities.guest_to_host,
        host_visible_hostname: capabilities.host_visible_hostname,
        supports_port_forwarding: capabilities.supports_port_forwarding,
        requires_privileged_helper: capabilities.requires_privileged_helper,
    }
}

fn network_backend_label(backend: NetworkBackend) -> &'static str {
    match backend {
        NetworkBackend::AppleVz => "apple-vz",
        NetworkBackend::Qemu => "qemu",
    }
}

fn network_plan_error_blocker(error: NetworkPlanError) -> NetworkPlanBlockerRecord {
    let code = match &error {
        NetworkPlanError::UnsupportedModeName(_) => "unsupported-network-mode",
        NetworkPlanError::EmptyHostname => "empty-network-hostname",
        NetworkPlanError::InvalidPortForward { .. } => "invalid-port-forward",
        NetworkPlanError::DuplicateHostPort { .. } => "duplicate-host-port",
        NetworkPlanError::UnsupportedMode { .. } => "unsupported-network-backend-mode",
        NetworkPlanError::UnsupportedPortForwarding { .. } => "unsupported-port-forwarding",
    };
    NetworkPlanBlockerRecord {
        code: code.to_string(),
        message: error.to_string(),
    }
}

fn validate_port_pair(host: u16, guest: u16) -> Result<(), String> {
    if host == 0 || guest == 0 {
        return Err("ports must be between 1 and 65535".to_string());
    }
    Ok(())
}

fn validate_network_plan(manifest: &VmManifest) -> Result<(), String> {
    let mode = manifest
        .network
        .mode
        .parse::<NetworkMode>()
        .map_err(|error| error.to_string())?;
    let backend = CurrentRuntimeEngine::for_manifest(manifest).network_backend();
    let forwards = manifest
        .network
        .forwards
        .iter()
        .map(|forward| PortForwardRule {
            host: forward.host,
            guest: forward.guest,
        })
        .collect();
    plan_network(backend, mode, manifest.network.hostname.clone(), forwards)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn port_forward_list(name: &str, forwards: &[PortForward]) -> PortForwardListRecord {
    PortForwardListRecord {
        vm: name.to_string(),
        forwards: forwards
            .iter()
            .map(|forward| PortForwardRecord {
                host: forward.host,
                guest: forward.guest,
            })
            .collect(),
    }
}

pub fn list_shares(store: &VmStore, name: &str) -> Result<SharedFolderListRecord, String> {
    let (_, manifest) = store.get_vm(name).map_err(|error| error.to_string())?;
    Ok(shared_folder_list(name, &manifest.shared_folders))
}

pub fn add_share(
    store: &VmStore,
    name: &str,
    share: String,
    host_path: String,
    read_only: bool,
    host_path_token: Option<String>,
) -> Result<SharedFolderListRecord, String> {
    let (bundle, mut manifest) = store.get_vm(name).map_err(|error| error.to_string())?;
    manifest.shared_folders.push(SharedFolder {
        name: share,
        host_path,
        read_only,
        host_path_token,
    });
    manifest.validate().map_err(|error| error.to_string())?;
    if let Some(folder) = manifest.shared_folders.last_mut() {
        folder.host_path = canonical_share_host_path(&folder.host_path)?;
    }
    manifest
        .shared_folders
        .sort_by(|left, right| left.name.cmp(&right.name));
    manifest
        .write(&bundle.join("manifest.yaml"))
        .map_err(|error| error.to_string())?;
    Ok(shared_folder_list(name, &manifest.shared_folders))
}

fn canonical_share_host_path(host_path: &str) -> Result<String, String> {
    let requested = Path::new(host_path);
    if !requested.is_absolute() {
        return Err("shared folder hostPath must be an absolute path".to_string());
    }
    if requested
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err("shared folder hostPath cannot contain '..' components".to_string());
    }
    reject_symlink_components(requested)?;
    let canonical = std::fs::canonicalize(host_path).map_err(|error| {
        format!("shared folder hostPath '{host_path}' is not accessible: {error}")
    })?;
    if !canonical.is_dir() {
        return Err(format!(
            "shared folder hostPath '{}' must be an existing directory",
            canonical.display()
        ));
    }
    canonical
        .into_os_string()
        .into_string()
        .map_err(|_| "shared folder hostPath must be valid UTF-8".to_string())
}

fn reject_symlink_components(path: &Path) -> Result<(), String> {
    let mut current = PathBuf::new();
    let mut normal_components = 0usize;
    for component in path.components() {
        current.push(component.as_os_str());
        if matches!(component, Component::Normal(_)) {
            normal_components += 1;
        }
        let metadata = fs::symlink_metadata(&current).map_err(|error| {
            format!(
                "shared folder hostPath '{}' is not accessible: {error}",
                path.display()
            )
        })?;
        if metadata.file_type().is_symlink() {
            if normal_components <= 1 {
                continue;
            }
            return Err(format!(
                "shared folder hostPath '{}' cannot traverse symlink '{}'",
                path.display(),
                current.display()
            ));
        }
    }
    Ok(())
}

pub fn remove_share(
    store: &VmStore,
    name: &str,
    share: &str,
) -> Result<SharedFolderListRecord, String> {
    let (bundle, mut manifest) = store.get_vm(name).map_err(|error| error.to_string())?;
    let initial_len = manifest.shared_folders.len();
    manifest
        .shared_folders
        .retain(|folder| folder.name != share);
    if manifest.shared_folders.len() == initial_len {
        return Err(format!("shared folder '{share}' is not configured"));
    }
    manifest
        .write(&bundle.join("manifest.yaml"))
        .map_err(|error| error.to_string())?;
    Ok(shared_folder_list(name, &manifest.shared_folders))
}

fn shared_folder_list(name: &str, shared_folders: &[SharedFolder]) -> SharedFolderListRecord {
    SharedFolderListRecord {
        vm: name.to_string(),
        shared_folders: shared_folders
            .iter()
            .map(|folder| SharedFolderRecord {
                name: folder.name.clone(),
                host_path: folder.host_path.clone(),
                read_only: folder.read_only,
                host_path_token: folder.resolved_host_path_token(),
            })
            .collect(),
    }
}

pub fn ssh_plan(store: &VmStore, name: &str, user: Option<&str>) -> Result<SshPlanRecord, String> {
    let (_, manifest) = store.get_vm(name).map_err(|error| error.to_string())?;
    let user = user.unwrap_or("user").to_string();
    if user.trim().is_empty() {
        return Err("ssh user must not be empty".to_string());
    }

    if manifest.mode == VmMode::Compatibility {
        if let Some(plan) = ssh_plan_from_forward(name, user.clone(), &manifest.network.forwards) {
            return Ok(plan);
        }
    }

    if let Some(runtime) = store
        .guest_tools_runtime_metadata(name)
        .map_err(|error| error.to_string())?
    {
        if runtime.connected {
            if let Some(address) = runtime
                .guest_ip_addresses
                .iter()
                .map(|address| address.address.trim())
                .find(|address| valid_guest_ip(address))
            {
                return Ok(ssh_plan_record(
                    name,
                    user,
                    address.to_string(),
                    22,
                    SshPlanSource::GuestToolsIp,
                ));
            }
        }
    }

    if manifest.mode != VmMode::Compatibility {
        if let Some(plan) = ssh_plan_from_forward(name, user, &manifest.network.forwards) {
            return Ok(plan);
        }
    }

    Err(format!(
        "no SSH target available for {name}; report a reachable guest IP through guest tools or add a port forward such as 2222:22"
    ))
}

pub fn open_port_plan(
    store: &VmStore,
    name: &str,
    guest_port: u16,
    scheme: Option<&str>,
) -> Result<OpenPortPlanRecord, String> {
    if guest_port == 0 {
        return Err("guest port must be between 1 and 65535".to_string());
    }
    let scheme = normalized_url_scheme(scheme.unwrap_or("http"))?;
    let (_, manifest) = store.get_vm(name).map_err(|error| error.to_string())?;
    let forward = manifest
        .network
        .forwards
        .iter()
        .filter(|forward| forward.guest == guest_port)
        .min_by_key(|forward| forward.host)
        .ok_or_else(|| {
            format!(
                "no host port is forwarded to guest port {guest_port}; add one with: bridgevm port add {name} <host>:{guest_port}"
            )
        })?;
    let host = "127.0.0.1".to_string();
    let url = format!("{scheme}://{host}:{}", forward.host);
    Ok(OpenPortPlanRecord {
        vm: name.to_string(),
        scheme,
        host,
        guest_port,
        host_port: forward.host,
        command: vec!["open".to_string(), url.clone()],
        url,
    })
}

fn normalized_url_scheme(scheme: &str) -> Result<String, String> {
    let scheme = scheme.trim().to_ascii_lowercase();
    if scheme.is_empty() {
        return Err("URL scheme must not be empty".to_string());
    }
    let mut chars = scheme.chars();
    let Some(first) = chars.next() else {
        return Err("URL scheme must not be empty".to_string());
    };
    if !first.is_ascii_alphabetic() {
        return Err("URL scheme must start with an ASCII letter".to_string());
    }
    if !chars.all(|char| char.is_ascii_alphanumeric() || matches!(char, '+' | '-' | '.')) {
        return Err(
            "URL scheme may only contain ASCII letters, digits, '+', '-', or '.'".to_string(),
        );
    }
    Ok(scheme)
}

fn ssh_plan_from_forward(
    name: &str,
    user: String,
    forwards: &[PortForward],
) -> Option<SshPlanRecord> {
    forwards
        .iter()
        .filter(|forward| forward.guest == 22)
        .min_by_key(|forward| forward.host)
        .map(|forward| {
            ssh_plan_record(
                name,
                user,
                "127.0.0.1".to_string(),
                forward.host,
                SshPlanSource::PortForward,
            )
        })
}

fn valid_guest_ip(address: &str) -> bool {
    match address.parse::<IpAddr>() {
        Ok(IpAddr::V4(address)) => {
            !address.is_unspecified() && !address.is_loopback() && !address.is_link_local()
        }
        Ok(IpAddr::V6(address)) => {
            !address.is_unspecified() && !address.is_loopback() && !address.is_unicast_link_local()
        }
        Err(_) => false,
    }
}

fn ssh_plan_record(
    name: &str,
    user: String,
    host: String,
    port: u16,
    source: SshPlanSource,
) -> SshPlanRecord {
    let mut command = vec!["ssh".to_string()];
    if port != 22 {
        command.extend(["-p".to_string(), port.to_string()]);
    }
    command.push(format!("{user}@{host}"));
    SshPlanRecord {
        vm: name.to_string(),
        user,
        host,
        port,
        source,
        command,
    }
}

pub fn create_diagnostic_bundle(
    store: &VmStore,
    name: &str,
    output: PathBuf,
) -> Result<DiagnosticBundleMetadata, String> {
    let (source, _) = store.get_vm(name).map_err(|error| error.to_string())?;
    let created_at_unix = now_unix();
    let bundle_name = format!("bridgevm-diagnostics-{name}-{created_at_unix}");
    let destination = output.join(bundle_name);
    if destination.exists() {
        return Err(format!(
            "diagnostic bundle output already exists: {}",
            destination.display()
        ));
    }
    fs::create_dir_all(&destination)
        .map_err(|error| format!("failed to create diagnostic bundle: {error}"))?;

    let token = store
        .guest_tools_token(name)
        .map(|metadata| metadata.token)
        .ok();
    let mut files = Vec::new();
    copy_diagnostic_file(
        &source.join("manifest.yaml"),
        &destination.join("manifest.yaml"),
        &destination,
        token.as_deref(),
        &mut files,
    )?;
    copy_diagnostic_dir(
        &source.join("metadata"),
        &destination.join("metadata"),
        &destination,
        token.as_deref(),
        &mut files,
    )?;
    copy_diagnostic_dir(
        &source.join("logs"),
        &destination.join("logs"),
        &destination,
        token.as_deref(),
        &mut files,
    )?;

    let mut metadata = DiagnosticBundleMetadata {
        vm: name.to_string(),
        source,
        output: destination.clone(),
        files,
        created_at_unix,
    };
    let metadata_path = destination.join("diagnostic-bundle.json");
    fs::write(
        &metadata_path,
        serde_json::to_string_pretty(&metadata).map_err(|error| error.to_string())?,
    )
    .map_err(|error| format!("failed to write diagnostic bundle metadata: {error}"))?;
    metadata.files.push(PathBuf::from("diagnostic-bundle.json"));
    fs::write(
        &metadata_path,
        serde_json::to_string_pretty(&metadata).map_err(|error| error.to_string())?,
    )
    .map_err(|error| format!("failed to write diagnostic bundle metadata: {error}"))?;

    Ok(metadata)
}

pub fn view_vm_log(
    store: &VmStore,
    name: &str,
    kind: VmLogKind,
    max_bytes: Option<u64>,
) -> Result<VmLogViewRecord, String> {
    let (bundle, _) = store.get_vm(name).map_err(|error| error.to_string())?;
    let path = bundle.join("logs").join(kind.file_name());
    if !path.exists() {
        return Ok(VmLogViewRecord {
            vm: name.to_string(),
            kind,
            path,
            exists: false,
            bytes: 0,
            returned_bytes: 0,
            truncated: false,
            content: String::new(),
        });
    }
    let bytes_to_read = max_bytes
        .unwrap_or(DEFAULT_LOG_VIEW_BYTES)
        .clamp(1, MAX_LOG_VIEW_BYTES);
    let mut file =
        fs::File::open(&path).map_err(|error| format!("failed to open log file: {error}"))?;
    let bytes = file
        .metadata()
        .map_err(|error| format!("failed to inspect log file: {error}"))?
        .len();
    let start = bytes.saturating_sub(bytes_to_read);
    file.seek(SeekFrom::Start(start))
        .map_err(|error| format!("failed to seek log file: {error}"))?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)
        .map_err(|error| format!("failed to read log file: {error}"))?;
    let returned_bytes = buffer.len() as u64;
    Ok(VmLogViewRecord {
        vm: name.to_string(),
        kind,
        path,
        exists: true,
        bytes,
        returned_bytes,
        truncated: start > 0,
        content: String::from_utf8_lossy(&buffer).to_string(),
    })
}

pub fn create_performance_baseline(
    store: &VmStore,
    name: &str,
    output: PathBuf,
) -> Result<PerformanceBaselineMetadata, String> {
    let (source, _) = store.get_vm(name).map_err(|error| error.to_string())?;
    let created_at_unix = now_unix();
    let baseline_name = format!("bridgevm-performance-{name}-{created_at_unix}");
    let destination = output.join(baseline_name);
    if destination.exists() {
        return Err(format!(
            "performance baseline output already exists: {}",
            destination.display()
        ));
    }
    fs::create_dir_all(&destination)
        .map_err(|error| format!("failed to create performance baseline: {error}"))?;

    let state = store.state(name).map_err(|error| error.to_string())?;
    let runner = store
        .runner_metadata(name)
        .map_err(|error| error.to_string())?;
    let guest_tools = inspect_guest_tools_status(store, name)?;
    let metrics = guest_tools
        .runtime
        .as_ref()
        .and_then(|runtime| runtime.metrics.clone());
    let measurements = performance_measurements(created_at_unix, &state, runner.as_ref(), &metrics);
    let artifact = destination.join("performance-baseline.json");
    let baseline = PerformanceBaselineMetadata {
        vm: name.to_string(),
        source,
        output: destination,
        artifact: artifact.clone(),
        created_at_unix,
        metadata_only: true,
        state,
        runner,
        guest_tools,
        metrics,
        measurements,
        notes: vec![
            "metadata-only baseline; no active benchmark workloads were executed".to_string(),
            "captures existing VM state, runner metadata, and guest-tools runtime metrics"
                .to_string(),
        ],
    };
    fs::write(
        &artifact,
        serde_json::to_string_pretty(&baseline).map_err(|error| error.to_string())?,
    )
    .map_err(|error| format!("failed to write performance baseline metadata: {error}"))?;

    Ok(baseline)
}

pub fn create_performance_sample(
    store: &VmStore,
    name: &str,
    output: PathBuf,
    artifact_bytes: Option<u64>,
    iterations: Option<u16>,
    sync: bool,
) -> Result<PerformanceSampleMetadata, String> {
    let generation_started = Instant::now();
    let (source, manifest) = store.get_vm(name).map_err(|error| error.to_string())?;
    let (bytes, iterations, total_bytes) =
        validate_performance_sample_request(artifact_bytes, iterations)?;
    let created_at_unix = now_unix();
    let sample_name = format!("bridgevm-performance-sample-{name}-{created_at_unix}");
    let destination = output.join(sample_name);
    if destination.exists() {
        return Err(format!(
            "performance sample output already exists: {}",
            destination.display()
        ));
    }
    fs::create_dir_all(&destination)
        .map_err(|error| format!("failed to create performance sample: {error}"))?;

    let state_read_started = Instant::now();
    let state = store.state(name).map_err(|error| error.to_string())?;
    let state_read_latency = state_read_started.elapsed();
    let runner_read_started = Instant::now();
    let runner = store
        .runner_metadata(name)
        .map_err(|error| error.to_string())?;
    let runner_read_latency = runner_read_started.elapsed();
    let guest_tools_started = Instant::now();
    let guest_tools = inspect_guest_tools_status(store, name)?;
    let guest_tools_latency = guest_tools_started.elapsed();
    let metrics = guest_tools
        .runtime
        .as_ref()
        .and_then(|runtime| runtime.metrics.clone());

    let probe_data = vec![0_u8; bytes as usize];
    let mut probes = Vec::new();
    let mut iteration_results = Vec::new();
    for iteration in 1..=iterations {
        let probe = if iterations == 1 {
            destination.join("write-probe.bin")
        } else {
            destination.join(format!("write-probe-{iteration:04}.bin"))
        };
        let latency = write_performance_probe(&probe, &probe_data, sync)?;
        probes.push(probe.clone());
        iteration_results.push(PerformanceSampleIterationRecord {
            iteration,
            probe,
            bytes,
            write_latency_microseconds: duration_micros_u64(latency),
            sync,
        });
    }
    let probe = probes
        .first()
        .cloned()
        .ok_or_else(|| "performance sample did not produce a probe".to_string())?;

    let mut measurements =
        performance_measurements(created_at_unix, &state, runner.as_ref(), &metrics);
    measurements.push(performance_measurement_with_metadata_flag(
        "bridgevm_state_read_latency_microseconds",
        duration_micros_u64(state_read_latency),
        "microseconds",
        "bridgevm.store.state",
        false,
    ));
    measurements.push(performance_measurement_with_metadata_flag(
        "bridgevm_runner_metadata_read_latency_microseconds",
        duration_micros_u64(runner_read_latency),
        "microseconds",
        "bridgevm.store.runner_metadata",
        false,
    ));
    measurements.push(performance_measurement_with_metadata_flag(
        "bridgevm_guest_tools_status_inspect_latency_microseconds",
        duration_micros_u64(guest_tools_latency),
        "microseconds",
        "bridgevm.api.inspect_guest_tools_status",
        false,
    ));
    let write_latencies: Vec<u64> = iteration_results
        .iter()
        .map(|result| result.write_latency_microseconds)
        .collect();
    measurements.push(performance_measurement_with_metadata_flag(
        "host_artifact_write_latency_microseconds",
        mean_u64(&write_latencies),
        "microseconds",
        "host.fs.write_probe",
        false,
    ));
    measurements.push(performance_measurement_with_metadata_flag(
        "host_artifact_write_bytes",
        bytes,
        "bytes",
        "host.fs.write_probe",
        false,
    ));
    measurements.push(performance_measurement_with_metadata_flag(
        "host_artifact_write_iterations",
        u64::from(iterations),
        "count",
        "host.fs.write_probe",
        false,
    ));
    measurements.push(performance_measurement_with_metadata_flag(
        "host_artifact_write_total_bytes",
        total_bytes,
        "bytes",
        "host.fs.write_probe",
        false,
    ));
    measurements.push(performance_measurement_with_metadata_flag(
        "host_artifact_write_latency_min_microseconds",
        *write_latencies.iter().min().unwrap_or(&0),
        "microseconds",
        "host.fs.write_probe",
        false,
    ));
    measurements.push(performance_measurement_with_metadata_flag(
        "host_artifact_write_latency_max_microseconds",
        *write_latencies.iter().max().unwrap_or(&0),
        "microseconds",
        "host.fs.write_probe",
        false,
    ));
    measurements.push(performance_measurement_with_metadata_flag(
        "host_artifact_write_latency_mean_microseconds",
        mean_u64(&write_latencies),
        "microseconds",
        "host.fs.write_probe",
        false,
    ));
    measurements.push(performance_measurement_with_metadata_flag(
        "host_artifact_write_latency_p50_microseconds",
        percentile_u64(write_latencies.clone(), 50),
        "microseconds",
        "host.fs.write_probe",
        false,
    ));
    let mut notes = vec![
        "host-side sample; no guest benchmark workloads were executed".to_string(),
        "write latency is measured for the probe file left in this artifact directory".to_string(),
    ];
    match inspect_sample_primary_disk(store, name, &source, &manifest) {
        Ok(Some(disk)) => {
            measurements.push(performance_measurement_with_metadata_flag(
                "disk_inspect_duration_microseconds",
                disk.inspect_duration_microseconds,
                "microseconds",
                "host.qemu-img.info",
                false,
            ));
            notes.push(
                "disk inspect duration measures host qemu-img info execution, not guest disk I/O"
                    .to_string(),
            );
        }
        Ok(None) => notes.push(
            "disk inspect duration skipped because no existing non-raw primary disk was available"
                .to_string(),
        ),
        Err(error) => notes.push(format!("disk inspect duration skipped: {error}")),
    }
    measurements.push(performance_measurement_with_metadata_flag(
        "sample_generation_duration_microseconds",
        duration_micros_u64(generation_started.elapsed()),
        "microseconds",
        "bridgevm.performance.sample",
        false,
    ));

    let artifact = destination.join("performance-sample.json");
    let sample = PerformanceSampleMetadata {
        vm: name.to_string(),
        source,
        output: destination,
        artifact: artifact.clone(),
        probe,
        probes,
        artifact_bytes: bytes,
        iterations,
        sync,
        iteration_results,
        created_at_unix,
        state,
        runner,
        guest_tools,
        metrics,
        measurements,
        notes,
    };
    fs::write(
        &artifact,
        serde_json::to_string_pretty(&sample).map_err(|error| error.to_string())?,
    )
    .map_err(|error| format!("failed to write performance sample metadata: {error}"))?;

    Ok(sample)
}

fn validate_performance_sample_request(
    artifact_bytes: Option<u64>,
    iterations: Option<u16>,
) -> Result<(u64, u16, u64), String> {
    let bytes = artifact_bytes.unwrap_or(DEFAULT_PERFORMANCE_SAMPLE_ARTIFACT_BYTES);
    if bytes > MAX_PERFORMANCE_SAMPLE_ARTIFACT_BYTES {
        return Err(format!(
            "performance sample artifact is too large: {bytes} bytes (max {MAX_PERFORMANCE_SAMPLE_ARTIFACT_BYTES})"
        ));
    }
    let iterations = iterations.unwrap_or(DEFAULT_PERFORMANCE_SAMPLE_ITERATIONS);
    if iterations == 0 {
        return Err("performance sample iterations must be greater than zero".to_string());
    }
    if iterations > MAX_PERFORMANCE_SAMPLE_ITERATIONS {
        return Err(format!(
            "performance sample iterations is too large: {iterations} (max {MAX_PERFORMANCE_SAMPLE_ITERATIONS})"
        ));
    }
    let total_bytes = bytes
        .checked_mul(u64::from(iterations))
        .ok_or_else(|| "performance sample total bytes overflowed".to_string())?;
    if total_bytes > MAX_PERFORMANCE_SAMPLE_TOTAL_BYTES {
        return Err(format!(
            "performance sample total artifact bytes is too large: {total_bytes} bytes (max {MAX_PERFORMANCE_SAMPLE_TOTAL_BYTES})"
        ));
    }
    Ok((bytes, iterations, total_bytes))
}

fn inspect_sample_primary_disk(
    store: &VmStore,
    name: &str,
    bundle: &Path,
    manifest: &VmManifest,
) -> Result<Option<DiskInspectMetadata>, String> {
    if manifest.storage.primary.format == "raw" {
        return Ok(None);
    }
    let path = resolve_bundle_path(bundle, &manifest.storage.primary.path);
    if !path.exists() {
        return Ok(None);
    }
    store
        .inspect_primary_disk(name)
        .map(Some)
        .map_err(|error| error.to_string())
}

fn resolve_bundle_path(bundle: &Path, relative_or_absolute: &str) -> PathBuf {
    let path = PathBuf::from(relative_or_absolute);
    if path.is_absolute() {
        path
    } else {
        bundle.join(path)
    }
}

fn duration_micros_u64(duration: std::time::Duration) -> u64 {
    duration.as_micros().min(u128::from(u64::MAX)) as u64
}

fn write_performance_probe(
    probe: &Path,
    probe_data: &[u8],
    sync: bool,
) -> Result<std::time::Duration, String> {
    let write_started = Instant::now();
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(probe)
        .map_err(|error| format!("failed to open performance sample probe: {error}"))?;
    file.write_all(probe_data)
        .map_err(|error| format!("failed to write performance sample probe: {error}"))?;
    if sync {
        file.sync_data()
            .map_err(|error| format!("failed to sync performance sample probe: {error}"))?;
    }
    Ok(write_started.elapsed())
}

fn mean_u64(values: &[u64]) -> u64 {
    if values.is_empty() {
        return 0;
    }
    values.iter().sum::<u64>() / values.len() as u64
}

fn percentile_u64(mut values: Vec<u64>, percentile: u8) -> u64 {
    if values.is_empty() {
        return 0;
    }
    values.sort_unstable();
    let index = ((values.len() - 1) * usize::from(percentile)) / 100;
    values[index]
}

fn performance_measurements(
    created_at_unix: u64,
    state: &VmRuntimeMetadata,
    runner: Option<&RunnerMetadata>,
    metrics: &Option<GuestToolsMetricsMetadata>,
) -> Vec<PerformanceMeasurementRecord> {
    let mut measurements = Vec::new();
    if let Some(value) = created_at_unix.checked_sub(state.updated_at_unix) {
        measurements.push(performance_measurement(
            "state_metadata_age_seconds",
            value,
            "seconds",
            "state.updated_at_unix",
        ));
    }
    if let Some(runner) = runner {
        if let Some(value) = created_at_unix.checked_sub(runner.started_at_unix) {
            measurements.push(performance_measurement(
                "runner_observed_uptime_seconds",
                value,
                "seconds",
                "runner.started_at_unix",
            ));
        }
    }
    if let Some(metrics) = metrics {
        measurements.push(performance_measurement(
            "guest_cpu_percent",
            u64::from(metrics.cpu_percent),
            "percent",
            "guest_tools.metrics.cpu_percent",
        ));
        measurements.push(performance_measurement(
            "guest_memory_used_mib",
            metrics.memory_used_mib,
            "MiB",
            "guest_tools.metrics.memory_used_mib",
        ));
        if let Some(value) = created_at_unix.checked_sub(metrics.updated_at_unix) {
            measurements.push(performance_measurement(
                "guest_metrics_age_seconds",
                value,
                "seconds",
                "guest_tools.metrics.updated_at_unix",
            ));
        }
    }
    measurements
}

fn performance_measurement(
    name: &str,
    value: u64,
    unit: &str,
    source: &str,
) -> PerformanceMeasurementRecord {
    performance_measurement_with_metadata_flag(name, value, unit, source, true)
}

fn performance_measurement_with_metadata_flag(
    name: &str,
    value: u64,
    unit: &str,
    source: &str,
    metadata_only: bool,
) -> PerformanceMeasurementRecord {
    PerformanceMeasurementRecord {
        name: name.to_string(),
        value,
        unit: unit.to_string(),
        source: source.to_string(),
        metadata_only,
    }
}

fn copy_diagnostic_dir(
    source: &Path,
    destination: &Path,
    bundle_root: &Path,
    token: Option<&str>,
    files: &mut Vec<PathBuf>,
) -> Result<(), String> {
    if !source.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(source).map_err(|error| {
        format!(
            "failed to read diagnostic directory {}: {error}",
            source.display()
        )
    })? {
        let entry = entry.map_err(|error| error.to_string())?;
        let source_path = entry.path();
        if should_skip_diagnostic_path(&source_path) {
            continue;
        }
        let destination_path = destination.join(entry.file_name());
        let file_type = entry.file_type().map_err(|error| error.to_string())?;
        if file_type.is_dir() {
            copy_diagnostic_dir(&source_path, &destination_path, bundle_root, token, files)?;
        } else if file_type.is_file() {
            copy_diagnostic_file(&source_path, &destination_path, bundle_root, token, files)?;
        }
    }
    Ok(())
}

fn copy_diagnostic_file(
    source: &Path,
    destination: &Path,
    bundle_root: &Path,
    token: Option<&str>,
    files: &mut Vec<PathBuf>,
) -> Result<(), String> {
    if !source.exists() {
        return Ok(());
    }
    if should_skip_diagnostic_path(source) {
        return Ok(());
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create diagnostic directory {}: {error}",
                parent.display()
            )
        })?;
    }
    let bytes = fs::read(source).map_err(|error| {
        format!(
            "failed to read diagnostic file {}: {error}",
            source.display()
        )
    })?;
    let content = redact_diagnostic_text(&String::from_utf8_lossy(&bytes), token);
    fs::write(destination, content.as_bytes()).map_err(|error| {
        format!(
            "failed to write diagnostic file {}: {error}",
            destination.display()
        )
    })?;
    let relative = destination
        .strip_prefix(bundle_root)
        .map_err(|error| error.to_string())?
        .to_path_buf();
    files.push(relative);
    Ok(())
}

fn should_skip_diagnostic_path(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with(".sock") || name.ends_with(".lock"))
}

fn redact_diagnostic_text(content: &str, token: Option<&str>) -> String {
    let mut redacted = redact_sensitive_json_keys(content).unwrap_or_else(|| content.to_string());
    if let Some(token) = token.filter(|token| !token.is_empty()) {
        redacted = redacted.replace(token, "<redacted>");
    }
    redacted
}

fn redact_sensitive_json_keys(content: &str) -> Option<String> {
    let mut value: serde_json::Value = serde_json::from_str(content).ok()?;
    redact_sensitive_json_value(&mut value);
    serde_json::to_string_pretty(&value).ok()
}

fn redact_sensitive_json_value(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, value) in map.iter_mut() {
                if is_sensitive_diagnostic_key(key) {
                    *value = serde_json::Value::String("<redacted>".to_string());
                } else {
                    redact_sensitive_json_value(value);
                }
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                redact_sensitive_json_value(item);
            }
        }
        serde_json::Value::String(text) => {
            if let Some(redacted) = redact_url_query(text) {
                *text = redacted;
            }
        }
        _ => {}
    }
}

fn redact_url_query(value: &str) -> Option<String> {
    if !(value.starts_with("http://") || value.starts_with("https://")) {
        return None;
    }
    let (before_fragment, fragment) = value
        .split_once('#')
        .map_or((value, ""), |(before_fragment, fragment)| {
            (before_fragment, fragment)
        });
    let (base, _) = before_fragment.split_once('?')?;
    let mut redacted = format!("{base}?<redacted>");
    if !fragment.is_empty() {
        redacted.push('#');
        redacted.push_str(fragment);
    }
    Some(redacted)
}

fn is_sensitive_diagnostic_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    ["token", "password", "secret", "authorization", "credential"]
        .iter()
        .any(|sensitive| key.contains(sensitive))
}

pub fn import_boot_media(
    store: &VmStore,
    name: &str,
    source: PathBuf,
    kind: Option<BootMediaKind>,
) -> Result<BootMediaImportMetadata, String> {
    let source_metadata = fs::metadata(&source)
        .map_err(|error| format!("failed to read source media {}: {error}", source.display()))?;
    if !source_metadata.is_file() {
        return Err(format!("source media is not a file: {}", source.display()));
    }

    let (bundle, manifest, _) = store
        .get_vm_with_active_disk(name)
        .map_err(|error| error.to_string())?;
    let plan = build_fast_plan(&manifest, &bundle).map_err(|error| error.to_string())?;
    let (kind, destination) = boot_media_destination(&plan.launch_spec().boot, kind)?;
    ensure_boot_media_write_destination_in_bundle(&bundle, &destination, kind)?;
    let imported_at_unix = now_unix();
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create boot media directory {}: {error}",
                parent.display()
            )
        })?;
    }
    let replaced = destination.exists();
    if source == destination {
        let metadata = BootMediaImportMetadata {
            vm: name.to_string(),
            kind,
            source,
            destination,
            bytes: source_metadata.len(),
            replaced,
            imported_at_unix,
        };
        write_boot_media_import_metadata(&bundle, &metadata)?;
        return Ok(metadata);
    }
    let bytes = fs::copy(&source, &destination).map_err(|error| {
        format!(
            "failed to copy boot media from {} to {}: {error}",
            source.display(),
            destination.display()
        )
    })?;
    let metadata = BootMediaImportMetadata {
        vm: name.to_string(),
        kind,
        source,
        destination,
        bytes,
        replaced,
        imported_at_unix,
    };
    write_boot_media_import_metadata(&bundle, &metadata)?;
    Ok(metadata)
}

pub fn inspect_boot_media_status(store: &VmStore, name: &str) -> Result<BootMediaStatus, String> {
    let (bundle, manifest, _) = store
        .get_vm_with_active_disk(name)
        .map_err(|error| error.to_string())?;
    let plan = build_fast_plan(&manifest, &bundle).map_err(|error| error.to_string())?;
    let mut entries = Vec::new();
    push_boot_media_status_entry(
        &mut entries,
        &bundle,
        BootMediaKind::InstallerImage,
        plan.launch_spec().boot.installer_image.as_ref(),
    )?;
    push_boot_media_status_entry(
        &mut entries,
        &bundle,
        BootMediaKind::Kernel,
        plan.launch_spec().boot.kernel.as_ref(),
    )?;
    push_boot_media_status_entry(
        &mut entries,
        &bundle,
        BootMediaKind::Initrd,
        plan.launch_spec().boot.initrd.as_ref(),
    )?;
    push_boot_media_status_entry(
        &mut entries,
        &bundle,
        BootMediaKind::MacosRestoreImage,
        plan.launch_spec().boot.macos_restore_image.as_ref(),
    )?;
    Ok(BootMediaStatus {
        vm: name.to_string(),
        entries,
    })
}

pub fn verify_boot_media(
    store: &VmStore,
    name: &str,
    expected_sha256: &str,
    kind: Option<BootMediaKind>,
) -> Result<BootMediaVerificationMetadata, String> {
    let expected_sha256 = normalize_sha256(expected_sha256)?;
    let (bundle, manifest, _) = store
        .get_vm_with_active_disk(name)
        .map_err(|error| error.to_string())?;
    let plan = build_fast_plan(&manifest, &bundle).map_err(|error| error.to_string())?;
    let (kind, path) = boot_media_destination(&plan.launch_spec().boot, kind)?;
    let file_metadata = fs::metadata(&path)
        .map_err(|error| format!("failed to read boot media {}: {error}", path.display()))?;
    if !file_metadata.is_file() {
        return Err(format!("boot media is not a file: {}", path.display()));
    }
    let actual_sha256 = sha256_file(&path)?;
    let verified = actual_sha256 == expected_sha256;
    let verification = BootMediaVerificationMetadata {
        vm: name.to_string(),
        kind,
        path,
        bytes: file_metadata.len(),
        expected_sha256,
        actual_sha256,
        verified,
        verified_at_unix: now_unix(),
    };
    write_boot_media_verification_metadata(&bundle, &verification)?;
    if !verification.verified {
        return Err(format!(
            "boot media SHA-256 mismatch for {}: expected {}, got {}",
            verification.path.display(),
            verification.expected_sha256,
            verification.actual_sha256
        ));
    }
    Ok(verification)
}

pub fn plan_boot_media_download(
    store: &VmStore,
    name: &str,
    url: &str,
    expected_sha256: Option<&str>,
    kind: Option<BootMediaKind>,
) -> Result<BootMediaDownloadPlanMetadata, String> {
    let url = normalize_download_url(url)?;
    let expected_sha256 = expected_sha256.map(normalize_sha256).transpose()?;
    let (bundle, manifest, _) = store
        .get_vm_with_active_disk(name)
        .map_err(|error| error.to_string())?;
    let plan = build_fast_plan(&manifest, &bundle).map_err(|error| error.to_string())?;
    let (kind, destination) = boot_media_destination(&plan.launch_spec().boot, kind)?;
    ensure_boot_media_write_destination_in_bundle(&bundle, &destination, kind)?;
    let file_metadata = fs::metadata(&destination).ok();
    let exists = file_metadata
        .as_ref()
        .is_some_and(std::fs::Metadata::is_file);
    let bytes = file_metadata
        .filter(std::fs::Metadata::is_file)
        .map(|metadata| metadata.len());
    let metadata = BootMediaDownloadPlanMetadata {
        vm: name.to_string(),
        kind,
        url,
        destination,
        exists,
        bytes,
        expected_sha256,
        last_import: read_boot_media_import_metadata(&bundle, kind)?,
        last_verification: read_boot_media_verification_metadata(&bundle, kind)?,
        planned_at_unix: now_unix(),
    };
    write_boot_media_download_plan_metadata(&bundle, &metadata)?;
    Ok(metadata)
}

pub fn download_boot_media(
    store: &VmStore,
    name: &str,
    kind: Option<BootMediaKind>,
) -> Result<BootMediaDownloadResultMetadata, String> {
    let (bundle, manifest, _) = store
        .get_vm_with_active_disk(name)
        .map_err(|error| error.to_string())?;
    let plan = build_fast_plan(&manifest, &bundle).map_err(|error| error.to_string())?;
    let (kind, destination) = boot_media_destination(&plan.launch_spec().boot, kind)?;
    ensure_boot_media_write_destination_in_bundle(&bundle, &destination, kind)?;
    let download_plan = read_boot_media_download_plan_metadata(&bundle, kind)?
        .ok_or_else(|| format!("no download plan recorded for boot media kind {kind}"))?;
    if download_plan.destination != destination {
        return Err(format!(
            "download plan destination {} does not match current resolved destination {}",
            download_plan.destination.display(),
            destination.display()
        ));
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create boot media directory {}: {error}",
                parent.display()
            )
        })?;
    }
    let temp_path = boot_media_download_temp_path(&destination);
    if temp_path.exists() {
        fs::remove_file(&temp_path).map_err(|error| {
            format!(
                "failed to remove stale download temp file {}: {error}",
                temp_path.display()
            )
        })?;
    }
    let command = vec![
        "curl".to_string(),
        "--location".to_string(),
        "--fail".to_string(),
        "--silent".to_string(),
        "--show-error".to_string(),
        "--output".to_string(),
        temp_path.display().to_string(),
        download_plan.url.clone(),
    ];
    let output = Command::new("curl")
        .args([
            "--location",
            "--fail",
            "--silent",
            "--show-error",
            "--output",
        ])
        .arg(&temp_path)
        .arg(&download_plan.url)
        .output()
        .map_err(|error| format!("failed to execute curl: {error}"))?;
    let replaced = destination.exists();
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if !output.status.success() {
        let metadata = BootMediaDownloadResultMetadata {
            vm: name.to_string(),
            kind,
            url: download_plan.url,
            destination,
            temp_path,
            command,
            exit_status: output.status.code(),
            stdout,
            stderr,
            bytes: None,
            replaced,
            expected_sha256: download_plan.expected_sha256,
            actual_sha256: None,
            verified: None,
            downloaded: false,
            downloaded_at_unix: now_unix(),
        };
        write_boot_media_download_result_metadata(&bundle, &metadata)?;
        return Err(format!(
            "boot media download failed with status {}",
            metadata
                .exit_status
                .map_or("unknown".to_string(), |status| status.to_string())
        ));
    }

    let actual_sha256 = sha256_file(&temp_path)?;
    let verified = download_plan
        .expected_sha256
        .as_ref()
        .map(|expected| expected == &actual_sha256);
    if verified == Some(false) {
        let bytes = fs::metadata(&temp_path).ok().map(|metadata| metadata.len());
        let metadata = BootMediaDownloadResultMetadata {
            vm: name.to_string(),
            kind,
            url: download_plan.url,
            destination,
            temp_path,
            command,
            exit_status: output.status.code(),
            stdout,
            stderr,
            bytes,
            replaced,
            expected_sha256: download_plan.expected_sha256,
            actual_sha256: Some(actual_sha256),
            verified,
            downloaded: false,
            downloaded_at_unix: now_unix(),
        };
        write_boot_media_download_result_metadata(&bundle, &metadata)?;
        return Err(format!(
            "downloaded boot media SHA-256 mismatch for {}",
            metadata.destination.display()
        ));
    }

    fs::rename(&temp_path, &destination).map_err(|error| {
        format!(
            "failed to move downloaded boot media from {} to {}: {error}",
            temp_path.display(),
            destination.display()
        )
    })?;
    let bytes = fs::metadata(&destination)
        .ok()
        .map(|metadata| metadata.len());
    let metadata = BootMediaDownloadResultMetadata {
        vm: name.to_string(),
        kind,
        url: download_plan.url,
        destination,
        temp_path,
        command,
        exit_status: output.status.code(),
        stdout,
        stderr,
        bytes,
        replaced,
        expected_sha256: download_plan.expected_sha256,
        actual_sha256: Some(actual_sha256),
        verified,
        downloaded: true,
        downloaded_at_unix: now_unix(),
    };
    write_boot_media_download_result_metadata(&bundle, &metadata)?;
    Ok(metadata)
}

fn boot_media_destination(
    boot: &AppleVzBootSpec,
    requested: Option<BootMediaKind>,
) -> Result<(BootMediaKind, PathBuf), String> {
    let mut candidates = Vec::new();
    push_boot_media_candidate(
        &mut candidates,
        BootMediaKind::InstallerImage,
        boot.installer_image.as_ref(),
    );
    push_boot_media_candidate(&mut candidates, BootMediaKind::Kernel, boot.kernel.as_ref());
    push_boot_media_candidate(&mut candidates, BootMediaKind::Initrd, boot.initrd.as_ref());
    push_boot_media_candidate(
        &mut candidates,
        BootMediaKind::MacosRestoreImage,
        boot.macos_restore_image.as_ref(),
    );

    if let Some(requested) = requested {
        return candidates
            .into_iter()
            .find(|(kind, _)| *kind == requested)
            .ok_or_else(|| format!("boot media kind {requested} is not present in this VM plan"));
    }

    match candidates.len() {
        0 => Err("no importable boot media path is present in this VM plan".to_string()),
        1 => Ok(candidates.remove(0)),
        _ => Err(
            "multiple boot media paths are present; pass --kind to choose which one to import"
                .to_string(),
        ),
    }
}

fn push_boot_media_candidate(
    candidates: &mut Vec<(BootMediaKind, PathBuf)>,
    kind: BootMediaKind,
    path: Option<&AppleVzPathSpec>,
) {
    if let Some(path) = path {
        candidates.push((kind, PathBuf::from(&path.path)));
    }
}

fn ensure_boot_media_write_destination_in_bundle(
    bundle: &std::path::Path,
    destination: &std::path::Path,
    kind: BootMediaKind,
) -> Result<(), String> {
    let bundle = normalize_absolute_path(bundle);
    let destination = normalize_absolute_path(destination);
    if destination.starts_with(&bundle) {
        Ok(())
    } else {
        Err(format!(
            "boot media {kind} destination {} is outside VM bundle {}",
            destination.display(),
            bundle.display()
        ))
    }
}

fn normalize_absolute_path(path: &std::path::Path) -> PathBuf {
    use std::path::Component;

    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Normal(part) => normalized.push(part),
        }
    }
    normalized
}

fn push_boot_media_status_entry(
    entries: &mut Vec<BootMediaStatusEntry>,
    bundle: &std::path::Path,
    kind: BootMediaKind,
    path: Option<&AppleVzPathSpec>,
) -> Result<(), String> {
    let Some(path) = path else {
        return Ok(());
    };
    let path = PathBuf::from(&path.path);
    let file_metadata = fs::metadata(&path).ok();
    let exists = file_metadata
        .as_ref()
        .is_some_and(std::fs::Metadata::is_file);
    let bytes = file_metadata
        .filter(std::fs::Metadata::is_file)
        .map(|metadata| metadata.len());
    entries.push(BootMediaStatusEntry {
        kind,
        path,
        exists,
        bytes,
        last_import: read_boot_media_import_metadata(bundle, kind)?,
        last_verification: read_boot_media_verification_metadata(bundle, kind)?,
        last_download_plan: read_boot_media_download_plan_metadata(bundle, kind)?,
        last_download: read_boot_media_download_result_metadata(bundle, kind)?,
    });
    Ok(())
}

fn write_boot_media_import_metadata(
    bundle: &std::path::Path,
    metadata: &BootMediaImportMetadata,
) -> Result<(), String> {
    let path = boot_media_import_metadata_path(bundle, metadata.kind);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create boot media metadata directory {}: {error}",
                parent.display()
            )
        })?;
    }
    fs::write(
        &path,
        serde_json::to_string_pretty(metadata).map_err(|error| error.to_string())?,
    )
    .map_err(|error| {
        format!(
            "failed to write boot media metadata {}: {error}",
            path.display()
        )
    })
}

fn read_boot_media_import_metadata(
    bundle: &std::path::Path,
    kind: BootMediaKind,
) -> Result<Option<BootMediaImportMetadata>, String> {
    let path = boot_media_import_metadata_path(bundle, kind);
    if !path.exists() {
        return Ok(None);
    }
    read_boot_media_metadata_json(&path, "boot media metadata").map(Some)
}

fn boot_media_import_metadata_path(bundle: &std::path::Path, kind: BootMediaKind) -> PathBuf {
    bundle
        .join("metadata")
        .join("boot-media")
        .join(format!("{kind}.json"))
}

fn write_boot_media_verification_metadata(
    bundle: &std::path::Path,
    metadata: &BootMediaVerificationMetadata,
) -> Result<(), String> {
    let path = boot_media_verification_metadata_path(bundle, metadata.kind);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create boot media metadata directory {}: {error}",
                parent.display()
            )
        })?;
    }
    fs::write(
        &path,
        serde_json::to_string_pretty(metadata).map_err(|error| error.to_string())?,
    )
    .map_err(|error| {
        format!(
            "failed to write boot media verification metadata {}: {error}",
            path.display()
        )
    })
}

fn read_boot_media_verification_metadata(
    bundle: &std::path::Path,
    kind: BootMediaKind,
) -> Result<Option<BootMediaVerificationMetadata>, String> {
    let path = boot_media_verification_metadata_path(bundle, kind);
    if !path.exists() {
        return Ok(None);
    }
    read_boot_media_metadata_json(&path, "boot media verification metadata").map(Some)
}

fn boot_media_verification_metadata_path(bundle: &std::path::Path, kind: BootMediaKind) -> PathBuf {
    bundle
        .join("metadata")
        .join("boot-media")
        .join(format!("{kind}-verify.json"))
}

fn write_boot_media_download_plan_metadata(
    bundle: &std::path::Path,
    metadata: &BootMediaDownloadPlanMetadata,
) -> Result<(), String> {
    let path = boot_media_download_plan_metadata_path(bundle, metadata.kind);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create boot media metadata directory {}: {error}",
                parent.display()
            )
        })?;
    }
    fs::write(
        &path,
        serde_json::to_string_pretty(metadata).map_err(|error| error.to_string())?,
    )
    .map_err(|error| {
        format!(
            "failed to write boot media download plan metadata {}: {error}",
            path.display()
        )
    })
}

fn read_boot_media_download_plan_metadata(
    bundle: &std::path::Path,
    kind: BootMediaKind,
) -> Result<Option<BootMediaDownloadPlanMetadata>, String> {
    let path = boot_media_download_plan_metadata_path(bundle, kind);
    if !path.exists() {
        return Ok(None);
    }
    read_boot_media_metadata_json(&path, "boot media download plan metadata").map(Some)
}

fn boot_media_download_plan_metadata_path(
    bundle: &std::path::Path,
    kind: BootMediaKind,
) -> PathBuf {
    bundle
        .join("metadata")
        .join("boot-media")
        .join(format!("{kind}-download.json"))
}

fn write_boot_media_download_result_metadata(
    bundle: &std::path::Path,
    metadata: &BootMediaDownloadResultMetadata,
) -> Result<(), String> {
    let path = boot_media_download_result_metadata_path(bundle, metadata.kind);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create boot media metadata directory {}: {error}",
                parent.display()
            )
        })?;
    }
    fs::write(
        &path,
        serde_json::to_string_pretty(metadata).map_err(|error| error.to_string())?,
    )
    .map_err(|error| {
        format!(
            "failed to write boot media download result metadata {}: {error}",
            path.display()
        )
    })
}

fn read_boot_media_download_result_metadata(
    bundle: &std::path::Path,
    kind: BootMediaKind,
) -> Result<Option<BootMediaDownloadResultMetadata>, String> {
    let path = boot_media_download_result_metadata_path(bundle, kind);
    if !path.exists() {
        return Ok(None);
    }
    read_boot_media_metadata_json(&path, "boot media download result metadata").map(Some)
}

fn read_boot_media_metadata_json<T: DeserializeOwned>(
    path: &Path,
    label: &str,
) -> Result<T, String> {
    let mut bytes = Vec::new();
    fs::File::open(path)
        .and_then(|file| {
            file.take(MAX_BOOT_MEDIA_METADATA_BYTES + 1)
                .read_to_end(&mut bytes)
        })
        .map_err(|error| format!("failed to read {label} {}: {error}", path.display()))?;
    if bytes.len() as u64 > MAX_BOOT_MEDIA_METADATA_BYTES {
        return Err(format!(
            "{label} {} exceeds the {MAX_BOOT_MEDIA_METADATA_BYTES}-byte limit",
            path.display()
        ));
    }
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("invalid {label} {}: {error}", path.display()))
}

fn boot_media_download_result_metadata_path(
    bundle: &std::path::Path,
    kind: BootMediaKind,
) -> PathBuf {
    bundle
        .join("metadata")
        .join("boot-media")
        .join(format!("{kind}-download-result.json"))
}

fn boot_media_download_temp_path(destination: &std::path::Path) -> PathBuf {
    let file_name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("boot-media");
    destination.with_file_name(format!(".{file_name}.download"))
}

pub fn readiness_report(store: &VmStore, name: &str) -> Result<VmReadinessReport, String> {
    readiness_report_with_live_evidence(store, name, None)
}

pub fn readiness_report_with_live_evidence(
    store: &VmStore,
    name: &str,
    live_evidence_path: Option<&Path>,
) -> Result<VmReadinessReport, String> {
    readiness_report_with_live_evidence_and_record(store, name, live_evidence_path, false)
}

pub fn readiness_report_with_live_evidence_and_record(
    store: &VmStore,
    name: &str,
    live_evidence_path: Option<&Path>,
    record_live_evidence: bool,
) -> Result<VmReadinessReport, String> {
    readiness_report_with_live_evidence_options(
        store,
        name,
        live_evidence_path,
        record_live_evidence,
        false,
    )
}

pub fn readiness_report_with_live_evidence_options(
    store: &VmStore,
    name: &str,
    live_evidence_path: Option<&Path>,
    record_live_evidence: bool,
    clear_live_evidence: bool,
) -> Result<VmReadinessReport, String> {
    let (_, manifest) = store.get_vm(name).map_err(|error| error.to_string())?;
    let state = store.state(name).map_err(|error| error.to_string())?.state;

    let (boot_media, boot_media_error) = if manifest.mode == VmMode::Fast {
        match inspect_boot_media_status(store, name) {
            Ok(status) => (Some(status), None),
            Err(error) => (None, Some(error)),
        }
    } else {
        (None, None)
    };

    let (snapshot_chain, snapshot_chain_error) = match store.snapshot_chain(name) {
        Ok(chain) => (Some(chain), None),
        Err(error) => (None, Some(error.to_string())),
    };
    let (runner, runner_error) = match store.runner_metadata(name) {
        Ok(metadata) => (metadata, None),
        Err(error) => (None, Some(error.to_string())),
    };
    let qmp_supervisor = store
        .qmp_supervisor_metadata(name)
        .map_err(|error| error.to_string())?;
    let mut pre_run_launch_readiness = None;

    let mut blockers = Vec::new();
    let mut notes = vec![
        "metadata-only preflight report; no VM, QEMU, Apple VZ, console, or guest workload was started".to_string(),
        "live E2E boot, console, and guest-tools effects still require the explicit opt-in live smoke evidence path".to_string(),
    ];
    let mut live_evidence = None;
    if clear_live_evidence {
        if live_evidence_path.is_some() || record_live_evidence {
            blockers.push(
                "live-evidence-clear-error:--clear-live-evidence cannot be combined with --live-evidence or --record-live-evidence"
                    .to_string(),
            );
        } else {
            match store.clear_live_evidence_metadata(name) {
                Ok(()) => notes.push("cleared preserved live evidence metadata".to_string()),
                Err(error) => blockers.push(format!("live-evidence-clear-error:{error}")),
            }
        }
    } else if let Some(path) = live_evidence_path {
        let live_context = LiveEvidenceVerificationContext::from_readiness(
            name,
            manifest.mode,
            &store.bundle_path(name),
            snapshot_chain.as_ref(),
        );
        match verify_live_evidence_bundle_with_context(path, Some(&live_context)) {
            Ok(mut verification) => {
                if record_live_evidence {
                    match store.import_live_evidence_bundle(name, path) {
                        Ok(metadata) => {
                            notes.push(format!(
                                "recorded preserved live evidence bundle: {}",
                                metadata.preserved_path.display()
                            ));
                            verification = verify_live_evidence_bundle_with_context(
                                &metadata.preserved_path,
                                Some(&live_context),
                            )?;
                        }
                        Err(error) => blockers.push(format!("live-evidence-record-error:{error}")),
                    }
                }
                if !blockers
                    .iter()
                    .any(|blocker| blocker.starts_with("live-evidence-record-error:"))
                {
                    notes.push(format!(
                        "verified {} live evidence bundle: {}",
                        live_evidence_backend_label(&verification.backend),
                        verification.path.display()
                    ));
                    live_evidence = Some(verification);
                }
            }
            Err(error) => blockers.push(format!("live-evidence-invalid:{error}")),
        }
    } else if record_live_evidence {
        blockers.push(
            "live-evidence-record-error:--record-live-evidence requires --live-evidence"
                .to_string(),
        );
    } else {
        match store.live_evidence_metadata(name) {
            Ok(Some(metadata)) => {
                let live_context = LiveEvidenceVerificationContext::from_readiness(
                    name,
                    manifest.mode,
                    &store.bundle_path(name),
                    snapshot_chain.as_ref(),
                );
                match verify_live_evidence_bundle_with_context(
                    &metadata.preserved_path,
                    Some(&live_context),
                ) {
                    Ok(verification) => {
                        notes.push(format!(
                            "verified {} live evidence bundle: {}",
                            live_evidence_backend_label(&verification.backend),
                            verification.path.display()
                        ));
                        live_evidence = Some(verification);
                    }
                    Err(error) => blockers.push(format!("live-evidence-invalid:{error}")),
                }
            }
            Ok(None) => {}
            Err(error) => blockers.push(format!("live-evidence-metadata-error:{error}")),
        }
    }

    if let Some(error) = &boot_media_error {
        blockers.push(format!("boot-media-status-error:{error}"));
    }
    if let Some(status) = &boot_media {
        for entry in status.entries.iter().filter(|entry| !entry.exists) {
            blockers.push(format!(
                "boot-media-missing:{}:{}",
                entry.kind,
                entry.path.display()
            ));
        }
    }

    if let Some(error) = &snapshot_chain_error {
        blockers.push(format!("snapshot-chain-error:{error}"));
    }
    if let Some(chain) = &snapshot_chain {
        if !chain.active_disk.exists {
            blockers.push(format!(
                "active-disk-missing:{}",
                chain.active_disk.path.display()
            ));
        }
    }

    match &runner {
        Some(metadata) => {
            if let Some(readiness) = &metadata.launch_readiness {
                if !readiness.ready {
                    for blocker in &readiness.blockers {
                        blockers.push(format!("launch-readiness-blocker:{}", blocker.code));
                    }
                }
            } else {
                notes.push(
                    "runner metadata has no launch-readiness field for this backend".to_string(),
                );
            }
        }
        None => {
            if let Some(error) = &runner_error {
                blockers.push(format!("runner-metadata-error:{error}"));
            } else if manifest.mode == VmMode::Fast {
                match build_fast_plan(&manifest, &store.bundle_path(name)) {
                    Ok(plan) => {
                        let readiness = launch_readiness_metadata(&plan.launch_spec().readiness);
                        if readiness.ready {
                            notes.push("Fast Mode launch readiness was evaluated from the manifest and bundle without writing runner metadata".to_string());
                        } else {
                            for blocker in &readiness.blockers {
                                blockers.push(format!("launch-readiness-blocker:{}", blocker.code));
                            }
                        }
                        pre_run_launch_readiness = Some(readiness);
                    }
                    Err(error) => {
                        blockers.push(format!("launch-readiness-error:{error}"));
                    }
                }
            } else if manifest.mode == VmMode::Compatibility {
                if let Some(chain) = &snapshot_chain {
                    let disk = bridgevm_storage::DiskPreparationMetadata {
                        path: chain.active_disk.path.clone(),
                        format: chain.active_disk.format.clone(),
                        size: manifest.storage.primary.size.clone(),
                        size_bytes: None,
                        exists: chain.active_disk.exists,
                        created: false,
                        create_command: None,
                        prepared_at_unix: now_unix(),
                    };
                    let bundle = store.bundle_path(name);
                    let mut readiness_blockers =
                        compatibility_launch_dependency_blockers(&manifest, &bundle);
                    if let Some(blocker) = build_compatibility_command(&manifest, &bundle)
                        .err()
                        .map(compatibility_launch_readiness_blocker_from_qemu_error)
                    {
                        readiness_blockers.push(blocker);
                    }
                    let readiness =
                        compatibility_launch_readiness_metadata(&disk, readiness_blockers);
                    if readiness.ready {
                        notes.push("Compatibility Mode launch readiness was evaluated from the manifest and active disk without writing runner metadata".to_string());
                    } else {
                        for blocker in &readiness.blockers {
                            blockers.push(format!("launch-readiness-blocker:{}", blocker.code));
                        }
                    }
                    pre_run_launch_readiness = Some(readiness);
                } else if let Some(error) = &snapshot_chain_error {
                    blockers.push(format!("launch-readiness-error:{error}"));
                }
            } else {
                blockers.push("runner-metadata-missing".to_string());
            }
        }
    }

    if state == VmRuntimeState::Running {
        notes.push(
            "running VM should use QMP status and bounded log tails for console diagnostics"
                .to_string(),
        );
    }
    if manifest.mode == VmMode::Compatibility {
        notes.push("Compatibility Mode readiness is driven by disk, runner metadata, QMP, and logs rather than Fast boot media status".to_string());
    }

    Ok(VmReadinessReport {
        vm: name.to_string(),
        mode: manifest.mode,
        state,
        metadata_only: true,
        live_e2e_required: true,
        live_evidence: live_evidence.clone(),
        evidence_requirements: metadata_safe_live_evidence_requirements(live_evidence.as_ref()),
        boot_media,
        boot_media_error,
        snapshot_chain,
        snapshot_chain_error,
        runner,
        pre_run_launch_readiness,
        qmp_supervisor,
        runner_error,
        blockers,
        notes,
    })
}

fn metadata_safe_live_evidence_requirements(
    live_evidence: Option<&VmLiveEvidenceVerification>,
) -> Vec<VmEvidenceRequirement> {
    let live_boot_proven = live_evidence.is_some_and(live_boot_progress_proven);
    let console_proven = live_evidence.is_some_and(|evidence| {
        evidence.serial_sentinel_proven
            || evidence.viewer_evidence_proven
            || evidence.qmp_evidence_proven
    });
    let guest_tools_effects_proven =
        live_evidence.is_some_and(|evidence| evidence.guest_tools_effects_proven);
    vec![
        VmEvidenceRequirement {
            kind: "live-boot".to_string(),
            required: true,
            proven: live_boot_proven,
            note: if live_boot_proven {
                let evidence = live_evidence.expect("live boot proven requires evidence");
                let progress_label =
                    if evidence.serial_sentinel_proven && evidence.graphical_boot_progress_proven {
                        "serial and graphical boot progress"
                    } else if evidence.graphical_boot_progress_proven {
                        "graphical boot progress"
                    } else {
                        "serial boot progress"
                    };
                format!(
                    "verified preserved opt-in {} {progress_label} evidence bundle",
                    live_evidence_backend_label(&evidence.backend)
                )
            } else if let Some(evidence) = live_evidence {
                format!(
                    "verified preserved opt-in {} launch evidence; guest boot progress evidence is still required",
                    live_evidence_backend_label(&evidence.backend)
                )
            } else {
                "requires preserved opt-in serial or graphical boot progress evidence from Apple VZ or QEMU"
                    .to_string()
            },
        },
        VmEvidenceRequirement {
            kind: "console".to_string(),
            required: true,
            proven: console_proven,
            note: if console_proven {
                "verified serial, graphical viewer, or QMP evidence from the preserved live bundle"
                    .to_string()
            } else {
                "requires graphical console, QMP, or serial evidence from a live backend"
                    .to_string()
            },
        },
        VmEvidenceRequirement {
            kind: "guest-tools-effects".to_string(),
            required: true,
            proven: guest_tools_effects_proven,
            note: if guest_tools_effects_proven {
                "verified guest-tools command/effect evidence from the preserved live bundle"
                    .to_string()
            } else {
                "requires guest-tools command and effect evidence from a live guest".to_string()
            },
        },
    ]
}

fn live_boot_progress_proven(evidence: &VmLiveEvidenceVerification) -> bool {
    evidence.serial_sentinel_proven || evidence.graphical_boot_progress_proven
}

struct LiveEvidenceVerificationContext {
    vm_name: String,
    mode: VmMode,
    bundle_path: PathBuf,
    disk_format: Option<String>,
    network: String,
    qmp_socket: PathBuf,
}

impl LiveEvidenceVerificationContext {
    fn from_readiness(
        vm_name: &str,
        mode: VmMode,
        bundle: &Path,
        snapshot_chain: Option<&SnapshotChainMetadata>,
    ) -> Self {
        Self {
            vm_name: vm_name.to_string(),
            mode,
            bundle_path: bundle.to_path_buf(),
            disk_format: snapshot_chain.map(|chain| chain.active_disk.format.clone()),
            network: "nat".to_string(),
            qmp_socket: qmp_socket_path(bundle),
        }
    }
}

fn verify_live_evidence_bundle_with_context(
    path: &Path,
    context: Option<&LiveEvidenceVerificationContext>,
) -> Result<VmLiveEvidenceVerification, String> {
    if path.join("qemu-live-evidence.json").exists() {
        verify_qemu_live_evidence_bundle(path, context)
    } else {
        verify_apple_vz_live_evidence_bundle(path, context)
    }
}

fn live_evidence_backend_label(backend: &str) -> &'static str {
    match backend {
        "apple-virtualization-framework" => "Apple VZ",
        "qemu" => "QEMU",
        _ => "backend",
    }
}

fn verify_apple_vz_live_evidence_bundle(
    path: &Path,
    context: Option<&LiveEvidenceVerificationContext>,
) -> Result<VmLiveEvidenceVerification, String> {
    if !path.is_dir() {
        return Err(format!("evidence directory not found: {}", path.display()));
    }

    let summary = read_evidence_text(path, "SUMMARY.txt")?;
    let environment = read_evidence_text(path, "environment.txt")?;
    let validate_output = read_evidence_text(path, "apple-vz-validate.output")?;
    let launch_output = read_evidence_text(path, "apple-vz-live-launch.output")?;
    let missing_opt_in_stderr = read_evidence_text(path, "live-vz-missing-helper-opt-in.stderr")?;
    let missing_opt_in_stdout = read_evidence_text(path, "live-vz-missing-helper-opt-in.stdout")?;
    let runner_path = read_evidence_text(path, "apple-vz-runner.path")?;
    let runner_artifact = read_optional_evidence_text(path, "apple-vz-runner.artifact")?;
    let runner_sha = read_evidence_text(path, "apple-vz-runner.sha256")?;
    let manifest = read_evidence_json(path, "fixture-manifest.json")?;
    let launch = read_evidence_json(path, "apple-vz-launch.json")?;
    let handoff = read_evidence_json(path, "live-vz-handoff.json")?;

    require_contains(
        &summary,
        "Apple VZ live boot opt-in smoke: passed",
        "SUMMARY.txt",
    )?;
    require_contains(&summary, "Serial evidence:", "SUMMARY.txt")?;
    require_contains(
        &validate_output,
        "AppleVzRunner handoff ready",
        "apple-vz-validate.output",
    )?;
    require_contains(
        &validate_output,
        "VZ configuration validation: ready",
        "apple-vz-validate.output",
    )?;
    require_contains(
        &validate_output,
        "Boot loader: linux-kernel",
        "apple-vz-validate.output",
    )?;
    require_contains(
        &validate_output,
        "Disk attachment: disk-image-raw",
        "apple-vz-validate.output",
    )?;
    require_contains(
        &validate_output,
        "Network attachment: nat",
        "apple-vz-validate.output",
    )?;
    require_contains(
        &environment,
        "BRIDGEVM_LIVE_VZ_ALLOW_REAL_START=1",
        "environment.txt",
    )?;
    require_contains(
        &missing_opt_in_stderr,
        "real Apple VZ start requires --allow-real-vz-start",
        "live-vz-missing-helper-opt-in.stderr",
    )?;
    if !missing_opt_in_stdout.is_empty() {
        return Err("live-vz-missing-helper-opt-in.stdout should be empty".to_string());
    }
    let runner_path = runner_path.lines().next().unwrap_or("").trim();
    if runner_path.is_empty() {
        return Err("apple-vz-runner.path is empty".to_string());
    }
    let runner_check_path = if let Some(runner_artifact) = runner_artifact {
        let runner_artifact = runner_artifact.lines().next().unwrap_or("").trim();
        if runner_artifact.is_empty() {
            return Err("apple-vz-runner.artifact is empty".to_string());
        }
        let artifact_path = Path::new(runner_artifact);
        if artifact_path.is_absolute() || runner_artifact.contains("..") {
            return Err(format!(
                "apple-vz-runner.artifact must be a relative evidence path: {runner_artifact}"
            ));
        }
        path.join(artifact_path)
    } else {
        PathBuf::from(runner_path)
    };
    let runner_metadata = fs::symlink_metadata(&runner_check_path).map_err(|error| {
        format!(
            "failed to inspect AppleVzRunner evidence {}: {error}",
            runner_check_path.display()
        )
    })?;
    if runner_metadata.file_type().is_symlink() {
        return Err(format!(
            "AppleVzRunner evidence must not be a symlink: {}",
            runner_check_path.display()
        ));
    }
    if !runner_metadata.is_file() {
        return Err(format!(
            "AppleVzRunner evidence is not a file: {}",
            runner_check_path.display()
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if runner_metadata.permissions().mode() & 0o111 == 0 {
            return Err(format!(
                "AppleVzRunner evidence is not executable: {}",
                runner_check_path.display()
            ));
        }
    }
    let runner_sha = runner_sha.lines().next().unwrap_or("").trim();
    if !is_sha256_hex(runner_sha) {
        return Err("apple-vz-runner.sha256 is not lowercase SHA-256 hex".to_string());
    }
    let actual_runner_sha = sha256_file(&runner_check_path)?;
    if actual_runner_sha != runner_sha {
        return Err("AppleVzRunner SHA-256 does not match evidence artifact".to_string());
    }

    let vm_name = json_string(&launch, &["vm_name"])?;
    if vm_name.trim().is_empty() {
        return Err("launch vm_name is empty".to_string());
    }
    if let Some(context) = context {
        if context.mode != VmMode::Fast {
            return Err(format!(
                "Apple VZ live evidence cannot verify {} Mode VM {}",
                context.mode, context.vm_name
            ));
        }
        if vm_name != context.vm_name {
            return Err(format!(
                "Apple VZ launch vm_name {vm_name} does not match readiness VM {}",
                context.vm_name
            ));
        }
        let launch_bundle_path = json_string(&launch, &["bundle_path"])?;
        let expected_bundle_path = context.bundle_path.display().to_string();
        if launch_bundle_path != expected_bundle_path {
            return Err(format!(
                "Apple VZ launch bundle_path {launch_bundle_path} does not match readiness VM bundle {expected_bundle_path}"
            ));
        }
    }
    require_contains(
        &launch_output,
        "AppleVzRunner handoff ready",
        "apple-vz-live-launch.output",
    )?;
    require_contains(
        &launch_output,
        "Launch spec diagnostics:",
        "apple-vz-live-launch.output",
    )?;
    require_contains(
        &launch_output,
        &format!("AppleVzRunner starting VM: {vm_name}"),
        "apple-vz-live-launch.output",
    )?;
    require_contains(
        &launch_output,
        &format!("AppleVzRunner VM finished: {vm_name}"),
        "apple-vz-live-launch.output",
    )?;
    let boot_mode = json_string(&launch, &["boot", "mode"])?;
    if boot_mode != "linux-kernel" {
        return Err(format!("launch boot mode is not linux-kernel: {boot_mode}"));
    }
    let disk_format = json_string(&launch, &["disk", "format"])?;
    if disk_format != "raw" {
        return Err(format!("launch disk format is not raw: {disk_format}"));
    }
    if let Some(expected) = context.and_then(|context| context.disk_format.as_deref()) {
        if expected == "raw" && disk_format != expected {
            return Err(format!(
                "Apple VZ launch disk format {disk_format} does not match active disk format {expected}"
            ));
        }
    }
    let network = json_string(&launch, &["devices", "network"])?;
    if network != "nat" {
        return Err(format!("launch network is not nat: {network}"));
    }
    if !json_bool(&launch, &["readiness", "ready"])? {
        return Err("launch readiness is not ready".to_string());
    }
    if json_array_len(&launch, &["readiness", "blockers"])? != 0 {
        return Err("launch readiness blockers are not empty".to_string());
    }
    if json_string(&handoff, &["backend"])? != "apple-virtualization-framework" {
        return Err("handoff backend is not apple-virtualization-framework".to_string());
    }
    if !apple_vz_handoff_ready(&handoff)? {
        return Err("handoff is not ready".to_string());
    }
    if json_string(&handoff, &["vm_name"])? != vm_name {
        return Err("handoff VM name does not match launch spec".to_string());
    }

    for key in [
        "source_kernel",
        "source_raw_disk",
        "bundle_kernel",
        "bundle_raw_disk",
    ] {
        verify_fixture_entry(&manifest, key, true)?;
    }
    for key in ["source_initrd", "bundle_initrd"] {
        verify_fixture_entry(&manifest, key, false)?;
    }
    verify_fixture_pair(&manifest, "source_kernel", "bundle_kernel")?;
    verify_fixture_pair(&manifest, "source_raw_disk", "bundle_raw_disk")?;
    verify_fixture_pair(&manifest, "source_initrd", "bundle_initrd")?;

    let launch_kernel = json_string(&launch, &["boot", "kernel", "path"])?;
    let environment_values = parse_environment_values(&environment);
    require_environment_value(
        &environment_values,
        "BRIDGEVM_LIVE_VZ_KERNEL",
        &json_string(&manifest, &["source_kernel", "path"])?,
        "environment kernel path does not match source kernel evidence",
    )?;
    require_environment_value(
        &environment_values,
        "BRIDGEVM_LIVE_VZ_RAW_DISK",
        &json_string(&manifest, &["source_raw_disk", "path"])?,
        "environment raw disk path does not match source raw disk evidence",
    )?;
    require_environment_value(
        &environment_values,
        "BRIDGEVM_LIVE_VZ_KERNEL_CMDLINE",
        &json_string(&launch, &["boot", "kernel_command_line"])?,
        "environment kernel command line does not match launch spec",
    )?;
    require_environment_value(
        &environment_values,
        "BRIDGEVM_LIVE_VZ_MEMORY_MIB",
        &json_u64_like(&launch, &["resources", "memory"])?.to_string(),
        "environment memory does not match launch spec resources",
    )?;
    require_environment_value(
        &environment_values,
        "BRIDGEVM_LIVE_VZ_CPU_COUNT",
        &json_u64_like(&launch, &["resources", "cpu"])?.to_string(),
        "environment CPU count does not match launch spec resources",
    )?;
    let stop_after_seconds = require_environment_entry(
        &environment_values,
        "BRIDGEVM_LIVE_VZ_STOP_AFTER_SECONDS",
        "environment stop-after seconds is missing",
    )?;
    let force_stop_grace_seconds = require_environment_entry(
        &environment_values,
        "BRIDGEVM_LIVE_VZ_FORCE_STOP_GRACE_SECONDS",
        "environment force-stop grace seconds is missing",
    )?;
    require_contains(
        &summary,
        &format!("Stop after seconds: {stop_after_seconds}"),
        "SUMMARY.txt",
    )?;
    require_contains(
        &summary,
        &format!("Force stop grace seconds: {force_stop_grace_seconds}"),
        "SUMMARY.txt",
    )?;
    require_contains(
        &launch_output,
        &format!("BRIDGEVM_LIVE_VZ_STOP_AFTER_SECONDS={stop_after_seconds}"),
        "apple-vz-live-launch.output",
    )?;
    require_contains(
        &launch_output,
        &format!("BRIDGEVM_LIVE_VZ_FORCE_STOP_GRACE_SECONDS={force_stop_grace_seconds}"),
        "apple-vz-live-launch.output",
    )?;
    if let Some(environment_runner) = environment_values.get("BRIDGEVM_LIVE_VZ_RUNNER") {
        if environment_runner != "<auto-build>" && environment_runner != runner_path {
            return Err(
                "environment runner path does not match recorded AppleVzRunner path".to_string(),
            );
        }
    }

    let bundle_kernel = json_string(&manifest, &["bundle_kernel", "path"])?;
    if launch_kernel != bundle_kernel {
        return Err("launch kernel path does not match bundled kernel evidence".to_string());
    }
    let launch_disk = json_string(&launch, &["disk", "path"])?;
    let bundle_disk = json_string(&manifest, &["bundle_raw_disk", "path"])?;
    if launch_disk != bundle_disk {
        return Err("launch disk path does not match bundled raw disk evidence".to_string());
    }
    require_contains(
        &launch_output,
        &format!("Kernel: {launch_kernel} "),
        "apple-vz-live-launch.output",
    )?;
    require_contains(
        &launch_output,
        &format!("Disk: {launch_disk} "),
        "apple-vz-live-launch.output",
    )?;
    let runner_log_path = json_string(&launch, &["logs", "runner_log_path"])?;
    if let Ok(handoff_runner_log_path) = json_string(&handoff, &["runner_log_path"]) {
        if !handoff_runner_log_path.is_empty() && handoff_runner_log_path != runner_log_path {
            return Err("handoff runner log path does not match launch spec".to_string());
        }
    }
    let _runner_log = evidence_bundle_file_path(path, &runner_log_path, "Apple VZ runner log")?;

    let serial_expected = environment_values
        .get("BRIDGEVM_LIVE_VZ_SERIAL_EXPECTED")
        .filter(|value| !value.is_empty() && value.as_str() != "<unset>");
    let serial_sentinel_proven = if let Some(expected) = serial_expected {
        require_contains(
            &summary,
            &format!("required sentinel found: {expected}"),
            "SUMMARY.txt",
        )?;
        let serial_log_path = json_string(&launch, &["devices", "serial_log_path"])?;
        if let Ok(handoff_serial_log_path) = json_string(&handoff, &["serial_log_path"]) {
            if !handoff_serial_log_path.is_empty() && handoff_serial_log_path != serial_log_path {
                return Err("handoff serial log path does not match launch spec".to_string());
            }
        }
        let serial_log_path =
            evidence_bundle_file_path(path, &serial_log_path, "Apple VZ serial log")?;
        let serial_log = read_bounded_text_file(&serial_log_path, "serial log evidence")?;
        require_contains(&serial_log, expected, "serial log evidence")?;
        true
    } else {
        false
    };
    let graphical_boot_progress_proven = verify_graphical_boot_progress_evidence(path)?;
    let viewer_evidence_proven = verify_viewer_evidence(path)?;
    let guest_tools_effects_proven = verify_guest_tools_effects_evidence(path)?;

    Ok(VmLiveEvidenceVerification {
        path: path.to_path_buf(),
        backend: "apple-virtualization-framework".to_string(),
        vm_name,
        boot_mode,
        disk_format,
        network,
        serial_sentinel_required: serial_expected.is_some(),
        serial_sentinel_proven,
        graphical_boot_progress_proven,
        viewer_evidence_proven,
        qmp_evidence_proven: false,
        guest_tools_effects_proven,
        summary: "Apple VZ live boot opt-in smoke: passed".to_string(),
    })
}

fn evidence_bundle_file_path(root: &Path, artifact: &str, label: &str) -> Result<PathBuf, String> {
    if artifact.trim().is_empty() {
        return Err(format!("{label} path is empty"));
    }
    let artifact_path = Path::new(artifact);
    let full_path = if artifact_path.is_absolute() {
        artifact_path.to_path_buf()
    } else {
        if artifact_path.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir | std::path::Component::Prefix(_)
            )
        }) {
            return Err(format!("{label} path must stay inside the evidence bundle"));
        }
        root.join(artifact_path)
    };
    let root_canonical = fs::canonicalize(root)
        .map_err(|error| format!("failed to canonicalize evidence bundle: {error}"))?;
    let metadata = fs::symlink_metadata(&full_path)
        .map_err(|error| format!("{label} is not a file: {} ({error})", full_path.display()))?;
    if metadata.file_type().is_symlink() {
        return Err(format!(
            "{label} must not be a symlink: {}",
            full_path.display()
        ));
    }
    if !metadata.is_file() {
        return Err(format!("{label} is not a file: {}", full_path.display()));
    }
    let full_canonical = fs::canonicalize(&full_path)
        .map_err(|error| format!("failed to canonicalize {label}: {error}"))?;
    if !full_canonical.starts_with(&root_canonical) {
        return Err(format!(
            "{label} path must stay inside the evidence bundle: {}",
            full_path.display()
        ));
    }
    Ok(full_path)
}

fn verify_qemu_live_evidence_bundle(
    path: &Path,
    context: Option<&LiveEvidenceVerificationContext>,
) -> Result<VmLiveEvidenceVerification, String> {
    if !path.is_dir() {
        return Err(format!("evidence directory not found: {}", path.display()));
    }

    let evidence = read_evidence_json(path, "qemu-live-evidence.json")?;
    if !json_bool(&evidence, &["proven"])? {
        return Err("qemu-live-evidence.json does not mark live evidence as proven".to_string());
    }
    let backend = json_string(&evidence, &["backend"])?;
    if backend != "qemu" {
        return Err(format!(
            "qemu-live-evidence.json backend is not qemu: {backend}"
        ));
    }
    let vm_name = json_string(&evidence, &["vm_name"])?;
    if vm_name.trim().is_empty() {
        return Err("qemu-live-evidence.json vm_name is empty".to_string());
    }
    if let Some(context) = context {
        if context.mode != VmMode::Compatibility {
            return Err(format!(
                "QEMU live evidence cannot verify {} Mode VM {}",
                context.mode, context.vm_name
            ));
        }
        if vm_name != context.vm_name {
            return Err(format!(
                "qemu-live-evidence.json vm_name {vm_name} does not match readiness VM {}",
                context.vm_name
            ));
        }
    }
    let boot_mode = json_string(&evidence, &["boot_mode"])?;
    if boot_mode != "compatibility" {
        return Err(format!(
            "qemu-live-evidence.json boot_mode is not compatibility: {boot_mode}"
        ));
    }
    let disk_format = json_string(&evidence, &["disk_format"])?;
    if disk_format.trim().is_empty() {
        return Err("qemu-live-evidence.json disk_format is empty".to_string());
    }
    if disk_format != "qcow2" {
        return Err(format!(
            "qemu-live-evidence.json disk_format is not qcow2: {disk_format}"
        ));
    }
    if let Some(expected) = context.and_then(|context| context.disk_format.as_deref()) {
        if disk_format != expected {
            return Err(format!(
                "qemu-live-evidence.json disk_format {disk_format} does not match active disk format {expected}"
            ));
        }
    }
    let network = json_string(&evidence, &["network"])?;
    if network.trim().is_empty() {
        return Err("qemu-live-evidence.json network is empty".to_string());
    }
    if network != "nat" {
        return Err(format!(
            "qemu-live-evidence.json network is not nat: {network}"
        ));
    }
    if let Some(context) = context {
        if network != context.network {
            return Err(format!(
                "qemu-live-evidence.json network {network} does not match expected network {}",
                context.network
            ));
        }
    }

    let command = json_array(&evidence, &["command"])?;
    if command.is_empty() {
        return Err("qemu-live-evidence.json command is empty".to_string());
    }
    let mut command_args = Vec::new();
    for (index, arg) in command.iter().enumerate() {
        command_args.push(
            arg.as_str().ok_or_else(|| {
                format!("qemu-live-evidence.json command[{index}] is not a string")
            })?,
        );
    }
    let executable = command_args[0];
    if !is_supported_qemu_system_executable(executable) {
        return Err(format!(
            "qemu-live-evidence.json command[0] is not a supported qemu-system executable: {executable}"
        ));
    }
    let command_vm_name =
        command_option_value(&command_args, "-name", "qemu-live-evidence.json command")?;
    if command_vm_name != vm_name {
        return Err(format!(
            "qemu-live-evidence.json command -name {command_vm_name} does not match vm_name {vm_name}"
        ));
    }

    if !json_bool(&evidence, &["qmp", "running"])? {
        return Err("qemu-live-evidence.json qmp.running is not true".to_string());
    }
    let qmp_status = json_string(&evidence, &["qmp", "status"])?;
    if qmp_status != "running" {
        return Err(format!(
            "qemu-live-evidence.json qmp.status is not running: {qmp_status}"
        ));
    }
    let qmp_socket = json_string(&evidence, &["qmp", "socket"])?;
    if qmp_socket.trim().is_empty() {
        return Err("qemu-live-evidence.json qmp.socket is empty".to_string());
    }
    if let Some(context) = context {
        let expected_qmp_socket = context.qmp_socket.display().to_string();
        if qmp_socket != expected_qmp_socket {
            return Err(format!(
                "qemu-live-evidence.json qmp.socket {qmp_socket} does not match expected VM QMP socket {expected_qmp_socket}"
            ));
        }
    }
    let command_qmp =
        command_option_value(&command_args, "-qmp", "qemu-live-evidence.json command")?;
    let expected_qmp = format!("unix:{qmp_socket},server=on,wait=off");
    if command_qmp != expected_qmp {
        return Err(format!(
            "qemu-live-evidence.json qmp.socket {qmp_socket} does not match command -qmp {command_qmp}"
        ));
    }

    let qemu_log = verify_evidence_artifact_sha256(path, &evidence, &["artifacts", "qemu_log"])?;
    let serial_log =
        verify_evidence_artifact_sha256(path, &evidence, &["artifacts", "serial_log"])?;
    let qmp_transcript =
        verify_evidence_artifact_sha256(path, &evidence, &["artifacts", "qmp_transcript"])?;
    let qemu_log_content = read_bounded_text_file(&qemu_log, "QEMU log evidence")?;
    let command_line = command_args.join(" ");
    require_contains(&qemu_log_content, &vm_name, "QEMU log evidence")?;
    require_contains(
        &qemu_log_content,
        "QMP status: running",
        "QEMU log evidence",
    )?;
    require_contains(&qemu_log_content, executable, "QEMU log evidence")?;
    require_contains(&qemu_log_content, &qmp_socket, "QEMU log evidence")?;
    require_contains(
        &qemu_log_content,
        &format!("Command: {command_line}"),
        "QEMU log evidence",
    )?;
    require_contains(
        &qemu_log_content,
        &format!("QMP socket: {qmp_socket}"),
        "QEMU log evidence",
    )?;
    verify_qmp_transcript_evidence(&qmp_transcript)?;
    let serial_sentinel = json_string(&evidence, &["serial_sentinel"])?;
    let serial_sentinel_required = !serial_sentinel.trim().is_empty();
    let serial_sentinel_proven = if serial_sentinel_required {
        let serial_content = read_bounded_text_file(&serial_log, "QEMU serial evidence")?;
        require_contains(
            &serial_content,
            &serial_sentinel,
            "QEMU serial log evidence",
        )?;
        true
    } else {
        false
    };

    let graphical_boot_progress_proven = verify_graphical_boot_progress_evidence(path)?;
    let viewer_evidence_proven = verify_viewer_evidence(path)?;
    let guest_tools_effects_proven = verify_guest_tools_effects_evidence(path)?;

    Ok(VmLiveEvidenceVerification {
        path: path.to_path_buf(),
        backend,
        vm_name,
        boot_mode,
        disk_format,
        network,
        serial_sentinel_required,
        serial_sentinel_proven,
        graphical_boot_progress_proven,
        viewer_evidence_proven,
        qmp_evidence_proven: true,
        guest_tools_effects_proven,
        summary: "QEMU live evidence: passed".to_string(),
    })
}

fn is_supported_qemu_system_executable(executable: &str) -> bool {
    let Some(basename) = Path::new(executable)
        .file_name()
        .and_then(|name| name.to_str())
    else {
        return false;
    };
    matches!(
        basename,
        "qemu-system-aarch64" | "qemu-system-i386" | "qemu-system-riscv64" | "qemu-system-x86_64"
    )
}

fn verify_qmp_transcript_evidence(path: &Path) -> Result<(), String> {
    let content = read_bounded_text_file(path, "QMP transcript evidence")?;
    let mut saw_greeting = false;
    let mut saw_query_status_command = false;
    let mut saw_running_query_status_response = false;
    let mut pending_query_status_response = false;

    for (index, line) in content.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let value: serde_json::Value = serde_json::from_str(line).map_err(|error| {
            format!(
                "QMP transcript evidence line {} is not valid JSON: {error}",
                index + 1
            )
        })?;
        if value.get("QMP").is_some() {
            saw_greeting = true;
        }
        if value
            .get("execute")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|execute| execute == "query-status")
        {
            saw_query_status_command = true;
            pending_query_status_response = true;
        }
        if let Some(response) = value.get("return") {
            if response
                .get("status")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|status| status == "running")
                && response
                    .get("running")
                    .and_then(serde_json::Value::as_bool)
                    .is_some_and(|running| running)
                && pending_query_status_response
            {
                saw_running_query_status_response = true;
            }
            pending_query_status_response = false;
        }
    }

    if !saw_greeting {
        return Err("QMP transcript evidence missing QMP greeting".to_string());
    }
    if !saw_query_status_command {
        return Err("QMP transcript evidence missing query-status command".to_string());
    }
    if !saw_running_query_status_response {
        return Err("QMP transcript evidence missing running query-status response".to_string());
    }

    Ok(())
}

fn verify_evidence_artifact_sha256(
    root: &Path,
    value: &serde_json::Value,
    path: &[&str],
) -> Result<PathBuf, String> {
    let artifact = json_nested_string(value, path, "path")?;
    let sha256 = json_nested_string(value, path, "sha256")?;
    if !is_sha256_hex(&sha256) {
        return Err(format!(
            "{}.sha256 is not lowercase SHA-256 hex",
            path.join(".")
        ));
    }
    let artifact_path = relative_evidence_artifact_path(root, &artifact, &path.join("."))?;
    let bytes = fs::read(&artifact_path).map_err(|error| {
        format!(
            "failed to read evidence artifact {}: {error}",
            artifact_path.display()
        )
    })?;
    if bytes.is_empty() {
        return Err(format!("{} artifact is empty", path.join(".")));
    }
    let actual_sha256 = format!("{:x}", Sha256::digest(&bytes));
    if actual_sha256 != sha256 {
        return Err(format!("{}.sha256 does not match artifact", path.join(".")));
    }
    Ok(artifact_path)
}

fn json_nested_string(
    value: &serde_json::Value,
    base_path: &[&str],
    leaf: &str,
) -> Result<String, String> {
    let mut full_path = base_path.to_vec();
    full_path.push(leaf);
    json_string(value, &full_path)
}

fn relative_evidence_artifact_path(
    root: &Path,
    artifact: &str,
    label: &str,
) -> Result<PathBuf, String> {
    if artifact.trim().is_empty() {
        return Err(format!("{label} path is empty"));
    }
    let artifact_path = Path::new(artifact);
    if artifact_path.is_absolute()
        || artifact_path.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir | std::path::Component::Prefix(_)
            )
        })
    {
        return Err(format!(
            "{label} path must be relative and stay inside the evidence bundle"
        ));
    }
    let full_path = root.join(artifact_path);
    let metadata = fs::symlink_metadata(&full_path).map_err(|error| {
        format!(
            "{label} artifact is not a file: {} ({error})",
            full_path.display()
        )
    })?;
    if metadata.file_type().is_symlink() {
        return Err(format!(
            "{label} artifact must not be a symlink: {}",
            full_path.display()
        ));
    }
    if !metadata.is_file() {
        return Err(format!(
            "{label} artifact is not a file: {}",
            full_path.display()
        ));
    }
    Ok(full_path)
}

fn command_option_value<'a>(
    args: &'a [&str],
    option: &str,
    label: &str,
) -> Result<&'a str, String> {
    let index = args
        .iter()
        .position(|arg| *arg == option)
        .ok_or_else(|| format!("{label} is missing {option}"))?;
    args.get(index + 1)
        .copied()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("{label} {option} value is empty"))
}

fn verify_viewer_evidence(root: &Path) -> Result<bool, String> {
    verify_graphical_png_evidence(root, "viewer-evidence.json", "graphical-viewer")
        .map(|evidence| evidence.is_some())
}

fn verify_graphical_boot_progress_evidence(root: &Path) -> Result<bool, String> {
    let Some(evidence) = verify_graphical_png_evidence(
        root,
        "boot-progress-evidence.json",
        "graphical-boot-progress",
    )?
    else {
        return Ok(false);
    };

    let stage = json_string(&evidence, &["stage"])?;
    if stage.trim().is_empty() {
        return Err("boot-progress-evidence.json stage is empty".to_string());
    }
    let progress_marker = json_string(&evidence, &["progress_marker"])?;
    if progress_marker.trim().is_empty() {
        return Err("boot-progress-evidence.json progress_marker is empty".to_string());
    }

    Ok(true)
}

fn verify_graphical_png_evidence(
    root: &Path,
    file_name: &str,
    expected_kind: &str,
) -> Result<Option<serde_json::Value>, String> {
    let evidence_path = root.join(file_name);
    if !evidence_path.exists() {
        return Ok(None);
    }

    let evidence = read_evidence_json(root, file_name)?;
    if !json_bool(&evidence, &["proven"])? {
        return Err(format!(
            "{file_name} does not mark graphical evidence as proven"
        ));
    }
    let kind = json_string(&evidence, &["kind"])?;
    if kind != expected_kind {
        return Err(format!("{file_name} kind is not {expected_kind}: {kind}"));
    }
    let artifact = json_string(&evidence, &["artifact"])?;
    let full_artifact_path = relative_evidence_artifact_path(root, &artifact, file_name)?;
    let bytes = fs::read(&full_artifact_path).map_err(|error| {
        format!(
            "failed to read {file_name} artifact {}: {error}",
            full_artifact_path.display()
        )
    })?;
    if bytes.is_empty() {
        return Err(format!("{file_name} artifact is empty"));
    }
    let expected_sha256 = json_string(&evidence, &["sha256"])?;
    if !is_sha256_hex(&expected_sha256) {
        return Err(format!("{file_name} sha256 is not lowercase SHA-256 hex"));
    }
    let actual_sha256 = format!("{:x}", Sha256::digest(&bytes));
    if actual_sha256 != expected_sha256 {
        return Err(format!("{file_name} sha256 does not match artifact"));
    }
    let width = json_u64(&evidence, &["width"])?;
    let height = json_u64(&evidence, &["height"])?;
    if width == 0 || height == 0 {
        return Err(format!("{file_name} width and height must be nonzero"));
    }
    let (actual_width, actual_height) =
        png_dimensions(&bytes).ok_or_else(|| format!("{file_name} artifact is not a PNG image"))?;
    if actual_width != width || actual_height != height {
        return Err(format!(
            "{file_name} width and height do not match artifact pixels"
        ));
    }
    let observation = json_string(&evidence, &["observation"])?;
    if observation.trim().is_empty() {
        return Err(format!("{file_name} observation is empty"));
    }

    Ok(Some(evidence))
}

fn png_dimensions(bytes: &[u8]) -> Option<(u64, u64)> {
    const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
    if bytes.len() < 24 || &bytes[..8] != PNG_SIGNATURE {
        return None;
    }
    if &bytes[12..16] != b"IHDR" {
        return None;
    }
    let width = u32::from_be_bytes(bytes[16..20].try_into().ok()?) as u64;
    let height = u32::from_be_bytes(bytes[20..24].try_into().ok()?) as u64;
    if width == 0 || height == 0 {
        return None;
    }
    Some((width, height))
}

fn verify_guest_tools_effects_evidence(root: &Path) -> Result<bool, String> {
    let evidence_path = root.join("guest-tools-effects.json");
    if !evidence_path.exists() {
        return Ok(false);
    }

    let evidence = read_evidence_json(root, "guest-tools-effects.json")?;
    if !json_bool(&evidence, &["proven"])? {
        return Err("guest-tools-effects.json does not mark effects as proven".to_string());
    }
    let backend = json_string(&evidence, &["backend"])?;
    if backend != "bridgevm-tools-linux" {
        return Err(format!(
            "guest-tools-effects.json backend is not bridgevm-tools-linux: {backend}"
        ));
    }
    let command_request_id = json_string(&evidence, &["command", "request_id"])?;
    if command_request_id.trim().is_empty() {
        return Err("guest-tools-effects.json command request_id is empty".to_string());
    }
    let command_status = json_string(&evidence, &["command", "status"])?;
    if command_status != "ok" {
        return Err(format!(
            "guest-tools-effects.json command status is not ok: {command_status}"
        ));
    }
    let effects = json_array(&evidence, &["effects"])?;
    if effects.is_empty() {
        return Err("guest-tools-effects.json has no effect records".to_string());
    }

    let mut artifact_backed_effects = 0usize;
    for (index, effect) in effects.iter().enumerate() {
        let label = format!("guest-tools-effects.json effects[{index}]");
        let kind = json_string(effect, &["kind"])?;
        if kind.trim().is_empty() {
            return Err(format!("{label} has an empty kind"));
        }
        if !json_bool(effect, &["ok"])? {
            return Err(format!("{label} is not ok"));
        }
        let request_id = json_string(effect, &["request_id"])?;
        if request_id.trim().is_empty() {
            return Err(format!("{label} has an empty request_id"));
        }
        if request_id != command_request_id {
            return Err(format!("{label} request_id does not match command"));
        }
        let observation = json_string(effect, &["observation"])?;
        if observation.trim().is_empty() {
            return Err(format!("{label} has an empty observation"));
        }
        if verify_guest_tools_effect_observable(root, effect, &label)? {
            artifact_backed_effects += 1;
        }
    }
    if artifact_backed_effects == 0 {
        return Err(
            "guest-tools-effects.json needs at least one artifact/sha256-backed effect".to_string(),
        );
    }

    Ok(true)
}

fn verify_guest_tools_effect_observable(
    root: &Path,
    effect: &serde_json::Value,
    label: &str,
) -> Result<bool, String> {
    let expected_value = effect
        .get("expected_value")
        .and_then(serde_json::Value::as_str);
    let observed_value = effect
        .get("observed_value")
        .and_then(serde_json::Value::as_str);
    if let (Some(expected), Some(observed)) = (expected_value, observed_value) {
        if expected.trim().is_empty() {
            return Err(format!("{label} expected_value is empty"));
        }
        if observed != expected {
            return Err(format!(
                "{label} observed_value does not match expected_value"
            ));
        }
        return Ok(false);
    }

    let artifact = effect
        .get("artifact")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let sha256 = effect
        .get("sha256")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    if !artifact.trim().is_empty() || !sha256.trim().is_empty() {
        if artifact.trim().is_empty() {
            return Err(format!("{label} artifact is empty"));
        }
        if !is_sha256_hex(sha256) {
            return Err(format!("{label} sha256 is not lowercase SHA-256 hex"));
        }
        let artifact_path =
            evidence_bundle_file_path(root, artifact, &format!("{label} artifact"))?;
        let bytes = fs::read(&artifact_path).map_err(|error| {
            format!(
                "failed to read {label} artifact {}: {error}",
                artifact_path.display()
            )
        })?;
        let actual_sha256 = format!("{:x}", Sha256::digest(&bytes));
        if actual_sha256 != sha256 {
            return Err(format!("{label} sha256 does not match artifact"));
        }
        return Ok(true);
    }

    Err(format!(
        "{label} needs expected_value/observed_value or artifact/sha256 evidence"
    ))
}

fn read_evidence_text(root: &Path, name: &str) -> Result<String, String> {
    read_bounded_text_file(&root.join(name), &format!("evidence {name}"))
}

fn read_optional_evidence_text(root: &Path, name: &str) -> Result<Option<String>, String> {
    let path = root.join(name);
    if !path.exists() {
        return Ok(None);
    }
    read_bounded_text_file(&path, &format!("evidence {name}")).map(Some)
}

fn read_bounded_text_file(path: &Path, label: &str) -> Result<String, String> {
    let mut bytes = Vec::new();
    fs::File::open(path)
        .and_then(|file| {
            file.take(MAX_EVIDENCE_TEXT_BYTES + 1)
                .read_to_end(&mut bytes)
        })
        .map_err(|error| format!("failed to read {label} {}: {error}", path.display()))?;
    if bytes.len() as u64 > MAX_EVIDENCE_TEXT_BYTES {
        return Err(format!(
            "{label} {} exceeds the {MAX_EVIDENCE_TEXT_BYTES}-byte limit",
            path.display()
        ));
    }
    String::from_utf8(bytes)
        .map_err(|error| format!("{label} {} is not valid UTF-8: {error}", path.display()))
}

fn read_evidence_json(root: &Path, name: &str) -> Result<serde_json::Value, String> {
    let content = read_evidence_text(root, name)?;
    serde_json::from_str(&content).map_err(|error| format!("invalid evidence JSON {name}: {error}"))
}

fn require_contains(content: &str, needle: &str, label: &str) -> Result<(), String> {
    if content.contains(needle) {
        Ok(())
    } else {
        Err(format!("{label} missing {needle:?}"))
    }
}

fn parse_environment_values(content: &str) -> std::collections::BTreeMap<String, String> {
    content
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

fn require_environment_entry<'a>(
    values: &'a std::collections::BTreeMap<String, String>,
    key: &str,
    message: &str,
) -> Result<&'a str, String> {
    values
        .get(key)
        .map(String::as_str)
        .filter(|value| !value.is_empty() && *value != "<unset>")
        .ok_or_else(|| message.to_string())
}

fn require_environment_value(
    values: &std::collections::BTreeMap<String, String>,
    key: &str,
    expected: &str,
    message: &str,
) -> Result<(), String> {
    let actual = require_environment_entry(values, key, message)?;
    if actual == expected {
        Ok(())
    } else {
        Err(message.to_string())
    }
}

fn verify_fixture_entry(
    value: &serde_json::Value,
    key: &str,
    required_existing: bool,
) -> Result<(), String> {
    let exists = json_bool(value, &[key, "exists"])?;
    if required_existing && !exists {
        return Err(format!(
            "fixture manifest entry is not marked existing: {key}"
        ));
    }
    if exists {
        let path = json_string(value, &[key, "path"])?;
        if path.is_empty() {
            return Err(format!("fixture manifest entry has empty path: {key}"));
        }
        let bytes = json_u64(value, &[key, "bytes"])?;
        if bytes == 0 {
            return Err(format!(
                "fixture manifest entry has invalid byte count: {key}"
            ));
        }
        let sha256 = json_string(value, &[key, "sha256"])?;
        if !is_sha256_hex(&sha256) {
            return Err(format!("fixture manifest entry has invalid SHA-256: {key}"));
        }
        let metadata = fs::symlink_metadata(&path).map_err(|error| {
            format!("fixture manifest entry path is not a file: {key} ({error})")
        })?;
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "fixture manifest entry path must not be a symlink: {key}"
            ));
        }
        if !metadata.is_file() {
            return Err(format!("fixture manifest entry path is not a file: {key}"));
        }
        if metadata.len() != bytes {
            return Err(format!(
                "fixture manifest entry byte count does not match file: {key}"
            ));
        }
        let actual_sha256 = sha256_file(Path::new(&path))?;
        if actual_sha256 != sha256 {
            return Err(format!(
                "fixture manifest entry SHA-256 does not match file: {key}"
            ));
        }
    }
    Ok(())
}

fn verify_fixture_pair(
    value: &serde_json::Value,
    source_key: &str,
    bundle_key: &str,
) -> Result<(), String> {
    let source_exists = json_bool(value, &[source_key, "exists"])?;
    let bundle_exists = json_bool(value, &[bundle_key, "exists"])?;
    if source_exists != bundle_exists {
        return Err(format!(
            "source/bundle existence mismatch: {source_key} vs {bundle_key}"
        ));
    }
    if source_exists {
        if json_u64(value, &[source_key, "bytes"])? != json_u64(value, &[bundle_key, "bytes"])? {
            return Err(format!(
                "source/bundle byte count mismatch: {source_key} vs {bundle_key}"
            ));
        }
        if json_string(value, &[source_key, "sha256"])?
            != json_string(value, &[bundle_key, "sha256"])?
        {
            return Err(format!(
                "source/bundle SHA-256 mismatch: {source_key} vs {bundle_key}"
            ));
        }
    }
    Ok(())
}

fn json_at<'a>(
    value: &'a serde_json::Value,
    path: &[&str],
) -> Result<&'a serde_json::Value, String> {
    let mut current = value;
    for segment in path {
        current = current
            .get(*segment)
            .ok_or_else(|| format!("evidence JSON missing {}", path.join(".")))?;
    }
    Ok(current)
}

fn json_string(value: &serde_json::Value, path: &[&str]) -> Result<String, String> {
    json_at(value, path)?
        .as_str()
        .map(ToString::to_string)
        .ok_or_else(|| format!("evidence JSON field is not a string: {}", path.join(".")))
}

fn json_bool(value: &serde_json::Value, path: &[&str]) -> Result<bool, String> {
    json_at(value, path)?
        .as_bool()
        .ok_or_else(|| format!("evidence JSON field is not a bool: {}", path.join(".")))
}

fn apple_vz_handoff_ready(handoff: &serde_json::Value) -> Result<bool, String> {
    if let Ok(ready) = json_bool(handoff, &["readiness", "ready"]) {
        return Ok(ready);
    }
    json_bool(handoff, &["ready"])
}

fn json_u64(value: &serde_json::Value, path: &[&str]) -> Result<u64, String> {
    json_at(value, path)?
        .as_u64()
        .ok_or_else(|| format!("evidence JSON field is not a u64: {}", path.join(".")))
}

fn json_u64_like(value: &serde_json::Value, path: &[&str]) -> Result<u64, String> {
    let value = json_at(value, path)?;
    if let Some(number) = value.as_u64() {
        return Ok(number);
    }
    if let Some(text) = value.as_str() {
        return text
            .parse::<u64>()
            .map_err(|_| format!("evidence JSON field is not a u64: {}", path.join(".")));
    }
    Err(format!(
        "evidence JSON field is not a u64: {}",
        path.join(".")
    ))
}

fn json_array_len(value: &serde_json::Value, path: &[&str]) -> Result<usize, String> {
    json_at(value, path)?
        .as_array()
        .map(Vec::len)
        .ok_or_else(|| format!("evidence JSON field is not an array: {}", path.join(".")))
}

fn json_array<'a>(
    value: &'a serde_json::Value,
    path: &[&str],
) -> Result<&'a Vec<serde_json::Value>, String> {
    json_at(value, path)?
        .as_array()
        .ok_or_else(|| format!("evidence JSON field is not an array: {}", path.join(".")))
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

fn normalize_download_url(value: &str) -> Result<String, String> {
    let normalized = value.trim().to_string();
    if normalized.starts_with("https://") || normalized.starts_with("http://") {
        Ok(normalized)
    } else {
        Err("expected --url to start with http:// or https://".to_string())
    }
}

fn normalize_sha256(value: &str) -> Result<String, String> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.len() != 64 || !normalized.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("expected --sha256 to be a 64-character hex digest".to_string());
    }
    Ok(normalized)
}

fn sha256_file(path: &std::path::Path) -> Result<String, String> {
    let mut file = fs::File::open(path)
        .map_err(|error| format!("failed to open boot media {}: {error}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 1024 * 64];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("failed to read boot media {}: {error}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn run_backend(store: &VmStore, name: &str, spawn: bool) -> Result<RunnerMetadata, String> {
    let (bundle, mut manifest, _) = store
        .get_vm_with_active_disk(name)
        .map_err(|error| error.to_string())?;
    let runtime_engine = CurrentRuntimeEngine::for_manifest(&manifest);

    let (disk, active_disk) = store
        .prepare_active_disk(name)
        .map_err(|error| error.to_string())?;
    apply_active_disk_to_manifest(&mut manifest, &active_disk);
    if runtime_engine != CurrentRuntimeEngine::AppleVz && spawn && !disk.exists {
        return Err(missing_disk_message(&disk));
    }

    if runtime_engine == CurrentRuntimeEngine::AppleVz {
        // Gated REAL cold-start launch: when `BRIDGEVM_APPLE_VZ_RUNNER` is set
        // and the caller asked to spawn, boot a real Apple VZ VM via
        // `lightvm-runner` (fresh boot, no saved-state restore). When the env
        // is unset, preserve the legacy dry-run + runner-required fallback so
        // all existing metadata-safe smokes/tests stay green.
        if spawn && apple_vz_runner_configured() {
            return spawn_fast_backend(store, name, &bundle, &manifest, None, false, None);
        }
        let plan = build_fast_plan(&manifest, &bundle).map_err(|error| error.to_string())?;
        let launch_spec_path = write_launch_spec_artifact(&bundle, plan.launch_spec())
            .map_err(|error| error.to_string())?;
        let mut readiness = launch_readiness_metadata(&plan.launch_spec().readiness);
        if spawn {
            add_fast_spawn_runner_required_blocker(&mut readiness);
        }
        let spawn_error = spawn.then(|| fast_spawn_runner_required_error(&readiness));
        let metadata = RunnerMetadata {
            engine: runtime_engine.runner_metadata_engine().to_string(),
            pid: None,
            command: plan.render_runner_words_for_launch_spec(&launch_spec_path),
            log_path: plan.launch_spec().logs.runner_log_path.clone().into(),
            started_at_unix: now_unix(),
            dry_run: true,
            launch_spec_path: Some(launch_spec_path),
            guest_tools: None,
            disk: Some(disk),
            active_disk: Some(active_disk),
            launch_readiness: Some(readiness),
            runtime_control: None,
        };
        store
            .write_runner_metadata(name, &metadata)
            .map_err(|error| error.to_string())?;
        if let Some(error) = spawn_error {
            return Err(error);
        }
        return Ok(metadata);
    }

    let mut command = build_compatibility_command(&manifest, &bundle)
        .map_err(compatibility_qemu_command_error)?;
    let readiness = compatibility_launch_readiness_metadata(
        &disk,
        compatibility_launch_dependency_blockers(&manifest, &bundle),
    );
    if spawn && !readiness.ready {
        return Err(compatibility_launch_readiness_blocker_summary(&readiness));
    }
    let log_path = bundle.join("logs").join("qemu.log");
    let guest_tools = store
        .guest_tools_runner_metadata(name)
        .map_err(|error| error.to_string())?;

    if !spawn {
        let metadata = RunnerMetadata {
            engine: runtime_engine.runner_metadata_engine().to_string(),
            pid: None,
            command: command.render_shell_words(),
            log_path,
            started_at_unix: now_unix(),
            dry_run: true,
            launch_spec_path: None,
            guest_tools: Some(guest_tools),
            disk: Some(disk),
            active_disk: Some(active_disk),
            launch_readiness: Some(readiness),
            runtime_control: None,
        };
        store
            .write_runner_metadata(name, &metadata)
            .map_err(|error| error.to_string())?;
        return Ok(metadata);
    }

    // Pin this VM to a free VNC display before recording + spawning, so two
    // Compat VMs running at once don't collide on TCP 5900. (The dry-run path
    // above keeps the deterministic vnc=:0 template since it binds nothing.)
    // Daemon-less launches probe the live ports only; the daemon additionally
    // avoids displays it has already handed to its own children.
    assign_free_vnc_display(&mut command, &[])?;
    fs::create_dir_all(bundle.join("logs")).map_err(|error| error.to_string())?;
    let stdout = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|error| error.to_string())?;
    let stderr = stdout.try_clone().map_err(|error| error.to_string())?;
    let child = Command::new(&command.program)
        .args(&command.args)
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .spawn()
        .map_err(|error| format!("failed to spawn {}: {error}", command.program))?;
    let metadata = RunnerMetadata {
        engine: runtime_engine.runner_metadata_engine().to_string(),
        pid: Some(child.id()),
        command: command.render_shell_words(),
        log_path,
        started_at_unix: now_unix(),
        dry_run: false,
        launch_spec_path: None,
        guest_tools: Some(guest_tools),
        disk: Some(disk),
        active_disk: Some(active_disk),
        launch_readiness: Some(readiness),
        runtime_control: None,
    };
    store
        .write_runner_metadata(name, &metadata)
        .map_err(|error| error.to_string())?;
    store
        .transition_state(name, VmRuntimeState::Running)
        .map_err(|error| error.to_string())?;
    Ok(metadata)
}

fn compatibility_qemu_command_error(error: QemuError) -> String {
    format!("failed to build Compatibility Mode QEMU command: {error}")
}

pub fn compatibility_launch_readiness_metadata(
    disk: &bridgevm_storage::DiskPreparationMetadata,
    additional_blockers: Vec<LaunchReadinessBlockerMetadata>,
) -> LaunchReadinessMetadata {
    let mut blockers = Vec::new();
    if !disk.exists {
        blockers.push(LaunchReadinessBlockerMetadata {
            code: "missing-primary-disk".to_string(),
            message: missing_disk_message(disk),
            path: Some(disk.path.clone()),
            capability: Some("qemu".to_string()),
        });
    }
    blockers.extend(additional_blockers);
    LaunchReadinessMetadata {
        ready: blockers.is_empty(),
        blockers,
    }
}

pub fn compatibility_launch_dependency_blockers(
    manifest: &VmManifest,
    bundle: &Path,
) -> Vec<LaunchReadinessBlockerMetadata> {
    let mut blockers = Vec::new();

    if manifest.boot.as_ref().is_some_and(|boot| {
        boot.mode == BootMode::WindowsInstaller && boot.installer_image.is_some()
    }) {
        let installer = manifest
            .boot
            .as_ref()
            .and_then(|boot| boot.installer_image.as_deref())
            .expect("checked installer image presence");
        let path = resolve_bundle_path(bundle, installer);
        if !path.exists() {
            blockers.push(LaunchReadinessBlockerMetadata {
                code: "missing-windows-installer-image".to_string(),
                message: format!("Windows installer image is missing: {}", path.display()),
                path: Some(path),
                capability: Some("qemu-boot-media".to_string()),
            });
        }
    }

    if manifest.firmware.tpm {
        let path = swtpm_socket_path(bundle);
        if !path.exists() {
            blockers.push(LaunchReadinessBlockerMetadata {
                code: "missing-tpm-socket".to_string(),
                message: format!("firmware.tpm requires swtpm socket: {}", path.display()),
                path: Some(path),
                capability: Some("qemu-tpm".to_string()),
            });
        }
    }

    if manifest.firmware.secure_boot {
        let path = secure_boot_vars_path(bundle);
        if !path.exists() {
            blockers.push(LaunchReadinessBlockerMetadata {
                code: "missing-secure-boot-vars".to_string(),
                message: format!(
                    "firmware.secureBoot requires seeded edk2 variable store: {}",
                    path.display()
                ),
                path: Some(path),
                capability: Some("qemu-secure-boot".to_string()),
            });
        }
    }

    blockers.extend(compatibility_network_privilege_blockers(manifest));

    blockers
}

fn compatibility_network_privilege_blockers(
    manifest: &VmManifest,
) -> Vec<LaunchReadinessBlockerMetadata> {
    let Ok(mode) = manifest.network.mode.parse::<NetworkMode>() else {
        return Vec::new();
    };
    if !matches!(mode, NetworkMode::HostOnly | NetworkMode::Bridged) {
        return Vec::new();
    }
    let port_forwards = manifest
        .network
        .forwards
        .iter()
        .map(|forward| PortForwardRule {
            host: forward.host,
            guest: forward.guest,
        })
        .collect::<Vec<_>>();

    let Ok(plan) = plan_network(
        NetworkBackend::Qemu,
        mode,
        manifest.network.hostname.clone(),
        port_forwards,
    ) else {
        return Vec::new();
    };

    plan.requirements
        .into_iter()
        .map(|requirement| LaunchReadinessBlockerMetadata {
            code: requirement.blocker,
            message: requirement.requirement,
            path: None,
            capability: Some("qemu-network".to_string()),
        })
        .collect()
}

pub fn compatibility_launch_readiness_blocker_from_qemu_error(
    error: QemuError,
) -> LaunchReadinessBlockerMetadata {
    match error {
        QemuError::UnsupportedNetworkRequirement {
            mode,
            blocker,
            requirement,
        } => LaunchReadinessBlockerMetadata {
            code: blocker,
            message: format!(
                "{mode} networking requires an advanced Compatibility Mode QEMU schema before args can be generated; requirement: {requirement}"
            ),
            path: None,
            capability: Some("qemu-network".to_string()),
        },
        QemuError::NetworkPlan(error) => LaunchReadinessBlockerMetadata {
            code: "qemu-network-plan-invalid".to_string(),
            message: error.to_string(),
            path: None,
            capability: Some("qemu-network".to_string()),
        },
        QemuError::UnsupportedMode(mode) => LaunchReadinessBlockerMetadata {
            code: "qemu-unsupported-mode".to_string(),
            message: format!("QEMU command builder only supports Compatibility Mode manifests, got {mode}"),
            path: None,
            capability: Some("qemu".to_string()),
        },
        QemuError::UnsupportedNetworkMode(mode) => LaunchReadinessBlockerMetadata {
            code: "qemu-network-mode-unsupported".to_string(),
            message: format!("QEMU launch does not support {mode} networking yet"),
            path: None,
            capability: Some("qemu-network".to_string()),
        },
        QemuError::QmpIo(error) => LaunchReadinessBlockerMetadata {
            code: "qmp-io-error".to_string(),
            message: error.to_string(),
            path: None,
            capability: Some("qmp".to_string()),
        },
        QemuError::QmpJson(error) => LaunchReadinessBlockerMetadata {
            code: "qmp-json-error".to_string(),
            message: error.to_string(),
            path: None,
            capability: Some("qmp".to_string()),
        },
        QemuError::QmpProtocol(error) => LaunchReadinessBlockerMetadata {
            code: "qmp-protocol-error".to_string(),
            message: error,
            path: None,
            capability: Some("qmp".to_string()),
        },
        QemuError::MissingInstallerImage => LaunchReadinessBlockerMetadata {
            code: "qemu-missing-installer-image".to_string(),
            message: "windows-installer boot mode requires boot.installerImage".to_string(),
            path: None,
            capability: Some("qemu".to_string()),
        },
    }
}

fn compatibility_launch_readiness_blocker_summary(readiness: &LaunchReadinessMetadata) -> String {
    let summary = launch_readiness_blocker_summary(readiness);
    if summary.is_empty() {
        "Compatibility Mode launch readiness failed".to_string()
    } else {
        format!("Compatibility Mode launch readiness failed: {summary}")
    }
}

fn missing_disk_message(disk: &bridgevm_storage::DiskPreparationMetadata) -> String {
    if let Some(command) = &disk.create_command {
        format!(
            "primary disk is not ready: {}; create it with: {}",
            disk.path.display(),
            command.join(" ")
        )
    } else {
        format!("primary disk is not ready: {}", disk.path.display())
    }
}

fn apply_active_disk_to_manifest(
    manifest: &mut VmManifest,
    active_disk: &bridgevm_storage::ActiveDiskMetadata,
) {
    manifest.storage.primary.path = active_disk.path.display().to_string();
    manifest.storage.primary.format = active_disk.format.clone();
}

pub fn fast_spawn_runner_required_message() -> &'static str {
    "Fast Mode spawn requires BRIDGEVM_APPLE_VZ_RUNNER to point at a signed AppleVzRunner; dry-run planning metadata was updated"
}

pub fn fast_spawn_runner_required_error(readiness: &LaunchReadinessMetadata) -> String {
    let mut message = fast_spawn_runner_required_message().to_string();
    if !readiness.blockers.is_empty() {
        message.push_str("; launch blockers: ");
        message.push_str(&launch_readiness_blocker_summary(readiness));
    }
    message
}

pub fn launch_readiness_metadata(readiness: &AppleVzReadinessSpec) -> LaunchReadinessMetadata {
    LaunchReadinessMetadata {
        ready: readiness.ready,
        blockers: readiness
            .blockers
            .iter()
            .map(|blocker| LaunchReadinessBlockerMetadata {
                code: blocker.code.clone(),
                message: blocker.message.clone(),
                path: blocker.path.as_ref().map(PathBuf::from),
                capability: blocker.capability.clone(),
            })
            .collect(),
    }
}

pub fn add_fast_spawn_runner_required_blocker(readiness: &mut LaunchReadinessMetadata) {
    readiness.ready = false;
    if readiness
        .blockers
        .iter()
        .any(|blocker| blocker.code == "apple-vz-runner-unavailable")
    {
        return;
    }
    readiness.blockers.push(LaunchReadinessBlockerMetadata {
        code: "apple-vz-runner-unavailable".to_string(),
        message:
            "Fast Mode spawn needs BRIDGEVM_APPLE_VZ_RUNNER to point at a signed AppleVzRunner"
                .to_string(),
        path: None,
        capability: Some("apple-virtualization-framework".to_string()),
    });
}

fn launch_readiness_blocker_summary(readiness: &LaunchReadinessMetadata) -> String {
    readiness
        .blockers
        .iter()
        .map(|blocker| {
            let mut summary = format!("{}: {}", blocker.code, blocker.message);
            if let Some(path) = &blocker.path {
                summary.push_str(&format!(" ({})", path.display()));
            } else if let Some(capability) = &blocker.capability {
                summary.push_str(&format!(" ({capability})"));
            }
            summary
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Number of seconds a recorded backend process is given to exit gracefully
/// after `SIGTERM` (or a graceful QMP `quit`) before it is force-killed with
/// `SIGKILL`.
const STOP_TERMINATION_GRACE_SECONDS: u64 = 5;

/// Outcome of attempting to terminate a recorded backend process.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ProcessTerminationOutcome {
    /// No live process existed for the recorded pid (already gone).
    AlreadyGone,
    /// The process exited within the grace period after `SIGTERM`.
    ExitedAfterTerm,
    /// The process did not exit after `SIGTERM` and was force-killed with `SIGKILL`.
    Killed,
}

/// Whether a process with `pid` is currently alive.
///
/// Uses `kill -0`, which sends no signal but performs the permission/existence
/// check, so it reports liveness without disturbing the target.
#[cfg(unix)]
fn process_is_alive(pid: u32) -> bool {
    Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn process_is_alive(_pid: u32) -> bool {
    false
}

/// Send `signal` (e.g. `TERM`, `KILL`) to `pid` via the POSIX `kill` command.
///
/// Returns `Ok(())` even if the process is already gone (a no-op delivery),
/// matching the "make stop idempotent" contract.
#[cfg(unix)]
fn signal_process(pid: u32, signal: &str) -> Result<(), String> {
    let status = Command::new("kill")
        .arg(format!("-{signal}"))
        .arg(pid.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|error| format!("failed to send SIG{signal} to pid {pid}: {error}"))?;
    // A non-success status almost always means the process already exited
    // between our liveness check and the signal; treat that as success so stop
    // stays idempotent.
    let _ = status;
    Ok(())
}

#[cfg(not(unix))]
fn signal_process(_pid: u32, _signal: &str) -> Result<(), String> {
    Err("process termination is only supported on unix platforms".to_string())
}

/// Best-effort guard against PID reuse before we signal a recorded pid. A pid in
/// `runner.json` persists across crashes/reboots, and the OS can recycle it for
/// an unrelated process; signalling it blindly could SIGKILL a stranger. Compare
/// the live process's actual start time against the launch time we recorded:
/// only signal it if it started around then.
#[cfg(unix)]
fn recorded_process_is_ours(pid: u32, started_at_unix: u64) -> bool {
    // macOS `ps` exposes `etime` (elapsed, formatted `[[dd-]hh:]mm:ss`), not the
    // BSD/Linux `etimes` raw-seconds field.
    let output = match Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "etime="])
        .stderr(Stdio::null())
        .output()
    {
        Ok(out) if out.status.success() => out,
        // Can't query (process gone, or ps unavailable) -> don't signal it.
        _ => return false,
    };
    let elapsed_secs = match parse_ps_etime(&String::from_utf8_lossy(&output.stdout)) {
        Some(secs) => secs,
        None => return false,
    };
    let actual_start = now_unix().saturating_sub(elapsed_secs);
    // Allow generous slack for recording delay / clock skew, but reject a process
    // that started well before or after our recorded launch (i.e. a recycled pid).
    const TOLERANCE_SECS: u64 = 120;
    actual_start >= started_at_unix.saturating_sub(TOLERANCE_SECS)
        && actual_start <= started_at_unix.saturating_add(TOLERANCE_SECS)
}

/// Parse `ps -o etime` (`[[dd-]hh:]mm:ss`) into elapsed seconds.
fn parse_ps_etime(value: &str) -> Option<u64> {
    let value = value.trim();
    let (days, hms) = match value.split_once('-') {
        Some((days, rest)) => (days.trim().parse::<u64>().ok()?, rest),
        None => (0u64, value),
    };
    let parts: Vec<&str> = hms.split(':').collect();
    let (hours, minutes, seconds): (u64, u64, u64) = match parts.as_slice() {
        [h, m, s] => (h.parse().ok()?, m.parse().ok()?, s.parse().ok()?),
        [m, s] => (0, m.parse().ok()?, s.parse().ok()?),
        _ => return None,
    };
    Some(days * 86_400 + hours * 3_600 + minutes * 60 + seconds)
}

#[cfg(not(unix))]
fn recorded_process_is_ours(_pid: u32, _started_at_unix: u64) -> bool {
    false
}

/// Terminate the recorded backend process: `SIGTERM`, wait up to
/// `grace` for a clean exit, then `SIGKILL` if it is still alive.
///
/// This is the daemon-less stop path's equivalent of the daemon supervisor's
/// `Child::kill`: the API library only has the recorded pid (no `Child`
/// handle), so it signals the pid directly. Idempotent: a pid that is already
/// gone returns [`ProcessTerminationOutcome::AlreadyGone`].
fn terminate_recorded_process(
    pid: u32,
    started_at_unix: u64,
    grace: Duration,
) -> Result<ProcessTerminationOutcome, String> {
    // Guard against PID reuse: only signal a live pid that actually looks like
    // the process we launched. A recycled pid (or one whose identity we can't
    // confirm) is treated as already gone rather than risk killing a stranger.
    if !process_is_alive(pid) || !recorded_process_is_ours(pid, started_at_unix) {
        return Ok(ProcessTerminationOutcome::AlreadyGone);
    }

    signal_process(pid, "TERM")?;

    let deadline = Instant::now() + grace;
    while Instant::now() < deadline {
        if !process_is_alive(pid) {
            return Ok(ProcessTerminationOutcome::ExitedAfterTerm);
        }
        thread::sleep(Duration::from_millis(50));
    }

    if !process_is_alive(pid) {
        return Ok(ProcessTerminationOutcome::ExitedAfterTerm);
    }

    signal_process(pid, "KILL")?;

    // Give SIGKILL a brief window to take effect so a follow-up reconcile/stop
    // does not observe a zombie/live pid.
    let kill_deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < kill_deadline {
        if !process_is_alive(pid) {
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    Ok(ProcessTerminationOutcome::Killed)
}

/// Stop a VM's backend: gracefully quit QEMU over QMP (Compatibility Mode),
/// terminate the recorded child process (`SIGTERM` then `SIGKILL`) so no
/// AppleVzRunner / qemu orphan remains, then clear runtime state and metadata.
///
/// Dry-run VMs (no real recorded pid) keep their metadata-only behavior.
pub fn stop_backend(store: &VmStore, name: &str) -> Result<Option<RunnerMetadata>, String> {
    let (bundle, manifest) = store.get_vm(name).map_err(|error| error.to_string())?;
    let runtime_engine = CurrentRuntimeEngine::for_manifest(&manifest);
    let metadata = store
        .runner_metadata(name)
        .map_err(|error| error.to_string())?;

    // A real backend process is one recorded with a pid and not a dry run.
    // Both Fast (lightvm-runner / AppleVzRunner) and Compatibility (qemu)
    // backends record their child pid here.
    let recorded_pid = metadata
        .as_ref()
        .filter(|metadata| !metadata.dry_run)
        .and_then(|metadata| metadata.pid.map(|pid| (pid, metadata.started_at_unix)));

    // Compatibility Mode: attempt a graceful QMP quit first so QEMU can flush
    // and shut down cleanly. If the socket is gone but we have a live recorded
    // pid, fall through to signal-based termination rather than refusing.
    if runtime_engine.uses_qmp() {
        let socket_path = qmp_socket_path(&bundle);
        if socket_path.exists() {
            // Best-effort: if the guest already quit, the socket may error.
            if let Err(error) = qmp_quit(&socket_path) {
                // Only surface the error when there is no recorded pid to fall
                // back on; otherwise we proceed to terminate the pid directly.
                if recorded_pid.is_none() {
                    return Err(error.to_string());
                }
            }
        } else if recorded_pid.is_none()
            && metadata
                .as_ref()
                .is_some_and(|metadata| metadata.pid.is_some() && !metadata.dry_run)
        {
            // Defensive: pid present but filtered out should not happen, but keep
            // the historical guard for spawned-but-pidless edge cases.
            return Err(format!(
                "QMP socket unavailable: {}; refusing to mark spawned backend stopped",
                socket_path.display()
            ));
        }
    }

    // Release gate: actually terminate the recorded child process so no
    // AppleVzRunner / qemu orphan remains after stop. Dry-run VMs (no real pid)
    // skip this entirely and keep their prior metadata-only behavior.
    if let Some((pid, started_at_unix)) = recorded_pid {
        terminate_recorded_process(
            pid,
            started_at_unix,
            Duration::from_secs(STOP_TERMINATION_GRACE_SECONDS),
        )?;
    }

    // The backend process has been terminated -- the VM is definitively Stopped.
    // Force the transition so an unexpected prior recorded state can't leave a
    // killed backend recorded as Running/Suspended.
    store
        .force_transition_state(name, VmRuntimeState::Stopped)
        .map_err(|error| error.to_string())?;
    store
        .clear_runner_metadata(name)
        .map_err(|error| error.to_string())?;
    Ok(None)
}

pub fn restart_vm(store: &VmStore, name: &str) -> Result<VmRuntimeMetadata, String> {
    stop_backend(store, name)?;
    store
        .transition_state(name, VmRuntimeState::Running)
        .map_err(|error| error.to_string())
}

/// Default number of seconds the Fast Mode runner lets the guest run before it
/// pauses and saves machine state during a suspend.
const FAST_SUSPEND_RUN_SECONDS: u64 = 20;

/// Compute the Apple VZ saved-state file path for a VM.
///
/// Contract: `<bundle>/metadata/suspend-images/<slug(name)>.bin`.
pub fn fast_suspend_state_path(bundle: &Path, name: &str) -> PathBuf {
    bundle
        .join("metadata")
        .join("suspend-images")
        .join(format!("{}.bin", bridgevm_config::slug(name)))
}

/// Locate the `lightvm-runner` executable.
///
/// Honours `BRIDGEVM_LIGHTVM_RUNNER` (absolute path), then looks next to the
/// current executable, then falls back to `PATH` (mirrors the CLI's
/// executable-finding helper semantics).
fn find_lightvm_runner() -> PathBuf {
    if let Some(path) = std::env::var_os("BRIDGEVM_LIGHTVM_RUNNER") {
        return PathBuf::from(path);
    }
    if let Some(path) = bundled_executable_path("lightvm-runner") {
        return path;
    }
    if let Some(path) = path_executable("lightvm-runner") {
        return path;
    }
    PathBuf::from("lightvm-runner")
}

fn bundled_executable_path(name: &str) -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let candidate = exe.parent()?.join(name);
    is_executable_file(&candidate).then_some(candidate)
}

fn path_executable(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| is_executable_file(candidate))
}

fn is_executable_file(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        path.is_file()
            && path
                .metadata()
                .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
                .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        path.is_file()
    }
}

fn spawn_detached_fast_runner(command: &mut Command) -> std::io::Result<Child> {
    command.stdin(Stdio::null());
    #[cfg(unix)]
    {
        command.process_group(0);
    }
    command.spawn()
}

/// Resolve the signed AppleVzRunner from `BRIDGEVM_APPLE_VZ_RUNNER`.
fn require_apple_vz_runner() -> Result<PathBuf, String> {
    let path = std::env::var_os("BRIDGEVM_APPLE_VZ_RUNNER")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
        .ok_or_else(|| {
            "set BRIDGEVM_APPLE_VZ_RUNNER to a signed AppleVzRunner before suspending or resuming a Fast Mode VM"
                .to_string()
        })?;
    if !path.exists() {
        return Err(format!(
            "set BRIDGEVM_APPLE_VZ_RUNNER to a signed AppleVzRunner; {} does not exist",
            path.display()
        ));
    }
    Ok(path)
}

/// Whether `BRIDGEVM_APPLE_VZ_RUNNER` is set to a non-empty path.
///
/// This gates the REAL Fast Mode cold-start launch: when unset, the Fast spawn
/// path stays on the legacy dry-run + runner-required fallback for backward
/// compatibility. When set, `run_backend` (and `resume_backend`) launch a real
/// Apple VZ VM via `lightvm-runner`.
pub fn apple_vz_runner_configured() -> bool {
    std::env::var_os("BRIDGEVM_APPLE_VZ_RUNNER")
        .map(|value| !value.is_empty())
        .unwrap_or(false)
}

pub fn apple_vz_display_control_socket_path(bundle: &Path) -> PathBuf {
    PathBuf::from(format!(
        "/tmp/bvm-vz-{:016x}.sock",
        stable_runtime_control_socket_hash(&bundle.to_string_lossy())
    ))
}

pub fn apple_vz_display_framebuffer_rgba_path(bundle: &Path) -> PathBuf {
    bundle
        .join("metadata")
        .join("apple-vz-display-framebuffer.rgba")
}

fn stable_runtime_control_socket_hash(value: &str) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
    value.as_bytes().iter().fold(FNV_OFFSET, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(FNV_PRIME)
    })
}

pub fn runtime_control_command(
    store: &VmStore,
    name: &str,
    command: &str,
) -> Result<RuntimeControlCommandRecord, String> {
    let metadata = store
        .runner_metadata(name)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("No runner metadata recorded for {name}"))?;
    let control = metadata
        .runtime_control
        .ok_or_else(|| format!("No runtime control metadata recorded for {name}"))?;
    if !control
        .commands
        .iter()
        .any(|available| available == command)
    {
        let available = if control.commands.is_empty() {
            "none".to_string()
        } else {
            control.commands.join(", ")
        };
        return Err(format!(
            "runtime control `{}` is not advertised for {} (available: {})",
            command, name, available
        ));
    }

    let response = send_runtime_control_command(&control.socket_path, command)?;
    Ok(RuntimeControlCommandRecord {
        vm: name.to_string(),
        kind: control.kind,
        socket_path: control.socket_path,
        command: command.to_string(),
        response,
    })
}

fn send_runtime_control_command(socket: &Path, command: &str) -> Result<serde_json::Value, String> {
    let mut stream = UnixStream::connect(socket).map_err(|error| {
        format!(
            "failed to connect to runtime control socket {}: {}",
            socket.display(),
            error
        )
    })?;
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|error| format!("failed to configure runtime control read timeout: {error}"))?;
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .map_err(|error| format!("failed to configure runtime control write timeout: {error}"))?;
    let mut request = serde_json::to_vec(&serde_json::json!({ "command": command }))
        .map_err(|error| format!("failed to encode runtime control request: {error}"))?;
    request.push(b'\n');
    stream
        .write_all(&request)
        .map_err(|error| format!("failed to write runtime control request: {error}"))?;

    let mut response = Vec::new();
    BufReader::new(stream)
        .take(MAX_RUNTIME_CONTROL_RESPONSE_BYTES + 1)
        .read_until(b'\n', &mut response)
        .map_err(|error| format!("failed to read runtime control response: {error}"))?;
    if response.is_empty() {
        return Err("runtime control socket returned an empty response".to_string());
    }
    if response.len() as u64 > MAX_RUNTIME_CONTROL_RESPONSE_BYTES {
        return Err(format!(
            "runtime control response exceeded {} bytes",
            MAX_RUNTIME_CONTROL_RESPONSE_BYTES
        ));
    }
    if response.last() != Some(&b'\n') {
        return Err("runtime control socket returned an incomplete response frame".to_string());
    }
    serde_json::from_slice(&response)
        .map_err(|error| format!("invalid runtime control response JSON: {error}"))
}

/// Build the `lightvm-runner` argv used to launch a Fast Mode Apple VZ VM.
///
/// Shared by the Fast cold-start (`run_backend`) and resume (`resume_backend`)
/// paths. The only difference is the optional saved-state restore: a cold start
/// passes `restore_state == None` (fresh boot), while resume passes the saved
/// state file so the runner appends `--apple-vz-restore-state <file>`.
pub fn fast_runner_args(
    launch_spec_path: &Path,
    apple_vz_runner: &Path,
    restore_state: Option<&Path>,
    display: bool,
    display_size: Option<(u32, u32)>,
    runtime_control_socket: Option<&Path>,
    proxy_framebuffer_rgba_file: Option<&Path>,
    proxy_framebuffer_capture_interval_ms: Option<u64>,
) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "--launch-spec".to_string(),
        launch_spec_path.display().to_string(),
        "--require-ready".to_string(),
        "--launch".to_string(),
        "--apple-vz-runner".to_string(),
        apple_vz_runner.display().to_string(),
        "--apple-vz-allow-real-start".to_string(),
    ];
    if let Some(state_path) = restore_state {
        args.push("--apple-vz-restore-state".to_string());
        args.push(state_path.display().to_string());
    }
    // Embedded display: lightvm-runner forwards this as `--display` to the
    // AppleVzRunner, which boots with a graphics device + hosts a window.
    if display {
        args.push("--apple-vz-display".to_string());
        if let Some((width, height)) = display_size {
            args.push("--apple-vz-display-width".to_string());
            args.push(width.to_string());
            args.push("--apple-vz-display-height".to_string());
            args.push(height.to_string());
        }
        if let Some(socket_path) = runtime_control_socket {
            args.push("--apple-vz-runtime-control-socket".to_string());
            args.push(socket_path.display().to_string());
        }
        if let Some(path) = proxy_framebuffer_rgba_file {
            args.push("--apple-vz-proxy-framebuffer-rgba-file".to_string());
            args.push(path.display().to_string());
        }
        if let Some(interval_ms) = proxy_framebuffer_capture_interval_ms {
            args.push("--apple-vz-proxy-framebuffer-capture-interval-ms".to_string());
            args.push(interval_ms.to_string());
        }
    }
    args
}

/// Launch a Fast Mode Apple VZ VM via `lightvm-runner` (DETACHED).
///
/// Shared spawn path for the Fast cold-start (`restore_state == None`) and
/// resume (`restore_state == Some(state_file)`). Resolves the signed
/// AppleVzRunner, builds the launch spec, spawns the runner without waiting,
/// records the child pid with `dry_run:false`, and transitions the VM Running.
fn spawn_fast_backend(
    store: &VmStore,
    name: &str,
    bundle: &Path,
    manifest: &VmManifest,
    restore_state: Option<&Path>,
    display: bool,
    display_size: Option<(u32, u32)>,
) -> Result<RunnerMetadata, String> {
    let apple_vz_runner = require_apple_vz_runner()?;
    let lightvm_runner = find_lightvm_runner();

    let (disk, active_disk) = store
        .prepare_active_disk(name)
        .map_err(|error| error.to_string())?;

    let mut manifest = manifest.clone();
    apply_active_disk_to_manifest(&mut manifest, &active_disk);
    let plan = build_fast_plan(&manifest, bundle).map_err(|error| error.to_string())?;
    let launch_spec_path = write_launch_spec_artifact(bundle, plan.launch_spec())
        .map_err(|error| error.to_string())?;
    let readiness = launch_readiness_metadata(&plan.launch_spec().readiness);
    if !readiness.ready {
        return Err(format!(
            "Fast Mode launch readiness failed: {}",
            launch_readiness_blocker_summary(&readiness)
        ));
    }

    fs::create_dir_all(bundle.join("logs")).map_err(|error| error.to_string())?;
    fs::create_dir_all(bundle.join("run")).map_err(|error| error.to_string())?;
    let log_path: PathBuf = plan.launch_spec().logs.runner_log_path.clone().into();
    let stdout = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|error| error.to_string())?;
    let stderr = stdout.try_clone().map_err(|error| error.to_string())?;

    let runtime_control = display.then(|| RuntimeControlMetadata {
        kind: "apple-vz-display".to_string(),
        socket_path: apple_vz_display_control_socket_path(bundle),
        commands: vec![
            "status".to_string(),
            "stop".to_string(),
            "policy".to_string(),
            "pacing".to_string(),
        ],
    });
    let proxy_framebuffer_rgba_file =
        display.then(|| apple_vz_display_framebuffer_rgba_path(bundle));
    let args = fast_runner_args(
        &launch_spec_path,
        &apple_vz_runner,
        restore_state,
        display,
        display_size,
        runtime_control
            .as_ref()
            .map(|control| control.socket_path.as_path()),
        proxy_framebuffer_rgba_file.as_deref(),
        None,
    );

    let mut runner_command = Command::new(&lightvm_runner);
    runner_command
        .args(&args)
        .env("BRIDGEVM_APPLE_VZ_ALLOW_REAL_START", "1")
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    let child = spawn_detached_fast_runner(&mut runner_command)
        .map_err(|error| format!("failed to spawn {}: {error}", lightvm_runner.display()))?;

    let mut command = vec![lightvm_runner.display().to_string()];
    command.extend(args);
    let metadata = RunnerMetadata {
        engine: "lightvm".to_string(),
        pid: Some(child.id()),
        command,
        log_path,
        started_at_unix: now_unix(),
        dry_run: false,
        launch_spec_path: Some(launch_spec_path),
        guest_tools: None,
        disk: Some(disk),
        active_disk: Some(active_disk),
        launch_readiness: Some(readiness),
        runtime_control,
    };
    store
        .write_runner_metadata(name, &metadata)
        .map_err(|error| error.to_string())?;
    store
        .transition_state(name, VmRuntimeState::Running)
        .map_err(|error| error.to_string())?;
    if display {
        let policy = build_runtime_resource_policy_metadata(
            name,
            &manifest,
            RuntimeResourceVisibility::Foreground,
            VmRuntimeState::Running,
        );
        store
            .write_runtime_resource_policy_metadata(name, &policy)
            .map_err(|error| error.to_string())?;
    }

    Ok(metadata)
}

/// Boot a Fast Mode VM from cold (fresh boot, no saved-state restore).
///
/// Public entry point shared by the daemon-less CLI run path. Requires
/// `BRIDGEVM_APPLE_VZ_RUNNER` to be set (see [`apple_vz_runner_configured`]);
/// callers gate on that and fall back to dry-run planning when it is unset.
pub fn cold_start_fast_backend(store: &VmStore, name: &str) -> Result<RunnerMetadata, String> {
    let (bundle, mut manifest, _) = store
        .get_vm_with_active_disk(name)
        .map_err(|error| error.to_string())?;
    if manifest.mode != VmMode::Fast {
        return Err("cold-start launch is only implemented for Fast Mode VMs".to_string());
    }
    apply_power_aware_fast_resources(&mut manifest);
    spawn_fast_backend(store, name, &bundle, &manifest, None, false, None)
}

/// Expand `auto` Fast Mode memory/cpu using the host power state at launch time,
/// so a lightweight Apple VZ VM conserves resources on battery. Explicit per-VM
/// values are preserved (see [`bridgevm_resource_manager::resolve_memory`]). Only
/// applied to fresh launches — resume must reuse the saved-state config, and
/// preview/dry-run paths stay deterministic. Shared by the daemon-less CLI path
/// (here) and the daemon's own Fast cold-start so both adapt to battery.
pub fn apply_power_aware_fast_resources(manifest: &mut VmManifest) {
    use bridgevm_resource_manager::{
        decide_from_manifest_profile_with_power, read_on_battery, resolve_memory, resolve_vcpu,
    };
    let decision =
        decide_from_manifest_profile_with_power(&manifest.resources.profile, read_on_battery());
    manifest.resources.memory = resolve_memory(&manifest.resources.memory, &decision);
    manifest.resources.cpu = resolve_vcpu(&manifest.resources.cpu, &decision);
}

pub fn reapply_runtime_resources(
    store: &VmStore,
    name: &str,
    visibility: RuntimeResourceVisibility,
) -> Result<RuntimeResourcePolicyMetadata, String> {
    let (_, manifest) = store.get_vm(name).map_err(|error| error.to_string())?;
    if manifest.mode != VmMode::Fast {
        return Err("runtime resource reapply is only implemented for Fast Mode VMs".to_string());
    }

    let state = store.state(name).map_err(|error| error.to_string())?;
    if state.state != VmRuntimeState::Running {
        return Err(format!(
            "runtime resource reapply requires a running VM; current state is {}",
            state.state
        ));
    }

    let runner = store
        .runner_metadata(name)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "runtime resource reapply requires runner metadata".to_string())?;
    if runner.dry_run {
        return Err(
            "runtime resource reapply requires a real backend, not dry-run metadata".to_string(),
        );
    }

    let mut policy =
        build_runtime_resource_policy_metadata(name, &manifest, visibility, state.state);
    store
        .write_runtime_resource_policy_metadata(name, &policy)
        .map_err(|error| error.to_string())?;
    if runtime_control_policy_acknowledged(&runner) {
        policy.runtime_control_acknowledged = true;
        store
            .write_runtime_resource_policy_metadata(name, &policy)
            .map_err(|error| error.to_string())?;
    }
    Ok(policy)
}

fn runtime_control_policy_acknowledged(runner: &RunnerMetadata) -> bool {
    let Some(control) = &runner.runtime_control else {
        return false;
    };
    if !control.commands.iter().any(|command| command == "policy") {
        return false;
    }

    match send_runtime_control_command(&control.socket_path, "policy") {
        Ok(response) => response
            .get("ok")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        Err(_) => false,
    }
}

fn build_runtime_resource_policy_metadata(
    name: &str,
    manifest: &VmManifest,
    visibility: RuntimeResourceVisibility,
    state: VmRuntimeState,
) -> RuntimeResourcePolicyMetadata {
    use bridgevm_resource_manager::{
        decide_for_runtime, read_on_battery, resolve_memory, resolve_vcpu, ResourceProfile,
    };

    let on_battery = read_on_battery();
    let foreground = matches!(visibility, RuntimeResourceVisibility::Foreground);
    let decision = decide_for_runtime(
        ResourceProfile::parse(&manifest.resources.profile),
        foreground,
        on_battery,
    );
    RuntimeResourcePolicyMetadata {
        vm: name.to_string(),
        mode: manifest.mode.to_string(),
        profile: manifest.resources.profile.clone(),
        visibility,
        state,
        on_battery,
        memory: resolve_memory(&manifest.resources.memory, &decision),
        cpu: resolve_vcpu(&manifest.resources.cpu, &decision),
        display_fps_cap: decision.display_fps_cap,
        rationale: decision.rationale,
        live_applied: false,
        runtime_control_acknowledged: false,
        live_apply_blockers: vec![RuntimeResourcePolicyBlocker {
            code: "runtime-control-unavailable".to_string(),
            message: "Live Apple VZ CPU/RAM hot-apply is not implemented yet; the policy is available to display pacing and runtime policy IPC consumers.".to_string(),
        }],
        updated_at_unix: now_unix(),
    }
}

/// Boot a Fast Mode VM with an embedded graphical display: spawns the windowed
/// AppleVzRunner (via lightvm-runner `--apple-vz-display`) that hosts the VM in a
/// `VZVirtualMachineView` window. Requires `BRIDGEVM_APPLE_VZ_RUNNER` and a GUI
/// session. Unlike cold-start, the display path has no suspend/resume (a VZ
/// graphics device disables save/restore).
pub fn display_fast_backend(store: &VmStore, name: &str) -> Result<RunnerMetadata, String> {
    display_fast_backend_with_size(store, name, None)
}

pub fn display_fast_backend_with_size(
    store: &VmStore,
    name: &str,
    display_size: Option<(u32, u32)>,
) -> Result<RunnerMetadata, String> {
    let (bundle, mut manifest, _) = store
        .get_vm_with_active_disk(name)
        .map_err(|error| error.to_string())?;
    if manifest.mode != VmMode::Fast {
        return Err("embedded display is only implemented for Fast Mode VMs".to_string());
    }
    apply_power_aware_fast_resources(&mut manifest);
    spawn_fast_backend(store, name, &bundle, &manifest, None, true, display_size)
}

/// How long to wait for the QEMU `snapshot-save` job to conclude during a
/// Compatibility Mode suspend before giving up.
const COMPAT_SUSPEND_SNAPSHOT_TIMEOUT_SECONDS: u64 = 120;

/// Suspend a VM end-to-end, dispatching by mode.
///
/// Fast Mode: boots the VM via `lightvm-runner`, lets it run briefly, pauses,
/// saves the VZ machine state to `<bundle>/metadata/suspend-images/<slug>.bin`,
/// and exits (SYNCHRONOUS).
///
/// Compatibility Mode: connects to the running QEMU over QMP, pauses + saves a
/// full internal qcow2 snapshot, then quits QEMU
/// (see [`suspend_compatibility_backend`]).
pub fn suspend_backend(store: &VmStore, name: &str) -> Result<RunnerMetadata, String> {
    let (bundle, mut manifest, _) = store
        .get_vm_with_active_disk(name)
        .map_err(|error| error.to_string())?;
    if CurrentRuntimeEngine::for_manifest(&manifest).uses_qmp() {
        return suspend_compatibility_backend(store, name, &bundle);
    }

    let apple_vz_runner = require_apple_vz_runner()?;
    let lightvm_runner = find_lightvm_runner();

    let (disk, active_disk) = store
        .prepare_active_disk(name)
        .map_err(|error| error.to_string())?;
    apply_active_disk_to_manifest(&mut manifest, &active_disk);

    let plan = build_fast_plan(&manifest, &bundle).map_err(|error| error.to_string())?;
    let launch_spec_path = write_launch_spec_artifact(&bundle, plan.launch_spec())
        .map_err(|error| error.to_string())?;
    let readiness = launch_readiness_metadata(&plan.launch_spec().readiness);
    if !readiness.ready {
        return Err(format!(
            "Fast Mode launch readiness failed: {}",
            launch_readiness_blocker_summary(&readiness)
        ));
    }

    let state_path = fast_suspend_state_path(&bundle, name);
    if let Some(parent) = state_path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    fs::create_dir_all(bundle.join("logs")).map_err(|error| error.to_string())?;
    let log_path: PathBuf = plan.launch_spec().logs.runner_log_path.clone().into();
    let stdout = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|error| error.to_string())?;
    let stderr = stdout.try_clone().map_err(|error| error.to_string())?;

    let args: Vec<String> = vec![
        "--launch-spec".to_string(),
        launch_spec_path.display().to_string(),
        "--require-ready".to_string(),
        "--launch".to_string(),
        "--apple-vz-runner".to_string(),
        apple_vz_runner.display().to_string(),
        "--apple-vz-allow-real-start".to_string(),
        "--apple-vz-stop-after-seconds".to_string(),
        FAST_SUSPEND_RUN_SECONDS.to_string(),
        "--apple-vz-save-state".to_string(),
        state_path.display().to_string(),
    ];

    let status = Command::new(&lightvm_runner)
        .args(&args)
        .env("BRIDGEVM_APPLE_VZ_ALLOW_REAL_START", "1")
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .status()
        .map_err(|error| format!("failed to run {}: {error}", lightvm_runner.display()))?;
    if !status.success() {
        return Err(format!(
            "Fast Mode suspend runner exited with status {status}; see {}",
            log_path.display()
        ));
    }
    if !state_path.exists() {
        return Err(format!(
            "Fast Mode suspend runner finished but no saved state was written to {}",
            state_path.display()
        ));
    }

    let mut command = vec![lightvm_runner.display().to_string()];
    command.extend(args);
    let metadata = RunnerMetadata {
        engine: "lightvm".to_string(),
        pid: None,
        command,
        log_path,
        started_at_unix: now_unix(),
        dry_run: false,
        launch_spec_path: Some(launch_spec_path),
        guest_tools: None,
        disk: Some(disk),
        active_disk: Some(active_disk),
        launch_readiness: Some(readiness),
        runtime_control: None,
    };
    store
        .write_runner_metadata(name, &metadata)
        .map_err(|error| error.to_string())?;

    // Mark the suspend-image metadata so the saved state is discoverable
    // (image_exists=true after a successful suspend).
    store
        .mark_fast_suspend_image_exists(name, &state_path)
        .map_err(|error| error.to_string())?;

    // The machine state has been saved -- the VM is definitively Suspended now,
    // whatever the prior recorded state. Force the transition so an unexpected
    // prior state can't strand a saved-but-recorded-Running VM.
    store
        .force_transition_state(name, VmRuntimeState::Suspended)
        .map_err(|error| error.to_string())?;

    Ok(metadata)
}

/// Resume a previously suspended Fast Mode VM end-to-end.
///
/// Spawns `lightvm-runner` DETACHED (does not wait), restoring the saved VZ
/// machine state from `<bundle>/metadata/suspend-images/<slug>.bin`, records
/// the runner pid, and marks the VM running.
pub fn resume_backend(store: &VmStore, name: &str) -> Result<RunnerMetadata, String> {
    let (bundle, manifest, _) = store
        .get_vm_with_active_disk(name)
        .map_err(|error| error.to_string())?;
    if CurrentRuntimeEngine::for_manifest(&manifest).uses_qmp() {
        return resume_compatibility_backend(store, name, &bundle, &manifest);
    }

    let state_path = fast_suspend_state_path(&bundle, name);
    if !state_path.exists() {
        return Err(format!(
            "no saved Fast Mode state to resume from at {}; suspend the VM first",
            state_path.display()
        ));
    }

    // Resume is identical to a Fast cold start except it restores the saved VZ
    // machine state instead of booting fresh.
    spawn_fast_backend(
        store,
        name,
        &bundle,
        &manifest,
        Some(&state_path),
        false,
        None,
    )
}

/// Path to the Compatibility Mode suspend marker/metadata for a VM.
///
/// Records that an internal QEMU snapshot tagged [`COMPAT_SUSPEND_SNAPSHOT_TAG`]
/// lives inside the primary qcow2 so resume knows there is state to restore.
pub fn compat_suspend_marker_path(bundle: &Path, name: &str) -> PathBuf {
    bundle
        .join("metadata")
        .join("suspend-images")
        .join(format!("{}-compat.json", bridgevm_config::slug(name)))
}

/// Suspend a Compatibility Mode (QEMU) VM.
///
/// Connects to the running QEMU over QMP, pauses the guest (`stop`), saves a
/// full internal VM snapshot (CPU + RAM + device state) into the primary qcow2
/// via the job-based `snapshot-save` QMP command (tag
/// [`COMPAT_SUSPEND_SNAPSHOT_TAG`]), waits for the job to conclude, then `quit`s
/// QEMU. The recorded child pid (if any) is terminated to guarantee no orphan
/// remains, the suspend marker is recorded, and the VM is marked `suspended`.
fn suspend_compatibility_backend(
    store: &VmStore,
    name: &str,
    bundle: &Path,
) -> Result<RunnerMetadata, String> {
    let socket_path = qmp_socket_path(bundle);
    if !socket_path.exists() {
        return Err(format!(
            "QMP socket unavailable: {}; is the Compatibility Mode VM running?",
            socket_path.display()
        ));
    }

    let metadata = store
        .runner_metadata(name)
        .map_err(|error| error.to_string())?;
    let recorded_pid = metadata
        .as_ref()
        .filter(|metadata| !metadata.dry_run)
        .and_then(|metadata| metadata.pid.map(|pid| (pid, metadata.started_at_unix)));

    // Pause + save full machine state into the qcow2, then quit QEMU.
    suspend_to_snapshot(
        &socket_path,
        Duration::from_secs(COMPAT_SUSPEND_SNAPSHOT_TIMEOUT_SECONDS),
    )
    .map_err(|error| format!("Compatibility Mode suspend (snapshot-save) failed: {error}"))?;
    // The snapshot is committed; quit QEMU so the process releases the disk.
    qmp_quit(&socket_path).map_err(|error| error.to_string())?;

    // Guarantee the QEMU process is gone (QMP quit usually does this, but the
    // recorded pid is the release-gate backstop).
    if let Some((pid, started_at_unix)) = recorded_pid {
        terminate_recorded_process(
            pid,
            started_at_unix,
            Duration::from_secs(STOP_TERMINATION_GRACE_SECONDS),
        )?;
    }

    // Record a suspend marker so resume knows there is internal state to load.
    let marker_path = compat_suspend_marker_path(bundle, name);
    if let Some(parent) = marker_path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    // Reuse the disk path as the "image path" for the suspend-image metadata so
    // `mark_fast_suspend_image_exists` reports the location of the saved state.
    let disk_path = bundle.join("disks").join("root.qcow2");
    store
        .mark_fast_suspend_image_exists(name, &disk_path)
        .map_err(|error| error.to_string())?;
    fs::write(
        &marker_path,
        format!(
            "{{\"snapshot_tag\":\"{}\",\"disk\":\"{}\"}}\n",
            COMPAT_SUSPEND_SNAPSHOT_TAG,
            disk_path.display()
        ),
    )
    .map_err(|error| error.to_string())?;

    // Build a descriptive runner metadata (no live pid; backend is suspended).
    let command = build_compatibility_command(
        &store.get_vm(name).map_err(|error| error.to_string())?.1,
        bundle,
    )
    .map_err(compatibility_qemu_command_error)?;
    let log_path = bundle.join("logs").join("qemu.log");
    let guest_tools = store
        .guest_tools_runner_metadata(name)
        .map_err(|error| error.to_string())?;
    let suspend_metadata = RunnerMetadata {
        engine: "fullvm".to_string(),
        pid: None,
        command: command.render_shell_words(),
        log_path,
        started_at_unix: now_unix(),
        dry_run: false,
        launch_spec_path: None,
        guest_tools: Some(guest_tools),
        disk: None,
        active_disk: None,
        launch_readiness: None,
        runtime_control: None,
    };
    store
        .write_runner_metadata(name, &suspend_metadata)
        .map_err(|error| error.to_string())?;

    // The snapshot is committed and QEMU has been quit/killed -- the VM is
    // definitively Suspended. Force the transition so an unexpected prior state
    // can't leave a killed backend recorded as Running with an orphaned snapshot.
    store
        .force_transition_state(name, VmRuntimeState::Suspended)
        .map_err(|error| error.to_string())?;

    Ok(suspend_metadata)
}

/// Build the QEMU command used to resume a suspended Compatibility Mode VM:
/// the normal compatibility command plus `-loadvm <tag>` so QEMU restores the
/// internal VM snapshot saved during suspend.
///
/// Shared by the daemon-less resume path ([`resume_backend`]) and the daemon's
/// supervised resume so both spawn an identical process.
pub fn build_compatibility_resume_command(
    manifest: &VmManifest,
    bundle: &Path,
) -> Result<QemuCommand, String> {
    let mut command =
        build_compatibility_command(manifest, bundle).map_err(compatibility_qemu_command_error)?;
    command.args.push("-loadvm".to_string());
    command.args.push(COMPAT_SUSPEND_SNAPSHOT_TAG.to_string());
    Ok(command)
}

/// Confirm that a spawned QEMU process survived Compatibility Mode `-loadvm`.
///
/// QEMU can abort quickly while restoring an internal snapshot. The caller must
/// only consume the suspend marker after this returns `Ok(())`.
pub fn verify_compatibility_resume_loaded(
    child: &mut Child,
    bundle: &Path,
    log_path: &Path,
) -> Result<(), String> {
    // QEMU `-loadvm` can fail fast while restoring the snapshot — notably,
    // restoring an HVF-accelerated arm64 guest aborts in cpu_pre_load
    // (cpreg_vmstate_indexes). Confirm the process actually survived loading the
    // snapshot before declaring the VM running; otherwise report honestly and
    // leave the suspend marker + qcow2 snapshot intact so nothing is lost.
    // Poll over a readiness window so a -loadvm abort is caught WHENEVER it exits
    // (not only at a single fixed 2s), since we must not consume the irreplaceable
    // suspend marker unless the VM truly came back.
    let resume_deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(Some(status)) = child.try_wait() {
            return Err(format!(
                "Compatibility Mode resume failed: QEMU exited ({status}) while restoring the saved snapshot. Restoring a QEMU snapshot is not supported for HVF-accelerated arm64 guests on this host; the suspend snapshot is preserved. See {}.",
                log_path.display()
            ));
        }
        if Instant::now() >= resume_deadline {
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }
    // The process survived the window. If QMP is reachable and reports a terminal
    // status, the restore didn't actually come up -> fail and preserve the
    // snapshot (kill the half-up QEMU so it can't orphan). If QMP isn't reachable
    // we rely on the survived-the-window signal rather than risk a false failure.
    if let Ok(status) = query_status(&qmp_socket_path(bundle)) {
        if status.is_terminal() {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "Compatibility Mode resume: QEMU reported terminal status '{}' after restoring the snapshot; the suspend snapshot is preserved. See {}.",
                status.status,
                log_path.display()
            ));
        }
    }

    Ok(())
}

/// Resume a suspended Compatibility Mode (QEMU) VM.
///
/// Relaunches QEMU detached with `-loadvm <tag>` appended to the built command
/// so it restores the internal VM snapshot saved during suspend, records the
/// new child pid, and marks the VM `running`.
fn resume_compatibility_backend(
    store: &VmStore,
    name: &str,
    bundle: &Path,
    manifest: &VmManifest,
) -> Result<RunnerMetadata, String> {
    let marker_path = compat_suspend_marker_path(bundle, name);
    if !marker_path.exists() {
        return Err(format!(
            "no saved Compatibility Mode state to resume from at {}; suspend the VM first",
            marker_path.display()
        ));
    }

    let (disk, active_disk) = store
        .prepare_active_disk(name)
        .map_err(|error| error.to_string())?;
    if !disk.exists {
        return Err(missing_disk_message(&disk));
    }

    let mut manifest = manifest.clone();
    apply_active_disk_to_manifest(&mut manifest, &active_disk);
    let mut command = build_compatibility_resume_command(&manifest, bundle)?;
    // Pin a free VNC display so a resumed Compat VM doesn't collide on 5900.
    assign_free_vnc_display(&mut command, &[])?;

    let log_path = bundle.join("logs").join("qemu.log");
    let guest_tools = store
        .guest_tools_runner_metadata(name)
        .map_err(|error| error.to_string())?;

    fs::create_dir_all(bundle.join("logs")).map_err(|error| error.to_string())?;
    let stdout = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|error| error.to_string())?;
    let stderr = stdout.try_clone().map_err(|error| error.to_string())?;
    let mut child = Command::new(&command.program)
        .args(&command.args)
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .spawn()
        .map_err(|error| format!("failed to spawn {}: {error}", command.program))?;

    verify_compatibility_resume_loaded(&mut child, bundle, &log_path)?;

    let metadata = RunnerMetadata {
        engine: "fullvm".to_string(),
        pid: Some(child.id()),
        command: command.render_shell_words(),
        log_path,
        started_at_unix: now_unix(),
        dry_run: false,
        launch_spec_path: None,
        guest_tools: Some(guest_tools),
        disk: Some(disk),
        active_disk: Some(active_disk),
        launch_readiness: None,
        runtime_control: None,
    };
    store
        .write_runner_metadata(name, &metadata)
        .map_err(|error| error.to_string())?;

    // Resume succeeded (process survived snapshot load); consume the marker so a
    // subsequent stop->run doesn't try to resume stale state.
    let _ = fs::remove_file(&marker_path);

    store
        .transition_state(name, VmRuntimeState::Running)
        .map_err(|error| error.to_string())?;

    Ok(metadata)
}

fn lifecycle_plan(
    store: &VmStore,
    name: &str,
    action: LifecycleAction,
) -> Result<LifecyclePlanRecord, String> {
    let (bundle, manifest) = store.get_vm(name).map_err(|error| error.to_string())?;
    let current_state = store.state(name).map_err(|error| error.to_string())?.state;
    let qmp_supervisor = store
        .qmp_supervisor_metadata(name)
        .map_err(|error| error.to_string())?;
    let target_state = action.target_state();
    let valid_transition = lifecycle_transition_is_valid(current_state, action);
    let mut blockers = Vec::new();
    let mut notes = vec!["metadata-only lifecycle plan; no backend command was sent".to_string()];

    if !valid_transition {
        blockers.push(format!(
            "invalid-lifecycle-transition:{current_state}->{target_state}"
        ));
    }

    let runtime_engine = CurrentRuntimeEngine::for_manifest(&manifest);
    let (backend, qmp_command, socket_path, socket_available) = match runtime_engine {
        CurrentRuntimeEngine::QemuCompatibility => {
            let socket_path = qmp_socket_path(&bundle);
            let socket_available = socket_path.exists();
            if !socket_available {
                blockers.push(format!("qmp-socket-unavailable:{}", socket_path.display()));
            }
            notes.push("Compatibility Mode lifecycle control maps to QMP stop/cont".to_string());
            (
                runtime_engine.lifecycle_backend_label().to_string(),
                Some(action.qmp_command().to_string()),
                Some(socket_path),
                socket_available,
            )
        }
        CurrentRuntimeEngine::AppleVz => {
            if let Err(error) = require_apple_vz_runner() {
                blockers.push(format!("apple-vz-runner-unavailable:{error}"));
            }
            notes.push(
                "Fast Mode suspend/resume is wired through the runner via Apple VZ \
                 saveMachineState/restoreMachineState (not QMP); a real suspend/resume \
                 requires a signed AppleVzRunner (BRIDGEVM_APPLE_VZ_RUNNER)"
                    .to_string(),
            );
            (
                runtime_engine.lifecycle_backend_label().to_string(),
                None,
                None,
                false,
            )
        }
    };

    Ok(LifecyclePlanRecord {
        vm: name.to_string(),
        action,
        current_state,
        target_state,
        backend,
        metadata_only: true,
        executable: blockers.is_empty(),
        qmp_command,
        socket_path,
        socket_available,
        qmp_supervisor,
        blockers,
        notes,
    })
}

fn lifecycle_transition_is_valid(from: VmRuntimeState, action: LifecycleAction) -> bool {
    matches!(
        (from, action),
        (VmRuntimeState::Running, LifecycleAction::Suspend)
            | (VmRuntimeState::Suspended, LifecycleAction::Resume)
    )
}

fn execute_qmp_control<F>(
    store: &VmStore,
    name: &str,
    command: &str,
    execute: F,
) -> Result<QmpCommandRecord, String>
where
    F: FnOnce(&Path) -> Result<(), QemuError>,
{
    let (bundle, _) = store.get_vm(name).map_err(|error| error.to_string())?;
    let socket_path = qmp_socket_path(&bundle);
    if !socket_path.exists() {
        return Err(format!("QMP socket unavailable: {}", socket_path.display()));
    }

    execute(&socket_path).map_err(|error| error.to_string())?;
    Ok(QmpCommandRecord {
        vm: name.to_string(),
        socket_path,
        command: command.to_string(),
    })
}

fn records(store: &VmStore) -> Result<Vec<VmRecord>, bridgevm_storage::StorageError> {
    store
        .list_vms()?
        .into_iter()
        .map(|(_, manifest)| record_for(store, &manifest.name))
        .collect()
}

fn record_for(store: &VmStore, name: &str) -> Result<VmRecord, bridgevm_storage::StorageError> {
    let (path, manifest) = store.get_vm(name)?;
    let state = store.state(name)?.state.to_string();
    Ok(VmRecord {
        name: manifest.name,
        mode: manifest.mode.to_string(),
        guest_os: manifest.guest.os,
        guest_arch: manifest.guest.arch,
        state,
        path,
        qmp_supervisor: store.qmp_supervisor_metadata(name)?,
    })
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bridgevm_agent_protocol::{AgentAuth, AgentMessage, PROTOCOL_VERSION};
    use bridgevm_config::{BootMode, Guest, SharedFolder, VmMode};
    use bridgevm_core::boot_template_by_id;
    use bridgevm_storage::GuestToolsIpAddressMetadata;
    use std::net::TcpListener;
    use std::thread::JoinHandle;

    #[test]
    fn service_contract_marks_json_and_grpc_uds_migration_boundary() {
        let current = BridgeVmServiceContract::json_over_uds();
        let target = BridgeVmServiceContract::grpc_over_uds();

        assert!(current.is_same_contract_as(&target));
        assert_eq!(current.schema_id, BRIDGEVM_API_SCHEMA_ID);
        assert_eq!(current.version, BRIDGEVM_API_CONTRACT_VERSION);
        assert_eq!(current.service, BRIDGEVM_API_SERVICE_NAME);
        assert_eq!(current.transport, BRIDGEVM_API_JSON_OVER_UDS_TRANSPORT);
        assert_eq!(target.transport, BRIDGEVM_API_GRPC_OVER_UDS_TRANSPORT);
    }

    #[test]
    fn service_contract_serializes_as_stable_schema_marker() {
        let json = serde_json::to_value(BridgeVmServiceContract::json_over_uds()).unwrap();

        assert_eq!(
            json,
            serde_json::json!({
                "schema_id": "bridgevm.api/v1",
                "version": 1,
                "service": "bridgevm.api.v1.BridgeVmService",
                "request_type": "BridgeVmRequest",
                "response_type": "BridgeVmResponse",
                "transport": "json-ndjson-over-uds"
            })
        );
    }

    #[test]
    fn service_contract_does_not_wrap_existing_json_request_shape() {
        let request = BridgeVmRequest::ListVms;
        let json = serde_json::to_string(&request).unwrap();

        assert_eq!(json, r#"{"type":"list_vms"}"#);
        assert!(!json.contains("schema_id"));
        assert!(!json.contains("version"));

        let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(request, decoded);
    }

    #[test]
    fn current_runtime_engine_preserves_mode_to_engine_boundary() {
        let fast = CurrentRuntimeEngine::for_mode(VmMode::Fast);
        assert_eq!(fast, CurrentRuntimeEngine::AppleVz);
        assert_eq!(fast.network_backend(), NetworkBackend::AppleVz);
        assert_eq!(fast.runner_metadata_engine(), "lightvm");
        assert!(!fast.uses_qmp());
        assert_eq!(fast.lifecycle_backend_label(), "apple-vz");

        let compatibility = CurrentRuntimeEngine::for_mode(VmMode::Compatibility);
        assert_eq!(compatibility, CurrentRuntimeEngine::QemuCompatibility);
        assert_eq!(compatibility.network_backend(), NetworkBackend::Qemu);
        assert_eq!(compatibility.runner_metadata_engine(), "fullvm");
        assert!(compatibility.uses_qmp());
        assert_eq!(compatibility.lifecycle_backend_label(), "qemu-qmp");
    }

    #[test]
    fn doctor_request_and_response_round_trip_as_json() {
        let request = BridgeVmRequest::Doctor;
        let json = serde_json::to_string(&request).unwrap();
        assert_eq!(json, r#"{"type":"doctor"}"#);
        let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(request, decoded);

        let response = BridgeVmResponse::Doctor {
            store_root: PathBuf::from("/tmp/bridgevm"),
            vms_dir: PathBuf::from("/tmp/bridgevm/vms"),
            status: "OK".to_string(),
        };
        let json = serde_json::to_string(&response).unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&json).unwrap(),
            serde_json::json!({
                "type": "doctor",
                "store_root": "/tmp/bridgevm",
                "vms_dir": "/tmp/bridgevm/vms",
                "status": "OK"
            })
        );
        let decoded: BridgeVmResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(response, decoded);
    }

    #[test]
    fn request_round_trips_as_json() {
        let request = BridgeVmRequest::RecommendMode {
            choice: GuestChoice {
                os: "ubuntu".to_string(),
                version: None,
                arch: "arm64".to_string(),
            },
        };
        let json = serde_json::to_string(&request).unwrap();
        let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(request, decoded);
    }

    #[test]
    fn list_templates_request_round_trips_as_json() {
        let request = BridgeVmRequest::ListTemplates;
        let json = serde_json::to_string(&request).unwrap();
        let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(request, decoded);
    }

    #[test]
    fn create_vm_from_template_request_round_trips_as_json() {
        let request = BridgeVmRequest::CreateVmFromTemplate {
            name: "try-vz-linux".to_string(),
            template_id: "debian-arm64-apple-vz-linux-kernel-raw".to_string(),
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains(r#""type":"create_vm_from_template""#));
        let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(request, decoded);
    }

    #[test]
    fn inspect_boot_media_request_round_trips_as_json() {
        let request = BridgeVmRequest::InspectBootMedia {
            name: "ubuntu".to_string(),
        };
        let json = serde_json::to_string(&request).unwrap();
        let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(request, decoded);
    }

    #[test]
    fn import_boot_media_request_round_trips_as_json() {
        let request = BridgeVmRequest::ImportBootMedia {
            name: "ubuntu".to_string(),
            source: PathBuf::from("ubuntu.iso"),
            kind: Some(BootMediaKind::InstallerImage),
        };
        let json = serde_json::to_string(&request).unwrap();
        let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(request, decoded);
    }

    #[test]
    fn inspect_boot_media_status_request_round_trips_as_json() {
        let request = BridgeVmRequest::InspectBootMediaStatus {
            name: "ubuntu".to_string(),
        };
        let json = serde_json::to_string(&request).unwrap();
        let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(request, decoded);
    }

    #[test]
    fn verify_boot_media_request_round_trips_as_json() {
        let request = BridgeVmRequest::VerifyBootMedia {
            name: "ubuntu".to_string(),
            expected_sha256: "0".repeat(64),
            kind: Some(BootMediaKind::InstallerImage),
        };
        let json = serde_json::to_string(&request).unwrap();
        let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(request, decoded);
    }

    #[test]
    fn plan_boot_media_download_request_round_trips_as_json() {
        let request = BridgeVmRequest::PlanBootMediaDownload {
            name: "ubuntu".to_string(),
            url: "https://example.invalid/ubuntu.iso".to_string(),
            expected_sha256: Some("0".repeat(64)),
            kind: Some(BootMediaKind::InstallerImage),
        };
        let json = serde_json::to_string(&request).unwrap();
        let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(request, decoded);
    }

    #[test]
    fn download_boot_media_request_round_trips_as_json() {
        let request = BridgeVmRequest::DownloadBootMedia {
            name: "ubuntu".to_string(),
            kind: Some(BootMediaKind::InstallerImage),
        };
        let json = serde_json::to_string(&request).unwrap();
        let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(request, decoded);
    }

    #[test]
    fn port_requests_round_trip_as_json() {
        for request in [
            BridgeVmRequest::ListPorts {
                name: "legacy".to_string(),
            },
            BridgeVmRequest::AddPort {
                name: "legacy".to_string(),
                host: 3000,
                guest: 3000,
            },
            BridgeVmRequest::RemovePort {
                name: "legacy".to_string(),
                host: 3000,
                guest: 3000,
            },
            BridgeVmRequest::OpenPort {
                name: "legacy".to_string(),
                guest: 3000,
                scheme: Some("http".to_string()),
            },
        ] {
            let json = serde_json::to_string(&request).unwrap();
            let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
            assert_eq!(request, decoded);
        }
    }

    #[test]
    fn share_requests_round_trip_as_json() {
        for request in [
            BridgeVmRequest::ListShares {
                name: "dev".to_string(),
            },
            BridgeVmRequest::AddShare {
                name: "dev".to_string(),
                share: "workspace".to_string(),
                host_path: "/Users/me/project".to_string(),
                read_only: true,
                host_path_token: Some("share-token-workspace".to_string()),
            },
            BridgeVmRequest::RemoveShare {
                name: "dev".to_string(),
                share: "workspace".to_string(),
            },
        ] {
            let json = serde_json::to_string(&request).unwrap();
            let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
            assert_eq!(request, decoded);
        }
    }

    #[test]
    fn run_backend_request_round_trips_as_json() {
        let request = BridgeVmRequest::RunBackend {
            name: "legacy".to_string(),
            spawn: false,
        };
        let json = serde_json::to_string(&request).unwrap();
        let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(request, decoded);
    }

    #[test]
    fn suspend_backend_request_round_trips_as_json() {
        let request = BridgeVmRequest::SuspendBackend {
            name: "legacy".to_string(),
        };
        let json = serde_json::to_string(&request).unwrap();
        assert_eq!(json, r#"{"type":"suspend_backend","name":"legacy"}"#);
        let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(request, decoded);
    }

    #[test]
    fn resume_backend_request_round_trips_as_json() {
        let request = BridgeVmRequest::ResumeBackend {
            name: "legacy".to_string(),
        };
        let json = serde_json::to_string(&request).unwrap();
        assert_eq!(json, r#"{"type":"resume_backend","name":"legacy"}"#);
        let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(request, decoded);
    }

    #[test]
    fn reapply_runtime_resources_request_round_trips_as_json() {
        let request = BridgeVmRequest::ReapplyRuntimeResources {
            name: "fast-linux".to_string(),
            visibility: RuntimeResourceVisibility::Background,
        };
        let json = serde_json::to_string(&request).unwrap();
        assert_eq!(
            json,
            r#"{"type":"reapply_runtime_resources","name":"fast-linux","visibility":"background"}"#
        );
        let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(request, decoded);
    }

    #[test]
    fn runtime_control_request_and_response_round_trip_as_json() {
        let request = BridgeVmRequest::RuntimeControl {
            name: "fast-linux".to_string(),
            command: "status".to_string(),
        };
        let json = serde_json::to_string(&request).unwrap();
        assert_eq!(
            json,
            r#"{"type":"runtime_control","name":"fast-linux","command":"status"}"#
        );
        let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(request, decoded);

        let response = BridgeVmResponse::RuntimeControl {
            control: RuntimeControlCommandRecord {
                vm: "fast-linux".to_string(),
                kind: "apple-vz-display".to_string(),
                socket_path: PathBuf::from("/tmp/bvm-vz-test.sock"),
                command: "status".to_string(),
                response: serde_json::json!({
                    "ok": true,
                    "state": "running",
                    "display": {"width": 1024, "height": 768}
                }),
            },
        };
        let json = serde_json::to_string(&response).unwrap();
        let decoded: BridgeVmResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(response, decoded);
    }

    #[test]
    fn prepare_disk_request_round_trips_as_json() {
        let request = BridgeVmRequest::PrepareDisk {
            name: "legacy".to_string(),
        };
        let json = serde_json::to_string(&request).unwrap();
        let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(request, decoded);
    }

    #[test]
    fn create_disk_request_round_trips_as_json() {
        let request = BridgeVmRequest::CreateDisk {
            name: "legacy".to_string(),
        };
        let json = serde_json::to_string(&request).unwrap();
        let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(request, decoded);
    }

    #[test]
    fn inspect_disk_request_round_trips_as_json() {
        let request = BridgeVmRequest::InspectDisk {
            name: "legacy".to_string(),
        };
        let json = serde_json::to_string(&request).unwrap();
        let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(request, decoded);
    }

    #[test]
    fn verify_disk_request_round_trips_as_json() {
        let request = BridgeVmRequest::VerifyDisk {
            name: "legacy".to_string(),
        };
        let json = serde_json::to_string(&request).unwrap();
        let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(request, decoded);
    }

    #[test]
    fn compact_disk_request_round_trips_as_json() {
        let request = BridgeVmRequest::CompactDisk {
            name: "legacy".to_string(),
        };
        let json = serde_json::to_string(&request).unwrap();
        let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(request, decoded);
    }

    #[test]
    fn view_logs_request_round_trips_as_json() {
        let request = BridgeVmRequest::ViewLogs {
            name: "legacy".to_string(),
            kind: VmLogKind::Qemu,
            max_bytes: Some(4096),
        };
        let json = serde_json::to_string(&request).unwrap();
        let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(request, decoded);
    }

    #[test]
    fn ssh_plan_request_round_trips_as_json() {
        let request = BridgeVmRequest::SshPlan {
            name: "dev".to_string(),
            user: Some("ubuntu".to_string()),
        };
        let json = serde_json::to_string(&request).unwrap();
        let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(request, decoded);
    }

    #[test]
    fn stop_backend_request_round_trips_as_json() {
        let request = BridgeVmRequest::StopBackend {
            name: "legacy".to_string(),
        };
        let json = serde_json::to_string(&request).unwrap();
        let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(request, decoded);
    }

    #[test]
    fn qmp_control_requests_round_trip_as_json() {
        for request in [
            BridgeVmRequest::QmpStop {
                name: "legacy".to_string(),
            },
            BridgeVmRequest::QmpCont {
                name: "legacy".to_string(),
            },
        ] {
            let json = serde_json::to_string(&request).unwrap();
            let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
            assert_eq!(request, decoded);
        }
    }

    #[test]
    fn lifecycle_plan_request_round_trips_as_json() {
        let request = BridgeVmRequest::LifecyclePlan {
            name: "legacy".to_string(),
            action: LifecycleAction::Suspend,
        };
        let json = serde_json::to_string(&request).unwrap();
        let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(request, decoded);
    }

    #[test]
    fn lifecycle_plan_response_round_trips_as_json() {
        let response = BridgeVmResponse::LifecyclePlan {
            plan: LifecyclePlanRecord {
                vm: "legacy".to_string(),
                action: LifecycleAction::Resume,
                current_state: VmRuntimeState::Suspended,
                target_state: VmRuntimeState::Running,
                backend: "qemu-qmp".to_string(),
                metadata_only: true,
                executable: true,
                qmp_command: Some("cont".to_string()),
                socket_path: Some(PathBuf::from("/tmp/bridgevm/legacy/metadata/qmp.sock")),
                socket_available: true,
                qmp_supervisor: None,
                blockers: Vec::new(),
                notes: vec!["metadata-only lifecycle plan; no backend command was sent".to_string()],
            },
        };
        let json = serde_json::to_string(&response).unwrap();
        let decoded: BridgeVmResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(response, decoded);
    }

    #[test]
    fn restart_vm_request_round_trips_as_json() {
        let request = BridgeVmRequest::RestartVm {
            name: "legacy".to_string(),
        };
        let json = serde_json::to_string(&request).unwrap();
        let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(request, decoded);
    }

    #[test]
    fn restore_snapshot_request_round_trips_as_json() {
        let request = BridgeVmRequest::RestoreSnapshot {
            vm: "dev".to_string(),
            name: "before-upgrade".to_string(),
        };
        let json = serde_json::to_string(&request).unwrap();
        let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(request, decoded);
    }

    #[test]
    fn create_snapshot_disk_request_round_trips_as_json() {
        let request = BridgeVmRequest::CreateSnapshotDisk {
            vm: "dev".to_string(),
            name: "before-upgrade".to_string(),
        };
        let json = serde_json::to_string(&request).unwrap();
        let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(request, decoded);
    }

    #[test]
    fn application_consistent_snapshot_request_round_trips_as_json() {
        let request = BridgeVmRequest::CreateSnapshot {
            vm: "dev".to_string(),
            name: "before-upgrade".to_string(),
            kind: SnapshotKind::ApplicationConsistent,
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("application-consistent"));
        let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(request, decoded);
    }

    #[test]
    fn application_consistent_snapshot_response_round_trips_as_json() {
        let response = BridgeVmResponse::Snapshot {
            snapshot: SnapshotMetadata {
                name: "before-upgrade".to_string(),
                kind: SnapshotKind::ApplicationConsistent,
                created_at_unix: 1,
                vm_state: VmRuntimeState::Running,
            },
            disk: None,
            application_consistent_preflight: Some(
                ApplicationConsistentSnapshotPreflightMetadata {
                    snapshot: "before-upgrade".to_string(),
                    connected: true,
                    required_capabilities: vec!["fs-freeze".to_string(), "fs-thaw".to_string()],
                    available_capabilities: vec![
                        "heartbeat".to_string(),
                        "fs-freeze".to_string(),
                        "fs-thaw".to_string(),
                    ],
                    missing_capabilities: Vec::new(),
                    ready: true,
                    planned_freeze_semantics: "daemon-owned guest-tools fs-freeze request"
                        .to_string(),
                    planned_thaw_semantics: "daemon-owned guest-tools fs-thaw request".to_string(),
                    runtime_updated_at_unix: Some(2),
                    prepared_at_unix: 3,
                },
            ),
        };
        let json = serde_json::to_string(&response).unwrap();
        let decoded: BridgeVmResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(response, decoded);
    }

    #[test]
    fn snapshot_preflight_status_request_and_response_round_trip_as_json() {
        let request = BridgeVmRequest::SnapshotPreflightStatus {
            name: "dev".to_string(),
            consistency: SnapshotConsistency::ApplicationConsistent,
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("snapshot_preflight_status"));
        assert!(json.contains("application-consistent"));
        let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(request, decoded);

        let response = BridgeVmResponse::SnapshotPreflightStatus {
            preflight: SnapshotPreflightStatusRecord {
                vm: "dev".to_string(),
                consistency: SnapshotConsistency::ApplicationConsistent,
                backend_freeze_thaw_supported: false,
                guest_tools_connected: true,
                capabilities: vec!["fs-freeze".to_string(), "fs-thaw".to_string()],
                ready: false,
                blockers: vec![SnapshotPreflightBlockerRecord {
                    code: "backend-freeze-thaw-unavailable".to_string(),
                    message: "Freeze/thaw orchestration requires the bridgevmd-owned running backend; this offline preflight cannot drive the guest agent.".to_string(),
                    path: None,
                }],
                checked_at_unix: 1,
            },
        };
        let json = serde_json::to_string(&response).unwrap();
        let decoded: BridgeVmResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(response, decoded);
    }

    #[test]
    fn application_consistent_snapshot_execution_request_and_response_round_trip_as_json() {
        let request = BridgeVmRequest::ExecuteApplicationConsistentSnapshot {
            vm: "dev".to_string(),
            name: "before-upgrade".to_string(),
            freeze_timeout_millis: Some(5_000),
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("execute_application_consistent_snapshot"));
        let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(request, decoded);

        let response = BridgeVmResponse::ApplicationConsistentSnapshotExecution {
            execution: ApplicationConsistentSnapshotExecutionRecord {
                vm: "dev".to_string(),
                snapshot: "before-upgrade".to_string(),
                freeze_request_id: "freeze-1".to_string(),
                thaw_request_id: "thaw-1".to_string(),
                pending_commands_after_freeze: 1,
                pending_commands_after_thaw: 2,
                snapshot_created_at_unix: 42,
                freeze_result: ApplicationConsistentSnapshotCommandResultRecord {
                    request_id: "freeze-1".to_string(),
                    capability: Some("fs-freeze".to_string()),
                    ok: true,
                    error_code: None,
                    message: Some("freeze scaffold acknowledged".to_string()),
                    completed_at_unix: 40,
                },
                thaw_result: ApplicationConsistentSnapshotCommandResultRecord {
                    request_id: "thaw-1".to_string(),
                    capability: Some("fs-thaw".to_string()),
                    ok: true,
                    error_code: None,
                    message: Some("thaw scaffold acknowledged".to_string()),
                    completed_at_unix: 41,
                },
                preflight_ready: true,
                note: "scaffold boundary".to_string(),
            },
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("application_consistent_snapshot_execution"));
        let decoded: BridgeVmResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(response, decoded);
    }

    #[test]
    fn snapshot_chain_request_round_trips_as_json() {
        let request = BridgeVmRequest::SnapshotChain {
            vm: "dev".to_string(),
        };
        let json = serde_json::to_string(&request).unwrap();
        let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(request, decoded);
    }

    #[test]
    fn export_vm_request_round_trips_as_json() {
        let request = BridgeVmRequest::ExportVm {
            name: "dev".to_string(),
            output: PathBuf::from("dev.vmbridge"),
        };
        let json = serde_json::to_string(&request).unwrap();
        let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(request, decoded);
    }

    #[test]
    fn import_vm_request_round_trips_as_json() {
        let request = BridgeVmRequest::ImportVm {
            input: PathBuf::from("dev.vmbridge"),
            name: Some("dev-copy".to_string()),
        };
        let json = serde_json::to_string(&request).unwrap();
        let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(request, decoded);
    }

    #[test]
    fn clone_vm_request_round_trips_as_json() {
        let request = BridgeVmRequest::CloneVm {
            name: "dev".to_string(),
            new_name: "dev-copy".to_string(),
            linked: false,
        };
        let json = serde_json::to_string(&request).unwrap();
        assert_eq!(
            json,
            r#"{"type":"clone_vm","name":"dev","new_name":"dev-copy"}"#
        );
        let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(request, decoded);
    }

    #[test]
    fn clone_vm_request_defaults_missing_linked_to_false() {
        let decoded: BridgeVmRequest =
            serde_json::from_str(r#"{"type":"clone_vm","name":"dev","new_name":"dev-copy"}"#)
                .unwrap();
        assert_eq!(
            decoded,
            BridgeVmRequest::CloneVm {
                name: "dev".to_string(),
                new_name: "dev-copy".to_string(),
                linked: false,
            }
        );
    }

    #[test]
    fn linked_clone_vm_request_round_trips_as_json() {
        let request = BridgeVmRequest::CloneVm {
            name: "dev".to_string(),
            new_name: "dev-copy".to_string(),
            linked: true,
        };
        let json = serde_json::to_string(&request).unwrap();
        assert_eq!(
            json,
            r#"{"type":"clone_vm","name":"dev","new_name":"dev-copy","linked":true}"#
        );
        let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(request, decoded);
    }

    #[test]
    fn repair_metadata_request_round_trips_as_json() {
        let request = BridgeVmRequest::RepairMetadata {
            name: "dev".to_string(),
        };
        let json = serde_json::to_string(&request).unwrap();
        assert_eq!(json, r#"{"type":"repair_metadata","name":"dev"}"#);
        let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(request, decoded);
    }

    #[test]
    fn migrate_manifest_request_round_trips_as_json() {
        let request = BridgeVmRequest::MigrateManifest {
            name: "dev".to_string(),
            dry_run: true,
        };
        let json = serde_json::to_string(&request).unwrap();
        assert_eq!(
            json,
            r#"{"type":"migrate_manifest","name":"dev","dry_run":true}"#
        );
        let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(request, decoded);
    }

    #[test]
    fn plan_network_request_round_trips_as_json() {
        let request = BridgeVmRequest::PlanNetwork {
            name: "dev".to_string(),
        };
        let json = serde_json::to_string(&request).unwrap();
        assert_eq!(json, r#"{"type":"plan_network","name":"dev"}"#);
        let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(request, decoded);
    }

    #[test]
    fn repair_metadata_response_round_trips_as_json() {
        let response = BridgeVmResponse::MetadataRepaired {
            repair: VmMetadataRepairMetadata {
                vm: "dev".to_string(),
                bundle: PathBuf::from("/tmp/dev.vmbridge"),
                repaired: true,
                actions: vec![bridgevm_storage::MetadataRepairAction {
                    action: "repaired".to_string(),
                    path: PathBuf::from("/tmp/dev.vmbridge/metadata/runtime.json"),
                    detail: "wrote runtime metadata".to_string(),
                }],
                repaired_at_unix: 42,
            },
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("metadata_repaired"));
        let decoded: BridgeVmResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(response, decoded);
    }

    #[test]
    fn migrate_manifest_response_round_trips_as_json() {
        let response = BridgeVmResponse::ManifestMigrated {
            migration: VmManifestMigrationMetadata {
                vm: "dev".to_string(),
                bundle: PathBuf::from("/tmp/dev.vmbridge"),
                manifest_path: PathBuf::from("/tmp/dev.vmbridge/manifest.yaml"),
                from_schema: "bridgevm.io/v1".to_string(),
                to_schema: "bridgevm.io/v1".to_string(),
                dry_run: false,
                migrated: false,
                backup_path: Some(PathBuf::from(
                    "/tmp/dev.vmbridge/metadata/manifest-before-migration.yaml",
                )),
                receipt_path: Some(PathBuf::from(
                    "/tmp/dev.vmbridge/metadata/manifest-migration.json",
                )),
                actions: vec![bridgevm_storage::MetadataRepairAction {
                    action: "validated".to_string(),
                    path: PathBuf::from("/tmp/dev.vmbridge/manifest.yaml"),
                    detail: "manifest already uses the current schema".to_string(),
                }],
                migrated_at_unix: 42,
            },
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("manifest_migrated"));
        let decoded: BridgeVmResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(response, decoded);
    }

    #[test]
    fn network_planned_response_round_trips_as_json() {
        let response = BridgeVmResponse::NetworkPlanned {
            plan: NetworkPlanRecord {
                vm: "dev".to_string(),
                backend: "qemu".to_string(),
                mode: "bridged".to_string(),
                hostname: "dev.bridgevm.local".to_string(),
                dry_run: true,
                executable: false,
                port_forwards: Vec::new(),
                capabilities: Some(NetworkCapabilitiesRecord {
                    guest_outbound: true,
                    host_to_guest: true,
                    guest_to_host: true,
                    host_visible_hostname: true,
                    supports_port_forwarding: false,
                    requires_privileged_helper: true,
                }),
                blockers: vec![NetworkPlanBlockerRecord {
                    code: "qemu-bridged-requires-privilege".to_string(),
                    message: "Compatibility Mode QEMU bridged networking uses vmnet-bridged, which requires the qemu process to run as root or carry the com.apple.vm.networking entitlement"
                        .to_string(),
                }],
                notes: vec!["dry-run network plan".to_string()],
            },
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("network_planned"));
        let decoded: BridgeVmResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(response, decoded);
    }

    #[test]
    fn diagnostic_bundle_request_round_trips_as_json() {
        let request = BridgeVmRequest::CreateDiagnosticBundle {
            name: "dev".to_string(),
            output: PathBuf::from("diagnostics"),
        };
        let json = serde_json::to_string(&request).unwrap();
        let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(request, decoded);
    }

    #[test]
    fn performance_baseline_request_round_trips_as_json() {
        let request = BridgeVmRequest::CreatePerformanceBaseline {
            name: "dev".to_string(),
            output: PathBuf::from("performance"),
        };
        let json = serde_json::to_string(&request).unwrap();
        let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(request, decoded);
    }

    #[test]
    fn performance_sample_request_round_trips_as_json() {
        let request = BridgeVmRequest::CreatePerformanceSample {
            name: "dev".to_string(),
            output: PathBuf::from("performance"),
            artifact_bytes: Some(4096),
            iterations: Some(3),
            sync: true,
        };
        let json = serde_json::to_string(&request).unwrap();
        let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(request, decoded);
    }

    #[test]
    fn performance_sample_response_round_trips_as_json() {
        let response = BridgeVmResponse::PerformanceSample {
            sample: PerformanceSampleMetadata {
                vm: "dev".to_string(),
                source: PathBuf::from("/tmp/dev.vmbridge"),
                output: PathBuf::from("/tmp/performance"),
                artifact: PathBuf::from("/tmp/performance/performance-sample.json"),
                probe: PathBuf::from("/tmp/performance/probe-1.bin"),
                probes: vec![
                    PathBuf::from("/tmp/performance/probe-1.bin"),
                    PathBuf::from("/tmp/performance/probe-2.bin"),
                ],
                artifact_bytes: 4096,
                iterations: 2,
                sync: true,
                iteration_results: vec![
                    PerformanceSampleIterationRecord {
                        iteration: 1,
                        probe: PathBuf::from("/tmp/performance/probe-1.bin"),
                        bytes: 4096,
                        write_latency_microseconds: 120,
                        sync: true,
                    },
                    PerformanceSampleIterationRecord {
                        iteration: 2,
                        probe: PathBuf::from("/tmp/performance/probe-2.bin"),
                        bytes: 4096,
                        write_latency_microseconds: 110,
                        sync: true,
                    },
                ],
                created_at_unix: 42,
                state: VmRuntimeMetadata {
                    state: VmRuntimeState::Running,
                    updated_at_unix: 40,
                },
                runner: None,
                guest_tools: GuestToolsStatusRecord {
                    vm: "dev".to_string(),
                    tools: "bridgevm-agent".to_string(),
                    token_created_at_unix: 39,
                    capabilities: Vec::new(),
                    approved_shared_folders: Vec::new(),
                    runtime: None,
                },
                metrics: Some(GuestToolsMetricsMetadata {
                    cpu_percent: 7,
                    memory_used_mib: 512,
                    updated_at_unix: 41,
                }),
                measurements: vec![PerformanceMeasurementRecord {
                    name: "sample_write_latency_microseconds".to_string(),
                    value: 115,
                    unit: "microseconds".to_string(),
                    source: "bridgevm.performance_sample".to_string(),
                    metadata_only: false,
                }],
                notes: vec!["metadata-safe performance sample".to_string()],
            },
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("performance_sample"));
        let decoded: BridgeVmResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(response, decoded);
    }

    #[test]
    fn readiness_report_request_and_response_round_trips_as_json() {
        let request = BridgeVmRequest::ReadinessReport {
            name: "ubuntu".to_string(),
            live_evidence: None,
            record_live_evidence: false,
            clear_live_evidence: false,
        };
        let json = serde_json::to_string(&request).unwrap();
        assert_eq!(json, r#"{"type":"readiness_report","name":"ubuntu"}"#);
        let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(request, decoded);

        let request = BridgeVmRequest::ReadinessReport {
            name: "ubuntu".to_string(),
            live_evidence: Some(PathBuf::from("/tmp/live-evidence")),
            record_live_evidence: true,
            clear_live_evidence: false,
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains(r#""record_live_evidence":true"#));
        let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(request, decoded);

        let request = BridgeVmRequest::ReadinessReport {
            name: "ubuntu".to_string(),
            live_evidence: None,
            record_live_evidence: false,
            clear_live_evidence: true,
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains(r#""clear_live_evidence":true"#));
        let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(request, decoded);

        let response = BridgeVmResponse::ReadinessReport {
            report: VmReadinessReport {
                vm: "ubuntu".to_string(),
                mode: VmMode::Fast,
                state: VmRuntimeState::Stopped,
                metadata_only: true,
                live_e2e_required: true,
                live_evidence: None,
                evidence_requirements: vec![VmEvidenceRequirement {
                    kind: "live-boot".to_string(),
                    required: true,
                    proven: false,
                    note: "requires preserved opt-in serial or graphical boot progress evidence from Apple VZ or QEMU"
                        .to_string(),
                }],
                boot_media: None,
                boot_media_error: Some("missing boot metadata".to_string()),
                snapshot_chain: None,
                snapshot_chain_error: None,
                runner: None,
                pre_run_launch_readiness: Some(LaunchReadinessMetadata {
                    ready: false,
                    blockers: vec![LaunchReadinessBlockerMetadata {
                        code: "missing-primary-disk".to_string(),
                        message: "primary disk is missing".to_string(),
                        path: None,
                        capability: None,
                    }],
                }),
                qmp_supervisor: None,
                runner_error: None,
                blockers: vec!["boot-media-status-error:missing boot metadata".to_string()],
                notes: vec![
                    "metadata-only preflight report; no VM, QEMU, Apple VZ, console, or guest workload was started".to_string(),
                    "live E2E boot, console, and guest-tools effects still require the explicit opt-in live smoke evidence path".to_string(),
                ],
            },
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("readiness_report"));
        let decoded: BridgeVmResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(response, decoded);
    }

    #[test]
    fn guest_tools_requests_round_trip_as_json() {
        let status = BridgeVmRequest::GuestToolsStatus {
            name: "dev".to_string(),
        };
        let json = serde_json::to_string(&status).unwrap();
        let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(status, decoded);

        let token = BridgeVmRequest::GuestToolsToken {
            name: "dev".to_string(),
        };
        let json = serde_json::to_string(&token).unwrap();
        let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(token, decoded);

        let accept = BridgeVmRequest::GuestToolsAcceptHello {
            name: "dev".to_string(),
            envelope: AgentEnvelope::new(valid_guest_hello("token-1", &["clipboard"])),
        };
        let json = serde_json::to_string(&accept).unwrap();
        let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(accept, decoded);

        let send = BridgeVmRequest::GuestToolsSendCommand {
            name: "dev".to_string(),
            envelope: AgentEnvelope::with_request_id(
                AgentMessage::SetClipboard {
                    text: "hello".to_string(),
                },
                "clipboard-1",
            ),
        };
        let json = serde_json::to_string(&send).unwrap();
        let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(send, decoded);

        let mount_approved_share = BridgeVmRequest::GuestToolsMountApprovedShare {
            name: "dev".to_string(),
            share: "workspace".to_string(),
            request_id: Some("mount-workspace-1".to_string()),
        };
        let json = serde_json::to_string(&mount_approved_share).unwrap();
        let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(mount_approved_share, decoded);

        let linux_command = BridgeVmRequest::GuestToolsLinuxCommand {
            name: "dev".to_string(),
            transport: GuestToolsLinuxCommandTransport::Socket,
            token_file: Some(PathBuf::from("/run/bridgevm-token.json")),
            device: None,
        };
        let json = serde_json::to_string(&linux_command).unwrap();
        let decoded: BridgeVmRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(linux_command, decoded);
    }

    #[test]
    fn handler_creates_vm_record() {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "bridgevm-api-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = VmStore::new(root);
        let manifest = VmManifest::new(
            "dev",
            VmMode::Fast,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "arm64".to_string(),
            },
            "80GiB",
        );

        let response = handle_request(&store, BridgeVmRequest::CreateVm { manifest })
            .into_result()
            .unwrap();
        let BridgeVmResponse::Vm { vm } = response else {
            panic!("expected VM response");
        };
        assert_eq!(vm.name, "dev");
        assert_eq!(vm.state, "stopped");
    }

    #[test]
    fn handler_repairs_metadata() {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "bridgevm-api-metadata-repair-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = VmStore::new(root);
        let manifest = VmManifest::new(
            "dev",
            VmMode::Fast,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "arm64".to_string(),
            },
            "80GiB",
        );
        handle_request(&store, BridgeVmRequest::CreateVm { manifest })
            .into_result()
            .unwrap();

        let response = handle_request(
            &store,
            BridgeVmRequest::RepairMetadata {
                name: "dev".to_string(),
            },
        )
        .into_result()
        .unwrap();

        let BridgeVmResponse::MetadataRepaired { repair } = response else {
            panic!("expected metadata repair response");
        };
        assert_eq!(repair.vm, "dev");
        assert_eq!(repair.bundle, store.bundle_path("dev"));
        assert!(repair.repaired);
        assert!(repair
            .actions
            .iter()
            .any(|action| action.path.ends_with("metadata/primary-disk.json")));
    }

    #[test]
    fn handler_migrates_manifest_metadata_boundary() {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "bridgevm-api-manifest-migration-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = VmStore::new(root);
        let manifest = VmManifest::new(
            "dev",
            VmMode::Fast,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "arm64".to_string(),
            },
            "80GiB",
        );
        handle_request(&store, BridgeVmRequest::CreateVm { manifest })
            .into_result()
            .unwrap();

        let response = handle_request(
            &store,
            BridgeVmRequest::MigrateManifest {
                name: "dev".to_string(),
                dry_run: false,
            },
        )
        .into_result()
        .unwrap();

        let BridgeVmResponse::ManifestMigrated { migration } = response else {
            panic!("expected manifest migration response");
        };
        assert_eq!(migration.vm, "dev");
        assert_eq!(migration.from_schema, "bridgevm.io/v1");
        assert_eq!(migration.to_schema, "bridgevm.io/v1");
        assert!(migration.backup_path.as_ref().unwrap().exists());
        assert!(migration.receipt_path.as_ref().unwrap().exists());
    }

    #[test]
    fn handler_lists_boot_templates() {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "bridgevm-api-template-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = VmStore::new(root);

        let response = handle_request(&store, BridgeVmRequest::ListTemplates)
            .into_result()
            .unwrap();
        let BridgeVmResponse::BootTemplates { templates } = response else {
            panic!("expected boot templates response");
        };

        assert!(templates
            .iter()
            .any(|template| template.id == "ubuntu-arm64-installer"));
        let ubuntu_vz_template = templates
            .iter()
            .find(|template| template.id == "ubuntu-arm64-apple-vz-linux-kernel-raw")
            .expect("Ubuntu Apple VZ linux-kernel raw template");
        assert_eq!(ubuntu_vz_template.mode, BootMode::LinuxKernel);
        assert_eq!(
            ubuntu_vz_template.kernel_path.as_deref(),
            Some("boot/vmlinuz")
        );
        assert_eq!(
            ubuntu_vz_template.initrd_path.as_deref(),
            Some("boot/initrd")
        );
        assert_eq!(
            ubuntu_vz_template.kernel_command_line.as_deref(),
            Some("console=hvc0 root=/dev/vda2 rw systemd.unit=graphical.target")
        );
        let ubuntu_storage = ubuntu_vz_template
            .storage
            .as_ref()
            .expect("Ubuntu storage defaults");
        assert_eq!(ubuntu_storage.primary.path, "disks/root.raw");
        assert_eq!(ubuntu_storage.primary.format, "raw");
        assert_eq!(ubuntu_storage.primary.size, "32GiB");
        let vz_template = templates
            .iter()
            .find(|template| template.id == "debian-arm64-apple-vz-linux-kernel-raw")
            .expect("Apple VZ linux-kernel raw template");
        assert_eq!(vz_template.mode, BootMode::LinuxKernel);
        assert_eq!(vz_template.kernel_path.as_deref(), Some("boot/vmlinuz"));
        assert_eq!(vz_template.initrd_path.as_deref(), Some("boot/initrd"));
        assert_eq!(
            vz_template.kernel_command_line.as_deref(),
            Some("console=hvc0 priority=low")
        );
        let storage = vz_template.storage.as_ref().expect("storage defaults");
        assert_eq!(storage.primary.path, "disks/root.raw");
        assert_eq!(storage.primary.format, "raw");
        assert_eq!(storage.primary.size, "64MiB");
        assert!(templates
            .iter()
            .any(|template| template.id == "macos-restore"));
    }

    #[test]
    fn handler_create_preserves_debian_apple_vz_linux_kernel_raw_template_manifest() {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "bridgevm-api-vz-template-create-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = VmStore::new(root);
        handle_request(
            &store,
            BridgeVmRequest::CreateVmFromTemplate {
                name: "vz-template".to_string(),
                template_id: "debian-arm64-apple-vz-linux-kernel-raw".to_string(),
            },
        )
        .into_result()
        .unwrap();

        let (_, stored) = store.get_vm("vz-template").unwrap();
        assert_eq!(stored.mode, VmMode::Fast);
        assert_eq!(stored.guest.os, "debian");
        assert_eq!(stored.guest.arch, "arm64");
        assert_eq!(stored.storage.primary.path, "disks/root.raw");
        assert_eq!(stored.storage.primary.format, "raw");
        assert_eq!(stored.storage.primary.size, "64MiB");
        let boot = stored.boot.expect("boot");
        assert_eq!(boot.mode, BootMode::LinuxKernel);
        assert_eq!(boot.kernel_path.as_deref(), Some("boot/vmlinuz"));
        assert_eq!(boot.initrd_path.as_deref(), Some("boot/initrd"));
        assert_eq!(
            boot.kernel_command_line.as_deref(),
            Some("console=hvc0 priority=low")
        );
    }

    #[test]
    fn handler_create_preserves_ubuntu_apple_vz_linux_kernel_raw_template_manifest() {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "bridgevm-api-ubuntu-vz-template-create-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = VmStore::new(root);
        handle_request(
            &store,
            BridgeVmRequest::CreateVmFromTemplate {
                name: "ubuntu-vz-template".to_string(),
                template_id: "ubuntu-arm64-apple-vz-linux-kernel-raw".to_string(),
            },
        )
        .into_result()
        .unwrap();

        let (_, stored) = store.get_vm("ubuntu-vz-template").unwrap();
        assert_eq!(stored.mode, VmMode::Fast);
        assert_eq!(stored.guest.os, "ubuntu");
        assert_eq!(stored.guest.arch, "arm64");
        assert_eq!(stored.storage.primary.path, "disks/root.raw");
        assert_eq!(stored.storage.primary.format, "raw");
        assert_eq!(stored.storage.primary.size, "32GiB");
        let boot = stored.boot.expect("boot");
        assert_eq!(boot.mode, BootMode::LinuxKernel);
        assert_eq!(boot.kernel_path.as_deref(), Some("boot/vmlinuz"));
        assert_eq!(boot.initrd_path.as_deref(), Some("boot/initrd"));
        assert_eq!(
            boot.kernel_command_line.as_deref(),
            Some("console=hvc0 root=/dev/vda2 rw systemd.unit=graphical.target")
        );
    }

    #[test]
    fn handler_reports_guest_tools_policy_from_manifest() {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "bridgevm-api-guest-tools-status-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = VmStore::new(root);
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
        manifest.integration.clipboard = false;
        manifest.integration.shared_folders = false;
        handle_request(&store, BridgeVmRequest::CreateVm { manifest })
            .into_result()
            .unwrap();

        let response = handle_request(
            &store,
            BridgeVmRequest::GuestToolsStatus {
                name: "dev".to_string(),
            },
        )
        .into_result()
        .unwrap();
        let BridgeVmResponse::GuestToolsStatus { status } = response else {
            panic!("expected guest tools status response");
        };

        assert_eq!(status.vm, "dev");
        assert_eq!(status.tools, "required");
        assert!(status.token_created_at_unix > 0);
        assert!(status
            .capabilities
            .iter()
            .any(|capability| capability.name == "heartbeat"));
        assert!(status
            .capabilities
            .iter()
            .any(|capability| capability.name == "display-resize"));
        assert!(status
            .capabilities
            .iter()
            .any(|capability| capability.name == "applications"));
        assert!(status
            .capabilities
            .iter()
            .any(|capability| capability.name == "windows"));
        assert!(status
            .capabilities
            .iter()
            .any(|capability| capability.name == "agent-update"));
        assert!(!status
            .capabilities
            .iter()
            .any(|capability| capability.name == "clipboard"));
        assert!(!status
            .capabilities
            .iter()
            .any(|capability| capability.name == "shared-folders"));
        assert!(status.approved_shared_folders.is_empty());

        let response = handle_request(
            &store,
            BridgeVmRequest::GuestToolsToken {
                name: "dev".to_string(),
            },
        )
        .into_result()
        .unwrap();
        let BridgeVmResponse::GuestToolsToken { token } = response else {
            panic!("expected guest tools token response");
        };
        assert_eq!(token.vm, "dev");
        assert_eq!(token.token.len(), 64);
        assert_eq!(token.created_at_unix, status.token_created_at_unix);
    }

    #[test]
    fn handler_reports_last_guest_tools_command_result_from_runtime() {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "bridgevm-api-guest-tools-command-result-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = VmStore::new(root);
        let manifest = VmManifest::new(
            "dev",
            VmMode::Fast,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "arm64".to_string(),
            },
            "80GiB",
        );
        handle_request(&store, BridgeVmRequest::CreateVm { manifest })
            .into_result()
            .unwrap();
        store
            .write_guest_tools_runtime_metadata(
                "dev",
                &GuestToolsRuntimeMetadata {
                    connected: true,
                    guest_os: Some("linux".to_string()),
                    agent_version: Some("1.0.0".to_string()),
                    capabilities: vec!["applications".to_string()],
                    last_heartbeat_at_unix: Some(1),
                    guest_ip_addresses: Vec::new(),
                    shared_folders: Vec::new(),
                    metrics: None,
                    last_command_result: Some(bridgevm_storage::GuestToolsCommandResultMetadata {
                        request_id: "apps-1".to_string(),
                        capability: Some("applications".to_string()),
                        ok: false,
                        error_code: Some("not-ready".to_string()),
                        message: Some("application inventory is not ready".to_string()),
                        result: Some(serde_json::json!({
                            "applications": [
                                {
                                    "id": "terminal",
                                    "name": "Terminal"
                                }
                            ]
                        })),
                        metadata: Some(serde_json::json!({
                            "scan_duration_ms": 12
                        })),
                        completed_at_unix: 42,
                    }),
                    agent_update: None,
                    clipboard: None,
                    updated_at_unix: 43,
                },
            )
            .unwrap();

        let response = handle_request(
            &store,
            BridgeVmRequest::GuestToolsStatus {
                name: "dev".to_string(),
            },
        )
        .into_result()
        .unwrap();
        let BridgeVmResponse::GuestToolsStatus { status } = response else {
            panic!("expected guest tools status response");
        };
        let result = status
            .runtime
            .expect("runtime metadata")
            .last_command_result
            .expect("last command result");

        assert_eq!(result.request_id, "apps-1");
        assert_eq!(result.capability.as_deref(), Some("applications"));
        assert!(!result.ok);
        assert_eq!(result.error_code.as_deref(), Some("not-ready"));
        assert_eq!(
            result.message.as_deref(),
            Some("application inventory is not ready")
        );
        assert_eq!(
            result.result,
            Some(serde_json::json!({
                "applications": [
                    {
                        "id": "terminal",
                        "name": "Terminal"
                    }
                ]
            }))
        );
        assert_eq!(
            result.metadata,
            Some(serde_json::json!({
                "scan_duration_ms": 12
            }))
        );
        assert_eq!(result.completed_at_unix, 42);
    }

    #[test]
    fn handler_reports_guest_tools_agent_update_from_runtime() {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "bridgevm-api-guest-tools-agent-update-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = VmStore::new(root);
        let manifest = VmManifest::new(
            "dev",
            VmMode::Fast,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "arm64".to_string(),
            },
            "80GiB",
        );
        handle_request(&store, BridgeVmRequest::CreateVm { manifest })
            .into_result()
            .unwrap();
        store
            .write_guest_tools_runtime_metadata(
                "dev",
                &GuestToolsRuntimeMetadata {
                    connected: true,
                    guest_os: Some("linux".to_string()),
                    agent_version: Some("1.0.0".to_string()),
                    capabilities: vec!["agent-update".to_string()],
                    last_heartbeat_at_unix: Some(1),
                    guest_ip_addresses: Vec::new(),
                    shared_folders: Vec::new(),
                    metrics: None,
                    last_command_result: None,
                    agent_update: Some(bridgevm_storage::GuestToolsAgentUpdateMetadata {
                        current_version: "1.0.0".to_string(),
                        available_version: "1.1.0".to_string(),
                        download_url: Some("https://updates.example/bridgevm-tools".to_string()),
                        signature: Some("signed".to_string()),
                        observed_at_unix: 42,
                    }),
                    clipboard: None,
                    updated_at_unix: 43,
                },
            )
            .unwrap();

        let response = handle_request(
            &store,
            BridgeVmRequest::GuestToolsStatus {
                name: "dev".to_string(),
            },
        )
        .into_result()
        .unwrap();
        let BridgeVmResponse::GuestToolsStatus { status } = response else {
            panic!("expected guest tools status response");
        };
        let update = status
            .runtime
            .expect("runtime metadata")
            .agent_update
            .expect("agent update metadata");

        assert_eq!(update.current_version, "1.0.0");
        assert_eq!(update.available_version, "1.1.0");
        assert_eq!(
            update.download_url.as_deref(),
            Some("https://updates.example/bridgevm-tools")
        );
        assert_eq!(update.signature.as_deref(), Some("signed"));
        assert_eq!(update.observed_at_unix, 42);
    }

    #[test]
    fn handler_reports_approved_shared_folders_from_manifest() {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "bridgevm-api-approved-shares-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = VmStore::new(root);
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
        manifest.shared_folders = vec![
            SharedFolder {
                name: "workspace".to_string(),
                host_path: "/Users/me/project".to_string(),
                read_only: false,
                host_path_token: Some("share-token-workspace".to_string()),
            },
            SharedFolder {
                name: "downloads".to_string(),
                host_path: "/Users/me/Downloads".to_string(),
                read_only: true,
                host_path_token: None,
            },
        ];
        handle_request(&store, BridgeVmRequest::CreateVm { manifest })
            .into_result()
            .unwrap();

        let response = handle_request(
            &store,
            BridgeVmRequest::GuestToolsStatus {
                name: "dev".to_string(),
            },
        )
        .into_result()
        .unwrap();
        let BridgeVmResponse::GuestToolsStatus { status } = response else {
            panic!("expected guest tools status response");
        };

        assert_eq!(status.approved_shared_folders.len(), 2);
        assert_eq!(status.approved_shared_folders[0].name, "workspace");
        assert_eq!(
            status.approved_shared_folders[0].host_path,
            "/Users/me/project"
        );
        assert_eq!(
            status.approved_shared_folders[0].host_path_token,
            "share-token-workspace"
        );
        assert!(!status.approved_shared_folders[0].read_only);
        assert_eq!(status.approved_shared_folders[0].approval, "required");
        assert_eq!(status.approved_shared_folders[1].name, "downloads");
        assert!(status.approved_shared_folders[1].read_only);
        assert!(status.approved_shared_folders[1]
            .host_path_token
            .starts_with("share-"));
    }

    #[test]
    fn handler_updates_manifest_shared_folders() {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "bridgevm-api-share-manifest-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let host_path = root.join("workspace");
        fs::create_dir_all(&host_path).unwrap();
        let host_path = fs::canonicalize(&host_path)
            .unwrap()
            .to_string_lossy()
            .to_string();
        let store = VmStore::new(root);
        let manifest = VmManifest::new(
            "dev",
            VmMode::Fast,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "arm64".to_string(),
            },
            "80GiB",
        );
        handle_request(&store, BridgeVmRequest::CreateVm { manifest })
            .into_result()
            .unwrap();

        let response = handle_request(
            &store,
            BridgeVmRequest::AddShare {
                name: "dev".to_string(),
                share: "workspace".to_string(),
                host_path: host_path.clone(),
                read_only: true,
                host_path_token: Some("share-token-workspace".to_string()),
            },
        )
        .into_result()
        .unwrap();
        let BridgeVmResponse::SharedFolders { shares } = response else {
            panic!("expected shared folders response");
        };
        assert_eq!(shares.shared_folders.len(), 1);
        assert_eq!(shares.shared_folders[0].name, "workspace");
        assert_eq!(shares.shared_folders[0].host_path, host_path);
        assert!(shares.shared_folders[0].read_only);
        assert_eq!(
            shares.shared_folders[0].host_path_token,
            "share-token-workspace"
        );

        let (_, manifest) = store.get_vm("dev").unwrap();
        assert_eq!(manifest.shared_folders.len(), 1);
        assert_eq!(manifest.shared_folders[0].name, "workspace");
        assert_eq!(manifest.shared_folders[0].host_path, host_path);

        let response = handle_request(
            &store,
            BridgeVmRequest::RemoveShare {
                name: "dev".to_string(),
                share: "workspace".to_string(),
            },
        )
        .into_result()
        .unwrap();
        let BridgeVmResponse::SharedFolders { shares } = response else {
            panic!("expected shared folders response");
        };
        assert!(shares.shared_folders.is_empty());
    }

    #[test]
    fn handler_rejects_duplicate_shared_folder_names() {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "bridgevm-api-share-duplicate-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = VmStore::new(root);
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
            host_path_token: None,
        }];
        handle_request(&store, BridgeVmRequest::CreateVm { manifest })
            .into_result()
            .unwrap();

        let error = handle_request(
            &store,
            BridgeVmRequest::AddShare {
                name: "dev".to_string(),
                share: "workspace".to_string(),
                host_path: "/Users/me/other".to_string(),
                read_only: false,
                host_path_token: None,
            },
        )
        .into_result()
        .expect_err("duplicate shared folder should fail");
        assert!(error.contains("duplicate shared folder name 'workspace'"));
    }

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

    #[test]
    fn handler_generates_manifest_compatible_linux_tools_command() {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "bridgevm-api-guest-tools-linux-command-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = VmStore::new(root);
        let mut manifest = VmManifest::new(
            "dev",
            VmMode::Compatibility,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "arm64".to_string(),
            },
            "80GiB",
        );
        manifest.integration.clipboard = false;
        manifest.integration.shared_folders = false;
        handle_request(&store, BridgeVmRequest::CreateVm { manifest })
            .into_result()
            .unwrap();
        let token = store.guest_tools_token("dev").unwrap().token;

        let response = handle_request(
            &store,
            BridgeVmRequest::GuestToolsLinuxCommand {
                name: "dev".to_string(),
                transport: GuestToolsLinuxCommandTransport::Device,
                token_file: None,
                device: None,
            },
        )
        .into_result()
        .unwrap();
        let BridgeVmResponse::GuestToolsLinuxCommand { command } = response else {
            panic!("expected guest tools linux command response");
        };

        assert_eq!(command.vm, "dev");
        assert_eq!(command.transport, GuestToolsLinuxCommandTransport::Device);
        assert_eq!(command.command[0], "bridgevm-tools-linux");
        assert!(command
            .command
            .windows(2)
            .any(|pair| pair == ["--device", "/dev/virtio-ports/org.bridgevm.guest-tools.0"]));
        assert!(command.command.windows(2).any(|pair| {
            pair[0] == "--token-file" && pair[1].ends_with("metadata/guest-tools-token.json")
        }));
        assert!(command
            .command
            .windows(2)
            .any(|pair| pair == ["--capability", "heartbeat:1"]));
        assert!(command
            .command
            .windows(2)
            .any(|pair| pair == ["--capability", "guest-ip:1"]));
        assert!(command
            .command
            .windows(2)
            .any(|pair| pair == ["--capability", "time-sync:1"]));
        assert!(command
            .command
            .windows(2)
            .any(|pair| pair == ["--capability", "guest-metrics:1"]));
        assert!(!command
            .capabilities
            .iter()
            .any(|item| item == "clipboard:1"));
        assert!(!command
            .capabilities
            .iter()
            .any(|item| item == "shared-folders:1"));
        assert!(!command.command.iter().any(|word| word == &token));

        let socket_response = handle_request(
            &store,
            BridgeVmRequest::GuestToolsLinuxCommand {
                name: "dev".to_string(),
                transport: GuestToolsLinuxCommandTransport::Socket,
                token_file: Some(PathBuf::from("/run/bridgevm-token.json")),
                device: None,
            },
        )
        .into_result()
        .unwrap();
        let BridgeVmResponse::GuestToolsLinuxCommand { command } = socket_response else {
            panic!("expected guest tools linux command response");
        };
        assert!(command.command.windows(2).any(|pair| {
            pair[0] == "--socket" && pair[1].ends_with("metadata/guest-tools.sock")
        }));
        assert!(command
            .command
            .windows(2)
            .any(|pair| pair == ["--token-file", "/run/bridgevm-token.json"]));
    }

    #[test]
    fn handler_accepts_guest_tools_hello_against_manifest_policy() {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "bridgevm-api-guest-tools-hello-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = VmStore::new(root);
        let manifest = VmManifest::new(
            "dev",
            VmMode::Fast,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "arm64".to_string(),
            },
            "80GiB",
        );
        handle_request(&store, BridgeVmRequest::CreateVm { manifest })
            .into_result()
            .unwrap();
        let token = store.guest_tools_token("dev").unwrap().token;

        let response = handle_request(
            &store,
            BridgeVmRequest::GuestToolsAcceptHello {
                name: "dev".to_string(),
                envelope: AgentEnvelope::new(valid_guest_hello(
                    &token,
                    &["clipboard", "display-resize"],
                )),
            },
        )
        .into_result()
        .unwrap();
        let BridgeVmResponse::GuestToolsSession { session } = response else {
            panic!("expected guest tools session response");
        };

        assert_eq!(session.vm, "dev");
        assert_eq!(session.guest_os, "linux");
        assert_eq!(session.agent_version.as_deref(), Some("1.0.0"));
        assert_eq!(session.capabilities.len(), 2);

        let error = handle_request(
            &store,
            BridgeVmRequest::GuestToolsAcceptHello {
                name: "dev".to_string(),
                envelope: AgentEnvelope::new(valid_guest_hello("wrong-token", &["clipboard"])),
            },
        )
        .into_result()
        .expect_err("wrong tools token should be rejected");
        assert!(error.contains("InvalidToolsToken"));
    }

    #[test]
    fn handler_inspects_template_boot_media() {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "bridgevm-api-boot-media-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = VmStore::new(root);
        let mut manifest = VmManifest::new(
            "ubuntu",
            VmMode::Fast,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "arm64".to_string(),
            },
            "80GiB",
        );
        manifest.boot =
            boot_template_by_id("ubuntu-arm64-installer").map(|template| template.as_boot());
        handle_request(&store, BridgeVmRequest::CreateVm { manifest })
            .into_result()
            .unwrap();

        let response = handle_request(
            &store,
            BridgeVmRequest::InspectBootMedia {
                name: "ubuntu".to_string(),
            },
        )
        .into_result()
        .unwrap();
        let BridgeVmResponse::BootMedia { name, boot } = response else {
            panic!("expected boot media response");
        };

        assert_eq!(name, "ubuntu");
        assert_eq!(boot.mode, BootMode::LinuxInstaller);
        let installer = boot.installer_image.expect("expected installer image");
        assert!(installer.path.ends_with("installers/ubuntu-arm64.iso"));
        assert!(!installer.exists);
    }

    #[test]
    fn boot_media_metadata_reader_rejects_oversized_json_before_decode() {
        let root = std::env::temp_dir().join(format!(
            "bridgevm-api-oversized-boot-media-metadata-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path = boot_media_import_metadata_path(&root, BootMediaKind::InstallerImage);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            vec![b'x'; MAX_BOOT_MEDIA_METADATA_BYTES as usize + 1],
        )
        .unwrap();

        let error = read_boot_media_import_metadata(&root, BootMediaKind::InstallerImage)
            .expect_err("oversized metadata must be rejected");
        assert!(error.contains("exceeds the 1048576-byte limit"));
        assert!(error.contains(&path.display().to_string()));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn evidence_text_reader_rejects_oversized_files() {
        let path = std::env::temp_dir().join(format!(
            "bridgevm-api-oversized-evidence-text-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::write(&path, vec![b'x'; MAX_EVIDENCE_TEXT_BYTES as usize + 1]).unwrap();

        let error = read_bounded_text_file(&path, "test evidence")
            .expect_err("oversized evidence must be rejected");
        assert!(error.contains("exceeds the 16777216-byte limit"));
        assert!(error.contains(&path.display().to_string()));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn handler_reports_metadata_safe_readiness_blockers_without_preparing_launch() {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "bridgevm-api-readiness-report-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = VmStore::new(root);
        let mut manifest = VmManifest::new(
            "ubuntu",
            VmMode::Fast,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "arm64".to_string(),
            },
            "80GiB",
        );
        manifest.boot =
            boot_template_by_id("ubuntu-arm64-installer").map(|template| template.as_boot());
        handle_request(&store, BridgeVmRequest::CreateVm { manifest })
            .into_result()
            .unwrap();

        let response = handle_request(
            &store,
            BridgeVmRequest::ReadinessReport {
                name: "ubuntu".to_string(),
                live_evidence: None,
                record_live_evidence: false,
                clear_live_evidence: false,
            },
        )
        .into_result()
        .unwrap();
        let BridgeVmResponse::ReadinessReport { report } = response else {
            panic!("expected readiness report response");
        };

        assert_eq!(report.vm, "ubuntu");
        assert_eq!(report.mode, VmMode::Fast);
        assert_eq!(report.state, VmRuntimeState::Stopped);
        assert!(report.metadata_only);
        assert!(report.live_e2e_required);
        assert!(report
            .notes
            .iter()
            .any(|note| note.contains("metadata-only preflight report")));
        assert!(report
            .notes
            .iter()
            .any(|note| note.contains("explicit opt-in live smoke")));
        assert!(report.evidence_requirements.iter().any(|requirement| {
            requirement.kind == "live-boot" && requirement.required && !requirement.proven
        }));
        assert!(report.evidence_requirements.iter().any(|requirement| {
            requirement.kind == "console" && requirement.required && !requirement.proven
        }));
        assert!(report.evidence_requirements.iter().any(|requirement| {
            requirement.kind == "guest-tools-effects" && requirement.required && !requirement.proven
        }));
        assert!(report.boot_media.as_ref().is_some_and(|status| {
            status.entries.iter().any(|entry| {
                entry.kind == BootMediaKind::InstallerImage
                    && entry.path.ends_with("installers/ubuntu-arm64.iso")
                    && !entry.exists
            })
        }));
        assert!(report
            .blockers
            .iter()
            .any(|blocker| blocker.starts_with("boot-media-missing:installer-image:")));
        assert!(report
            .blockers
            .iter()
            .any(|blocker| blocker.starts_with("active-disk-missing:")));
        assert!(report
            .blockers
            .iter()
            .any(|blocker| blocker == "launch-readiness-blocker:missing-primary-disk"));
        assert!(report
            .blockers
            .iter()
            .any(|blocker| blocker == "launch-readiness-blocker:missing-installer-image"));
        let pre_run_readiness = report
            .pre_run_launch_readiness
            .as_ref()
            .expect("expected pre-run launch readiness");
        assert!(!pre_run_readiness.ready);
        assert!(pre_run_readiness
            .blockers
            .iter()
            .any(|blocker| blocker.code == "missing-primary-disk"));
        assert!(pre_run_readiness
            .blockers
            .iter()
            .any(|blocker| blocker.code == "missing-installer-image"));
        assert!(!report
            .blockers
            .iter()
            .any(|blocker| blocker == "runner-metadata-missing"));
        assert!(store.runner_metadata("ubuntu").unwrap().is_none());
        assert!(!store
            .root()
            .join("vms")
            .join("ubuntu.vmbridge")
            .join("metadata")
            .join("primary-disk.json")
            .exists());
    }

    #[test]
    fn compatibility_readiness_reports_missing_windows_firmware_dependencies() {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "bridgevm-api-compat-firmware-readiness-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = VmStore::new(root);
        let mut manifest = VmManifest::new(
            "win11",
            VmMode::Compatibility,
            Guest {
                os: "windows".to_string(),
                version: Some("11".to_string()),
                arch: "arm64".to_string(),
            },
            "80GiB",
        );
        manifest.boot = Some(bridgevm_config::Boot {
            mode: BootMode::WindowsInstaller,
            installer_image: Some("installers/win11-arm.iso".to_string()),
            kernel_path: None,
            initrd_path: None,
            kernel_command_line: None,
            macos_restore_image: None,
        });
        manifest.firmware.tpm = true;
        manifest.firmware.secure_boot = true;
        handle_request(&store, BridgeVmRequest::CreateVm { manifest })
            .into_result()
            .unwrap();

        let response = handle_request(
            &store,
            BridgeVmRequest::ReadinessReport {
                name: "win11".to_string(),
                live_evidence: None,
                record_live_evidence: false,
                clear_live_evidence: false,
            },
        )
        .into_result()
        .unwrap();
        let BridgeVmResponse::ReadinessReport { report } = response else {
            panic!("expected readiness report response");
        };
        let pre_run_readiness = report
            .pre_run_launch_readiness
            .as_ref()
            .expect("expected Compatibility Mode pre-run readiness");

        for code in [
            "missing-primary-disk",
            "missing-windows-installer-image",
            "missing-tpm-socket",
            "missing-secure-boot-vars",
        ] {
            assert!(
                pre_run_readiness
                    .blockers
                    .iter()
                    .any(|blocker| blocker.code == code),
                "missing pre-run blocker {code}: {:?}",
                pre_run_readiness.blockers
            );
            assert!(
                report
                    .blockers
                    .iter()
                    .any(|blocker| blocker == &format!("launch-readiness-blocker:{code}")),
                "missing report blocker {code}: {:?}",
                report.blockers
            );
        }

        let response = handle_request(
            &store,
            BridgeVmRequest::PrepareRun {
                name: "win11".to_string(),
            },
        )
        .into_result()
        .unwrap();
        let BridgeVmResponse::RunnerStatus {
            metadata: Some(metadata),
            ..
        } = response
        else {
            panic!("expected runner status");
        };
        let readiness = metadata
            .launch_readiness
            .as_ref()
            .expect("Compatibility dry-run includes launch readiness");
        assert!(!readiness.ready);
        assert!(readiness
            .blockers
            .iter()
            .any(|blocker| blocker.code == "missing-windows-installer-image"));
        assert!(readiness
            .blockers
            .iter()
            .any(|blocker| blocker.code == "missing-tpm-socket"));
        assert!(readiness
            .blockers
            .iter()
            .any(|blocker| blocker.code == "missing-secure-boot-vars"));
    }

    #[test]
    fn live_boot_requirement_needs_progress_evidence_not_only_a_bundle() {
        let evidence = |serial: bool, graphical_boot: bool, viewer: bool, qmp: bool| {
            VmLiveEvidenceVerification {
                path: PathBuf::from("/tmp/evidence"),
                backend: "apple-virtualization-framework".to_string(),
                vm_name: "ubuntu".to_string(),
                boot_mode: "linux-kernel".to_string(),
                disk_format: "raw".to_string(),
                network: "nat".to_string(),
                serial_sentinel_required: serial,
                serial_sentinel_proven: serial,
                graphical_boot_progress_proven: graphical_boot,
                viewer_evidence_proven: viewer,
                qmp_evidence_proven: qmp,
                guest_tools_effects_proven: false,
                summary: "synthetic test evidence".to_string(),
            }
        };

        let launch_only = evidence(false, false, false, false);
        let launch_only_requirements = metadata_safe_live_evidence_requirements(Some(&launch_only));
        assert!(launch_only_requirements.iter().any(|requirement| {
            requirement.kind == "live-boot" && requirement.required && !requirement.proven
        }));

        let serial_progress = evidence(true, false, false, false);
        let serial_requirements = metadata_safe_live_evidence_requirements(Some(&serial_progress));
        assert!(serial_requirements.iter().any(|requirement| {
            requirement.kind == "live-boot" && requirement.required && requirement.proven
        }));

        let graphical_progress = evidence(false, true, false, false);
        let graphical_requirements =
            metadata_safe_live_evidence_requirements(Some(&graphical_progress));
        assert!(graphical_requirements.iter().any(|requirement| {
            requirement.kind == "live-boot" && requirement.required && requirement.proven
        }));

        for console_only_evidence in [
            evidence(false, false, true, false),
            evidence(false, false, false, true),
        ] {
            let requirements =
                metadata_safe_live_evidence_requirements(Some(&console_only_evidence));
            assert!(requirements.iter().any(|requirement| {
                requirement.kind == "live-boot" && requirement.required && !requirement.proven
            }));
            assert!(requirements.iter().any(|requirement| {
                requirement.kind == "console" && requirement.required && requirement.proven
            }));
        }
    }

    #[test]
    fn handler_imports_template_boot_media() {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "bridgevm-api-media-import-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let source = root.join("source.iso");
        fs::create_dir_all(&root).unwrap();
        fs::write(&source, b"fake installer").unwrap();
        let store = VmStore::new(root);
        let mut manifest = VmManifest::new(
            "ubuntu",
            VmMode::Fast,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "arm64".to_string(),
            },
            "80GiB",
        );
        manifest.boot =
            boot_template_by_id("ubuntu-arm64-installer").map(|template| template.as_boot());
        handle_request(&store, BridgeVmRequest::CreateVm { manifest })
            .into_result()
            .unwrap();

        let response = handle_request(
            &store,
            BridgeVmRequest::ImportBootMedia {
                name: "ubuntu".to_string(),
                source: source.clone(),
                kind: None,
            },
        )
        .into_result()
        .unwrap();
        let BridgeVmResponse::BootMediaImported { import } = response else {
            panic!("expected boot media import response");
        };

        assert_eq!(import.vm, "ubuntu");
        assert_eq!(import.kind, BootMediaKind::InstallerImage);
        assert_eq!(import.source, source);
        assert!(import.destination.ends_with("installers/ubuntu-arm64.iso"));
        assert_eq!(import.bytes, 14);
        assert!(!import.replaced);
        assert!(import.imported_at_unix > 0);
        assert_eq!(fs::read(&import.destination).unwrap(), b"fake installer");
        assert!(store
            .root()
            .join("vms")
            .join("ubuntu.vmbridge")
            .join("metadata")
            .join("boot-media")
            .join("installer-image.json")
            .exists());

        let response = handle_request(
            &store,
            BridgeVmRequest::InspectBootMedia {
                name: "ubuntu".to_string(),
            },
        )
        .into_result()
        .unwrap();
        let BridgeVmResponse::BootMedia { boot, .. } = response else {
            panic!("expected boot media response");
        };
        assert!(boot.installer_image.unwrap().exists);

        let response = handle_request(
            &store,
            BridgeVmRequest::InspectBootMediaStatus {
                name: "ubuntu".to_string(),
            },
        )
        .into_result()
        .unwrap();
        let BridgeVmResponse::BootMediaStatus { status } = response else {
            panic!("expected boot media status response");
        };
        assert_eq!(status.vm, "ubuntu");
        assert_eq!(status.entries.len(), 1);
        let entry = &status.entries[0];
        assert_eq!(entry.kind, BootMediaKind::InstallerImage);
        assert!(entry.exists);
        assert_eq!(entry.bytes, Some(14));
        assert_eq!(entry.last_import.as_ref().unwrap().bytes, 14);

        let expected_sha256 = sha256_file(&entry.path).unwrap();
        let response = handle_request(
            &store,
            BridgeVmRequest::VerifyBootMedia {
                name: "ubuntu".to_string(),
                expected_sha256: expected_sha256.clone(),
                kind: None,
            },
        )
        .into_result()
        .unwrap();
        let BridgeVmResponse::BootMediaVerified { verification } = response else {
            panic!("expected boot media verification response");
        };
        assert_eq!(verification.kind, BootMediaKind::InstallerImage);
        assert_eq!(verification.expected_sha256, expected_sha256);
        assert_eq!(verification.actual_sha256, expected_sha256);
        assert!(verification.verified);
        assert!(store
            .root()
            .join("vms")
            .join("ubuntu.vmbridge")
            .join("metadata")
            .join("boot-media")
            .join("installer-image-verify.json")
            .exists());

        let response = handle_request(
            &store,
            BridgeVmRequest::InspectBootMediaStatus {
                name: "ubuntu".to_string(),
            },
        )
        .into_result()
        .unwrap();
        let BridgeVmResponse::BootMediaStatus { status } = response else {
            panic!("expected boot media status response");
        };
        let verification = status.entries[0].last_verification.as_ref().unwrap();
        assert!(verification.verified);
        assert_eq!(verification.actual_sha256, expected_sha256);

        let response = handle_request(
            &store,
            BridgeVmRequest::PlanBootMediaDownload {
                name: "ubuntu".to_string(),
                url: "https://example.invalid/ubuntu.iso".to_string(),
                expected_sha256: Some(expected_sha256.clone()),
                kind: None,
            },
        )
        .into_result()
        .unwrap();
        let BridgeVmResponse::BootMediaDownloadPlanned { plan } = response else {
            panic!("expected boot media download plan response");
        };
        assert_eq!(plan.vm, "ubuntu");
        assert_eq!(plan.kind, BootMediaKind::InstallerImage);
        assert_eq!(plan.url, "https://example.invalid/ubuntu.iso");
        assert_eq!(
            plan.expected_sha256.as_deref(),
            Some(expected_sha256.as_str())
        );
        assert!(plan.exists);
        assert_eq!(plan.bytes, Some(14));
        assert!(plan.last_import.is_some());
        assert!(plan.last_verification.is_some());
        assert!(store
            .root()
            .join("vms")
            .join("ubuntu.vmbridge")
            .join("metadata")
            .join("boot-media")
            .join("installer-image-download.json")
            .exists());

        let response = handle_request(
            &store,
            BridgeVmRequest::InspectBootMediaStatus {
                name: "ubuntu".to_string(),
            },
        )
        .into_result()
        .unwrap();
        let BridgeVmResponse::BootMediaStatus { status } = response else {
            panic!("expected boot media status response");
        };
        let download_plan = status.entries[0].last_download_plan.as_ref().unwrap();
        assert_eq!(download_plan.url, "https://example.invalid/ubuntu.iso");
        assert_eq!(
            download_plan.expected_sha256.as_deref(),
            Some(expected_sha256.as_str())
        );
    }

    #[test]
    fn handler_rejects_boot_media_write_destination_outside_bundle() {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "bridgevm-api-media-path-safety-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let source = root.join("source.iso");
        fs::create_dir_all(&root).unwrap();
        fs::write(&source, b"fake installer").unwrap();
        let store = VmStore::new(root.clone());
        let mut manifest = VmManifest::new(
            "unsafe",
            VmMode::Fast,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "arm64".to_string(),
            },
            "80GiB",
        );
        manifest.boot = Some(bridgevm_config::Boot {
            mode: BootMode::LinuxInstaller,
            installer_image: Some("../escaped.iso".to_string()),
            kernel_path: None,
            initrd_path: None,
            kernel_command_line: None,
            macos_restore_image: None,
        });
        handle_request(&store, BridgeVmRequest::CreateVm { manifest })
            .into_result()
            .unwrap();

        let import_error = handle_request(
            &store,
            BridgeVmRequest::ImportBootMedia {
                name: "unsafe".to_string(),
                source: source.clone(),
                kind: None,
            },
        )
        .into_result()
        .expect_err("escaping import destination should be rejected");
        assert!(import_error.contains("outside VM bundle"));

        let plan_error = handle_request(
            &store,
            BridgeVmRequest::PlanBootMediaDownload {
                name: "unsafe".to_string(),
                url: "https://example.invalid/ubuntu.iso".to_string(),
                expected_sha256: None,
                kind: None,
            },
        )
        .into_result()
        .expect_err("escaping download destination should be rejected");
        assert!(plan_error.contains("outside VM bundle"));

        assert!(!root.join("vms").join("escaped.iso").exists());
        assert!(!store
            .bundle_path("unsafe")
            .join("metadata")
            .join("boot-media")
            .join("installer-image.json")
            .exists());
        assert!(!store
            .bundle_path("unsafe")
            .join("metadata")
            .join("boot-media")
            .join("installer-image-download.json")
            .exists());
    }

    #[test]
    fn handler_executes_planned_boot_media_download_and_reports_status() {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "bridgevm-api-media-download-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = VmStore::new(root);
        let mut manifest = VmManifest::new(
            "ubuntu",
            VmMode::Fast,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "arm64".to_string(),
            },
            "80GiB",
        );
        manifest.boot =
            boot_template_by_id("ubuntu-arm64-installer").map(|template| template.as_boot());
        handle_request(&store, BridgeVmRequest::CreateVm { manifest })
            .into_result()
            .unwrap();

        let body = b"downloaded installer";
        let expected_sha256 = format!("{:x}", Sha256::digest(body));
        let (url, server) = serve_one_http_response(body);

        let response = handle_request(
            &store,
            BridgeVmRequest::PlanBootMediaDownload {
                name: "ubuntu".to_string(),
                url: url.clone(),
                expected_sha256: Some(expected_sha256.clone()),
                kind: None,
            },
        )
        .into_result()
        .unwrap();
        let BridgeVmResponse::BootMediaDownloadPlanned { plan } = response else {
            panic!("expected boot media download plan response");
        };
        assert_eq!(plan.url, url);
        assert!(!plan.exists);
        assert_eq!(plan.bytes, None);

        let response = handle_request(
            &store,
            BridgeVmRequest::DownloadBootMedia {
                name: "ubuntu".to_string(),
                kind: None,
            },
        )
        .into_result()
        .unwrap();
        server.join().expect("http test server should finish");
        let BridgeVmResponse::BootMediaDownloaded { download } = response else {
            panic!("expected boot media downloaded response");
        };
        assert_eq!(download.vm, "ubuntu");
        assert_eq!(download.kind, BootMediaKind::InstallerImage);
        assert_eq!(download.url, plan.url);
        assert_eq!(download.destination, plan.destination);
        assert_eq!(fs::read(&download.destination).unwrap(), body);
        assert_eq!(download.bytes, Some(body.len() as u64));
        assert!(!download.replaced);
        assert_eq!(
            download.expected_sha256.as_deref(),
            Some(expected_sha256.as_str())
        );
        assert_eq!(
            download.actual_sha256.as_deref(),
            Some(expected_sha256.as_str())
        );
        assert_eq!(download.verified, Some(true));
        assert!(download.downloaded);
        assert!(download.downloaded_at_unix > 0);
        assert!(store
            .root()
            .join("vms")
            .join("ubuntu.vmbridge")
            .join("metadata")
            .join("boot-media")
            .join("installer-image-download-result.json")
            .exists());

        let response = handle_request(
            &store,
            BridgeVmRequest::InspectBootMediaStatus {
                name: "ubuntu".to_string(),
            },
        )
        .into_result()
        .unwrap();
        let BridgeVmResponse::BootMediaStatus { status } = response else {
            panic!("expected boot media status response");
        };
        let entry = &status.entries[0];
        assert!(entry.exists);
        assert_eq!(entry.bytes, Some(body.len() as u64));
        let last_download = entry.last_download.as_ref().unwrap();
        assert!(last_download.downloaded);
        assert_eq!(last_download.verified, Some(true));
        assert_eq!(
            last_download.actual_sha256.as_deref(),
            Some(expected_sha256.as_str())
        );
    }

    fn serve_one_http_response(body: &'static [u8]) -> (String, JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind local test server");
        let address = listener
            .local_addr()
            .expect("read local test server address");
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept curl connection");
            let mut request = [0_u8; 1024];
            let _ = stream.read(&mut request);
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .expect("write http response headers");
            stream.write_all(body).expect("write http response body");
        });
        (format!("http://{address}/ubuntu.iso"), handle)
    }

    #[test]
    fn handler_prepares_compatibility_run() {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "bridgevm-api-prepare-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = VmStore::new(root);
        let manifest = VmManifest::new(
            "legacy",
            VmMode::Compatibility,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "x86_64".to_string(),
            },
            "64GiB",
        );
        handle_request(&store, BridgeVmRequest::CreateVm { manifest })
            .into_result()
            .unwrap();

        let response = handle_request(
            &store,
            BridgeVmRequest::PrepareRun {
                name: "legacy".to_string(),
            },
        )
        .into_result()
        .unwrap();
        let BridgeVmResponse::RunnerStatus {
            metadata: Some(metadata),
            ..
        } = response
        else {
            panic!("expected runner status");
        };
        assert!(metadata.dry_run);
        assert_eq!(metadata.pid, None);
        assert_eq!(metadata.command.first().unwrap(), "qemu-system-x86_64");
        let guest_tools = metadata.guest_tools.expect("guest tools metadata");
        assert_eq!(guest_tools.transport, "virtio-serial");
        assert_eq!(guest_tools.channel_name, "org.bridgevm.guest-tools.0");
        assert!(guest_tools
            .socket_path
            .ends_with("metadata/guest-tools.sock"));
        assert!(guest_tools
            .token_path
            .ends_with("metadata/guest-tools-token.json"));
        let token = store.guest_tools_token("legacy").unwrap().token;
        assert!(!metadata.command.join(" ").contains(&token));
    }

    #[test]
    fn handler_qemu_args_error_preserves_network_blocker_requirement() {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "bridgevm-api-qemu-network-blocker-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = VmStore::new(root);
        let mut manifest = VmManifest::new(
            "legacy",
            VmMode::Compatibility,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "x86_64".to_string(),
            },
            "64GiB",
        );
        manifest.network.mode = "advanced".to_string();
        handle_request(&store, BridgeVmRequest::CreateVm { manifest })
            .into_result()
            .unwrap();

        let message = handle_request(
            &store,
            BridgeVmRequest::QemuArgs {
                name: "legacy".to_string(),
            },
        )
        .into_result()
        .expect_err("advanced QEMU args should expose launch blocker");

        assert!(
            message.contains("failed to build Compatibility Mode QEMU command"),
            "{message}"
        );
        assert!(
            message.contains("QEMU launch blocker qemu-advanced-network-requires-schema"),
            "{message}"
        );
        assert!(
            message.contains("requirement: Compatibility Mode QEMU requires an advanced network schema and launcher wiring before launch"),
            "{message}"
        );
    }

    #[test]
    fn handler_prepare_run_error_preserves_network_blocker_requirement() {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "bridgevm-api-prepare-network-blocker-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = VmStore::new(root);
        let mut manifest = VmManifest::new(
            "legacy",
            VmMode::Compatibility,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "x86_64".to_string(),
            },
            "64GiB",
        );
        manifest.network.mode = "advanced".to_string();
        handle_request(&store, BridgeVmRequest::CreateVm { manifest })
            .into_result()
            .unwrap();

        let message = handle_request(
            &store,
            BridgeVmRequest::PrepareRun {
                name: "legacy".to_string(),
            },
        )
        .into_result()
        .expect_err("advanced prepare-run should expose launch blocker");

        assert!(
            message.contains("failed to build Compatibility Mode QEMU command"),
            "{message}"
        );
        assert!(
            message.contains("QEMU launch blocker qemu-advanced-network-requires-schema"),
            "{message}"
        );
        assert!(
            message.contains("requirement: Compatibility Mode QEMU requires an advanced network schema and launcher wiring before launch"),
            "{message}"
        );
    }

    #[test]
    fn handler_renders_qemu_host_only_args() {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "bridgevm-api-qemu-host-only-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = VmStore::new(root);
        let mut manifest = VmManifest::new(
            "legacy",
            VmMode::Compatibility,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "x86_64".to_string(),
            },
            "64GiB",
        );
        manifest.network.mode = "host-only".to_string();
        handle_request(&store, BridgeVmRequest::CreateVm { manifest })
            .into_result()
            .unwrap();

        let response = handle_request(
            &store,
            BridgeVmRequest::QemuArgs {
                name: "legacy".to_string(),
            },
        )
        .into_result()
        .expect("host-only QEMU args should render");
        let BridgeVmResponse::QemuCommand { command } = response else {
            panic!("expected qemu command");
        };

        assert!(command.args.iter().any(|arg| arg == "vmnet-host,id=net0"));
    }

    #[test]
    fn handler_refuses_qemu_host_only_spawn_without_privileged_networking() {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "bridgevm-api-qemu-host-only-spawn-blocker-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = VmStore::new(root);
        let mut manifest = VmManifest::new(
            "legacy",
            VmMode::Compatibility,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "x86_64".to_string(),
            },
            "1MiB",
        );
        manifest.storage.primary.format = "raw".to_string();
        manifest.storage.primary.path = "disks/root.raw".to_string();
        manifest.network.mode = "host-only".to_string();
        handle_request(&store, BridgeVmRequest::CreateVm { manifest })
            .into_result()
            .unwrap();

        let message = handle_request(
            &store,
            BridgeVmRequest::RunBackend {
                name: "legacy".to_string(),
                spawn: true,
            },
        )
        .into_result()
        .expect_err("host-only spawn should require privileged vmnet support");

        assert!(
            message.contains("qemu-host-only-requires-privilege"),
            "{message}"
        );
        assert!(message.contains("vmnet-host"), "{message}");
        assert!(message.contains("com.apple.vm.networking"), "{message}");
        assert!(store.runner_metadata("legacy").unwrap().is_none());
    }

    #[test]
    fn handler_refuses_qemu_bridged_spawn_without_privileged_networking() {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "bridgevm-api-qemu-bridged-spawn-blocker-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = VmStore::new(root);
        let mut manifest = VmManifest::new(
            "legacy",
            VmMode::Compatibility,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "x86_64".to_string(),
            },
            "1MiB",
        );
        manifest.storage.primary.format = "raw".to_string();
        manifest.storage.primary.path = "disks/root.raw".to_string();
        manifest.network.mode = "bridged".to_string();
        handle_request(&store, BridgeVmRequest::CreateVm { manifest })
            .into_result()
            .unwrap();

        let message = handle_request(
            &store,
            BridgeVmRequest::RunBackend {
                name: "legacy".to_string(),
                spawn: true,
            },
        )
        .into_result()
        .expect_err("bridged spawn should require privileged vmnet support");

        assert!(
            message.contains("qemu-bridged-requires-privilege"),
            "{message}"
        );
        assert!(message.contains("vmnet-bridged"), "{message}");
        assert!(message.contains("com.apple.vm.networking"), "{message}");
        assert!(store.runner_metadata("legacy").unwrap().is_none());
    }

    #[test]
    fn handler_plans_qemu_bridged_network_blocker_without_launching() {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "bridgevm-api-network-plan-bridged-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = VmStore::new(root);
        let mut manifest = VmManifest::new(
            "legacy",
            VmMode::Compatibility,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "x86_64".to_string(),
            },
            "64GiB",
        );
        manifest.network.mode = "bridged".to_string();
        handle_request(&store, BridgeVmRequest::CreateVm { manifest })
            .into_result()
            .unwrap();

        let response = handle_request(
            &store,
            BridgeVmRequest::PlanNetwork {
                name: "legacy".to_string(),
            },
        )
        .into_result()
        .expect("network plan should return blockers as data");
        let BridgeVmResponse::NetworkPlanned { plan } = response else {
            panic!("expected network plan response");
        };

        assert!(plan.dry_run);
        assert!(!plan.executable);
        assert_eq!(plan.backend, "qemu");
        assert_eq!(plan.mode, "bridged");
        assert!(plan
            .blockers
            .iter()
            .any(|blocker| blocker.code == "qemu-bridged-requires-privilege"
                && blocker.message.contains("com.apple.vm.networking")));
        assert!(store.runner_metadata("legacy").unwrap().is_none());
    }

    #[test]
    fn handler_plans_qemu_host_only_privilege_blocker_without_launching() {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "bridgevm-api-network-plan-host-only-privilege-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = VmStore::new(root);
        let mut manifest = VmManifest::new(
            "legacy",
            VmMode::Compatibility,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "x86_64".to_string(),
            },
            "64GiB",
        );
        manifest.network.mode = "host-only".to_string();
        handle_request(&store, BridgeVmRequest::CreateVm { manifest })
            .into_result()
            .unwrap();

        let response = handle_request(
            &store,
            BridgeVmRequest::PlanNetwork {
                name: "legacy".to_string(),
            },
        )
        .into_result()
        .expect("network plan should return privilege blockers as data");
        let BridgeVmResponse::NetworkPlanned { plan } = response else {
            panic!("expected network plan response");
        };

        assert!(plan.dry_run);
        assert!(!plan.executable);
        assert_eq!(plan.backend, "qemu");
        assert_eq!(plan.mode, "host-only");
        assert!(plan
            .capabilities
            .as_ref()
            .is_some_and(|capabilities| capabilities.requires_privileged_helper));
        assert!(plan.blockers.iter().any(|blocker| blocker.code
            == "qemu-host-only-requires-privilege"
            && blocker.message.contains("vmnet-host")
            && blocker.message.contains("com.apple.vm.networking")));
        assert!(store.runner_metadata("legacy").unwrap().is_none());
    }

    #[test]
    fn handler_plans_host_only_port_forward_blocker_without_mutation() {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "bridgevm-api-network-plan-host-only-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = VmStore::new(root);
        let mut manifest = VmManifest::new(
            "legacy",
            VmMode::Compatibility,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "x86_64".to_string(),
            },
            "64GiB",
        );
        manifest.network.mode = "host-only".to_string();
        manifest.network.forwards.push(PortForward {
            host: 3000,
            guest: 3000,
        });
        handle_request(&store, BridgeVmRequest::CreateVm { manifest })
            .into_result()
            .unwrap();

        let response = handle_request(
            &store,
            BridgeVmRequest::PlanNetwork {
                name: "legacy".to_string(),
            },
        )
        .into_result()
        .expect("network plan should return blockers as data");
        let BridgeVmResponse::NetworkPlanned { plan } = response else {
            panic!("expected network plan response");
        };

        assert!(plan.dry_run);
        assert!(!plan.executable);
        assert_eq!(plan.mode, "host-only");
        assert_eq!(plan.port_forwards[0].host, 3000);
        assert!(plan
            .blockers
            .iter()
            .any(|blocker| blocker.code == "unsupported-port-forwarding"));
        let (_, manifest) = store.get_vm("legacy").unwrap();
        assert_eq!(manifest.network.forwards.len(), 1);
    }

    #[test]
    fn handler_updates_port_forwards_and_qemu_args() {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "bridgevm-api-port-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = VmStore::new(root);
        let manifest = VmManifest::new(
            "legacy",
            VmMode::Compatibility,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "x86_64".to_string(),
            },
            "64GiB",
        );
        handle_request(&store, BridgeVmRequest::CreateVm { manifest })
            .into_result()
            .unwrap();

        let response = handle_request(
            &store,
            BridgeVmRequest::AddPort {
                name: "legacy".to_string(),
                host: 3000,
                guest: 3000,
            },
        )
        .into_result()
        .unwrap();
        let BridgeVmResponse::PortForwards { ports } = response else {
            panic!("expected port forwards response");
        };
        assert_eq!(ports.vm, "legacy");
        assert_eq!(ports.forwards.len(), 1);
        assert_eq!(ports.forwards[0].host, 3000);
        assert_eq!(ports.forwards[0].guest, 3000);

        let duplicate = handle_request(
            &store,
            BridgeVmRequest::AddPort {
                name: "legacy".to_string(),
                host: 3000,
                guest: 8080,
            },
        )
        .into_result()
        .expect_err("duplicate host port should fail");
        assert!(duplicate.contains("host port 3000"));

        let response = handle_request(
            &store,
            BridgeVmRequest::PrepareRun {
                name: "legacy".to_string(),
            },
        )
        .into_result()
        .unwrap();
        let BridgeVmResponse::RunnerStatus {
            metadata: Some(metadata),
            ..
        } = response
        else {
            panic!("expected runner status");
        };
        assert!(metadata
            .command
            .iter()
            .any(|word| word.contains("hostfwd=tcp::3000-:3000")));

        let response = handle_request(
            &store,
            BridgeVmRequest::RemovePort {
                name: "legacy".to_string(),
                host: 3000,
                guest: 3000,
            },
        )
        .into_result()
        .unwrap();
        let BridgeVmResponse::PortForwards { ports } = response else {
            panic!("expected port forwards response");
        };
        assert!(ports.forwards.is_empty());

        let (_, manifest) = store.get_vm("legacy").unwrap();
        assert!(manifest.network.forwards.is_empty());
    }

    #[test]
    fn handler_rejects_port_forwards_outside_nat_networking() {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "bridgevm-api-port-mode-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = VmStore::new(root);
        let mut manifest = VmManifest::new(
            "legacy",
            VmMode::Compatibility,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "x86_64".to_string(),
            },
            "64GiB",
        );
        manifest.network.mode = "host-only".to_string();
        handle_request(&store, BridgeVmRequest::CreateVm { manifest })
            .into_result()
            .unwrap();

        let error = handle_request(
            &store,
            BridgeVmRequest::AddPort {
                name: "legacy".to_string(),
                host: 3000,
                guest: 3000,
            },
        )
        .into_result()
        .expect_err("host-only port forward should fail");
        assert!(error.contains("host-only networking does not support port forwarding"));

        let (_, manifest) = store.get_vm("legacy").unwrap();
        assert!(manifest.network.forwards.is_empty());
    }

    #[test]
    fn handler_plans_ssh_from_port_forward_for_compatibility_mode() {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "bridgevm-api-ssh-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = VmStore::new(root);
        let mut manifest = VmManifest::new(
            "legacy",
            VmMode::Compatibility,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "x86_64".to_string(),
            },
            "64GiB",
        );
        manifest.network.forwards.push(PortForward {
            host: 2222,
            guest: 22,
        });
        handle_request(&store, BridgeVmRequest::CreateVm { manifest })
            .into_result()
            .unwrap();

        let response = handle_request(
            &store,
            BridgeVmRequest::SshPlan {
                name: "legacy".to_string(),
                user: Some("ubuntu".to_string()),
            },
        )
        .into_result()
        .unwrap();
        let BridgeVmResponse::SshPlan { plan } = response else {
            panic!("expected ssh plan");
        };
        assert_eq!(plan.source, SshPlanSource::PortForward);
        assert_eq!(
            plan.command,
            vec![
                "ssh".to_string(),
                "-p".to_string(),
                "2222".to_string(),
                "ubuntu@127.0.0.1".to_string()
            ]
        );

        store
            .write_guest_tools_runtime_metadata(
                "legacy",
                &GuestToolsRuntimeMetadata {
                    connected: true,
                    guest_os: Some("linux".to_string()),
                    agent_version: Some("1.0.0".to_string()),
                    capabilities: vec!["guest-ip".to_string()],
                    last_heartbeat_at_unix: Some(1),
                    guest_ip_addresses: vec![GuestToolsIpAddressMetadata {
                        address: "10.0.2.15".to_string(),
                        interface: Some("eth0".to_string()),
                    }],
                    shared_folders: Vec::new(),
                    metrics: None,
                    last_command_result: None,
                    agent_update: None,
                    clipboard: None,
                    updated_at_unix: 2,
                },
            )
            .unwrap();

        let response = handle_request(
            &store,
            BridgeVmRequest::SshPlan {
                name: "legacy".to_string(),
                user: Some("ubuntu".to_string()),
            },
        )
        .into_result()
        .unwrap();
        let BridgeVmResponse::SshPlan { plan } = response else {
            panic!("expected ssh plan");
        };
        assert_eq!(plan.source, SshPlanSource::PortForward);
        assert_eq!(plan.command.last().unwrap(), "ubuntu@127.0.0.1");
    }

    #[test]
    fn handler_plans_open_from_guest_port_forward() {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "bridgevm-api-open-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = VmStore::new(root);
        let mut manifest = VmManifest::new(
            "legacy",
            VmMode::Compatibility,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "x86_64".to_string(),
            },
            "64GiB",
        );
        manifest.network.forwards.push(PortForward {
            host: 18080,
            guest: 80,
        });
        handle_request(&store, BridgeVmRequest::CreateVm { manifest })
            .into_result()
            .unwrap();

        let response = handle_request(
            &store,
            BridgeVmRequest::OpenPort {
                name: "legacy".to_string(),
                guest: 80,
                scheme: Some("HTTPS".to_string()),
            },
        )
        .into_result()
        .unwrap();
        let BridgeVmResponse::OpenPortPlan { plan } = response else {
            panic!("expected open port plan");
        };
        assert_eq!(plan.scheme, "https");
        assert_eq!(plan.guest_port, 80);
        assert_eq!(plan.host_port, 18080);
        assert_eq!(plan.url, "https://127.0.0.1:18080");
        assert_eq!(
            plan.command,
            vec!["open".to_string(), "https://127.0.0.1:18080".to_string()]
        );
    }

    #[test]
    fn handler_rejects_open_without_guest_port_forward() {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "bridgevm-api-open-missing-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = VmStore::new(root);
        let manifest = VmManifest::new(
            "legacy",
            VmMode::Compatibility,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "x86_64".to_string(),
            },
            "64GiB",
        );
        handle_request(&store, BridgeVmRequest::CreateVm { manifest })
            .into_result()
            .unwrap();

        let error = handle_request(
            &store,
            BridgeVmRequest::OpenPort {
                name: "legacy".to_string(),
                guest: 80,
                scheme: Some("http".to_string()),
            },
        )
        .into_result()
        .expect_err("missing forwarded guest port should fail");
        assert!(error.contains("no host port is forwarded to guest port 80"));
    }

    #[test]
    fn handler_plans_ssh_from_connected_guest_tools_ip() {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "bridgevm-api-ssh-ip-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = VmStore::new(root);
        let manifest = VmManifest::new(
            "dev",
            VmMode::Fast,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "arm64".to_string(),
            },
            "80GiB",
        );
        handle_request(&store, BridgeVmRequest::CreateVm { manifest })
            .into_result()
            .unwrap();
        store
            .write_guest_tools_runtime_metadata(
                "dev",
                &GuestToolsRuntimeMetadata {
                    connected: true,
                    guest_os: Some("linux".to_string()),
                    agent_version: Some("1.0.0".to_string()),
                    capabilities: vec!["guest-ip".to_string()],
                    last_heartbeat_at_unix: Some(1),
                    guest_ip_addresses: vec![
                        GuestToolsIpAddressMetadata {
                            address: "127.0.0.1".to_string(),
                            interface: Some("lo".to_string()),
                        },
                        GuestToolsIpAddressMetadata {
                            address: "10.0.2.15".to_string(),
                            interface: Some("eth0".to_string()),
                        },
                    ],
                    shared_folders: Vec::new(),
                    metrics: None,
                    last_command_result: None,
                    agent_update: None,
                    clipboard: None,
                    updated_at_unix: 2,
                },
            )
            .unwrap();

        let response = handle_request(
            &store,
            BridgeVmRequest::SshPlan {
                name: "dev".to_string(),
                user: Some("ubuntu".to_string()),
            },
        )
        .into_result()
        .unwrap();
        let BridgeVmResponse::SshPlan { plan } = response else {
            panic!("expected ssh plan");
        };
        assert_eq!(plan.source, SshPlanSource::GuestToolsIp);
        assert_eq!(
            plan.command,
            vec!["ssh".to_string(), "ubuntu@10.0.2.15".to_string()]
        );
    }

    #[test]
    fn handler_views_bounded_vm_log_tail() {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "bridgevm-api-log-view-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = VmStore::new(root);
        let manifest = VmManifest::new(
            "legacy",
            VmMode::Compatibility,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "x86_64".to_string(),
            },
            "80GiB",
        );
        handle_request(&store, BridgeVmRequest::CreateVm { manifest })
            .into_result()
            .unwrap();
        let (bundle, _) = store.get_vm("legacy").unwrap();
        fs::create_dir_all(bundle.join("logs")).unwrap();
        fs::write(
            bundle.join("logs").join("qemu.log"),
            "first\nsecond\nthird\n",
        )
        .unwrap();

        let response = handle_request(
            &store,
            BridgeVmRequest::ViewLogs {
                name: "legacy".to_string(),
                kind: VmLogKind::Qemu,
                max_bytes: Some(12),
            },
        )
        .into_result()
        .unwrap();
        let BridgeVmResponse::LogsViewed { log } = response else {
            panic!("expected log view response");
        };
        assert_eq!(log.vm, "legacy");
        assert_eq!(log.kind, VmLogKind::Qemu);
        assert!(log.exists);
        assert_eq!(log.bytes, 19);
        assert_eq!(log.returned_bytes, 12);
        assert!(log.truncated);
        assert_eq!(log.content, "econd\nthird\n");
    }

    #[test]
    fn handler_rejects_ssh_plan_without_target() {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "bridgevm-api-ssh-missing-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = VmStore::new(root);
        let manifest = VmManifest::new(
            "dev",
            VmMode::Fast,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "arm64".to_string(),
            },
            "80GiB",
        );
        handle_request(&store, BridgeVmRequest::CreateVm { manifest })
            .into_result()
            .unwrap();

        let response = handle_request(
            &store,
            BridgeVmRequest::SshPlan {
                name: "dev".to_string(),
                user: None,
            },
        );
        let BridgeVmResponse::Error { message } = response else {
            panic!("expected error response");
        };
        assert!(message.contains("no SSH target available"));
    }

    #[test]
    fn handler_creates_redacted_diagnostic_bundle() {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "bridgevm-api-diagnostics-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = VmStore::new(root);
        let manifest = VmManifest::new(
            "legacy",
            VmMode::Compatibility,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "x86_64".to_string(),
            },
            "64GiB",
        );
        handle_request(&store, BridgeVmRequest::CreateVm { manifest })
            .into_result()
            .unwrap();
        let token = store.guest_tools_token("legacy").unwrap().token;
        let bundle = store.bundle_path("legacy");
        fs::write(
            bundle.join("logs").join("qemu.log"),
            format!("booted with token {token}\n"),
        )
        .unwrap();
        fs::write(
            bundle.join("metadata").join("secrets.json"),
            r#"{"password":"open-sesame","nested":{"authorization":"Bearer abc"}}"#,
        )
        .unwrap();
        fs::create_dir_all(bundle.join("metadata").join("boot-media")).unwrap();
        fs::write(
            bundle.join("metadata").join("boot-media").join("download-plan.json"),
            r#"{"url":"https://example.invalid/ubuntu.iso?sig=secret#section","command":["curl","https://example.invalid/ubuntu.iso?sig=secret"]}"#,
        )
        .unwrap();
        fs::write(
            bundle.join("metadata").join("qmp-supervisor.json"),
            r#"{"events":[{"event":"RESUME"}],"terminal_event":null,"envelopes_read":1,"limit_reached":false,"updated_at_unix":1}"#,
        )
        .unwrap();
        fs::write(bundle.join("metadata").join("diagnostics.lock"), "locked").unwrap();
        fs::create_dir_all(bundle.join("disks")).unwrap();
        fs::write(bundle.join("disks").join("root.qcow2"), "not copied").unwrap();

        let output = store.root().join("diagnostics-output");
        let response = handle_request(
            &store,
            BridgeVmRequest::CreateDiagnosticBundle {
                name: "legacy".to_string(),
                output,
            },
        )
        .into_result()
        .unwrap();
        let BridgeVmResponse::DiagnosticBundle { bundle } = response else {
            panic!("expected diagnostic bundle response");
        };

        assert!(bundle.output.exists());
        assert!(bundle.files.contains(&PathBuf::from("manifest.yaml")));
        assert!(bundle
            .files
            .contains(&PathBuf::from("metadata/guest-tools-token.json")));
        assert!(bundle.files.contains(&PathBuf::from("logs/qemu.log")));
        assert!(bundle
            .files
            .contains(&PathBuf::from("metadata/qmp-supervisor.json")));
        assert!(bundle
            .files
            .contains(&PathBuf::from("diagnostic-bundle.json")));
        assert!(!bundle
            .files
            .contains(&PathBuf::from("metadata/diagnostics.lock")));
        assert!(!bundle.files.contains(&PathBuf::from("disks/root.qcow2")));
        for file in &bundle.files {
            assert!(
                file.is_relative(),
                "diagnostic metadata should only report relative paths: {}",
                file.display()
            );
            assert!(
                !file
                    .components()
                    .any(|component| matches!(component, std::path::Component::ParentDir)),
                "diagnostic metadata should not report parent-directory paths: {}",
                file.display()
            );
            let file_name = file
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default();
            assert!(
                !file_name.ends_with(".sock") && !file_name.ends_with(".lock"),
                "diagnostic metadata should not report socket or lock files: {}",
                file.display()
            );
        }

        for file in &bundle.files {
            let content = fs::read_to_string(bundle.output.join(file)).unwrap();
            assert!(
                !content.contains(&token),
                "diagnostic file leaked guest tools token: {}",
                file.display()
            );
        }
        let token_metadata =
            fs::read_to_string(bundle.output.join("metadata/guest-tools-token.json")).unwrap();
        assert!(token_metadata.contains("<redacted>"));
        let log = fs::read_to_string(bundle.output.join("logs/qemu.log")).unwrap();
        assert!(log.contains("<redacted>"));
        let secrets = fs::read_to_string(bundle.output.join("metadata/secrets.json")).unwrap();
        assert!(!secrets.contains("open-sesame"));
        assert!(!secrets.contains("Bearer abc"));
        assert!(secrets.contains("<redacted>"));
        let download_plan = fs::read_to_string(
            bundle
                .output
                .join("metadata")
                .join("boot-media")
                .join("download-plan.json"),
        )
        .unwrap();
        assert!(!download_plan.contains("sig=secret"));
        assert!(download_plan.contains("https://example.invalid/ubuntu.iso?<redacted>#section"));
        assert!(download_plan.contains("https://example.invalid/ubuntu.iso?<redacted>"));
    }

    #[test]
    fn handler_creates_metadata_only_performance_baseline() {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "bridgevm-api-performance-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = VmStore::new(root);
        let manifest = VmManifest::new(
            "dev",
            VmMode::Fast,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "arm64".to_string(),
            },
            "80GiB",
        );
        handle_request(&store, BridgeVmRequest::CreateVm { manifest })
            .into_result()
            .unwrap();
        handle_request(
            &store,
            BridgeVmRequest::TransitionVm {
                name: "dev".to_string(),
                state: VmRuntimeState::Running,
            },
        )
        .into_result()
        .unwrap();

        let bundle = store.bundle_path("dev");
        let runner = RunnerMetadata {
            engine: "lightvm".to_string(),
            pid: Some(42),
            command: vec![
                "lightvm-runner".to_string(),
                "--vm".to_string(),
                "dev".to_string(),
            ],
            log_path: bundle.join("logs").join("runner.log"),
            started_at_unix: 10,
            dry_run: false,
            launch_spec_path: None,
            guest_tools: None,
            disk: None,
            active_disk: None,
            launch_readiness: None,
            runtime_control: None,
        };
        store.write_runner_metadata("dev", &runner).unwrap();
        store
            .write_guest_tools_runtime_metadata(
                "dev",
                &GuestToolsRuntimeMetadata {
                    connected: true,
                    guest_os: Some("linux".to_string()),
                    agent_version: Some("1.0.0".to_string()),
                    capabilities: vec!["guest-metrics".to_string()],
                    last_heartbeat_at_unix: Some(11),
                    guest_ip_addresses: vec![GuestToolsIpAddressMetadata {
                        address: "10.0.2.15".to_string(),
                        interface: Some("eth0".to_string()),
                    }],
                    shared_folders: Vec::new(),
                    metrics: Some(GuestToolsMetricsMetadata {
                        cpu_percent: 7,
                        memory_used_mib: 512,
                        updated_at_unix: 12,
                    }),
                    last_command_result: None,
                    agent_update: None,
                    clipboard: None,
                    updated_at_unix: 13,
                },
            )
            .unwrap();

        let output = store.root().join("performance-output");
        let response = handle_request(
            &store,
            BridgeVmRequest::CreatePerformanceBaseline {
                name: "dev".to_string(),
                output,
            },
        )
        .into_result()
        .unwrap();
        let BridgeVmResponse::PerformanceBaseline { baseline } = response else {
            panic!("expected performance baseline response");
        };

        assert!(baseline.output.exists());
        assert!(baseline.artifact.exists());
        assert!(baseline.metadata_only);
        assert_eq!(baseline.state.state, VmRuntimeState::Running);
        assert_eq!(baseline.runner.as_ref().unwrap().engine, "lightvm");
        assert_eq!(baseline.metrics.as_ref().unwrap().cpu_percent, 7);
        assert_eq!(baseline.metrics.as_ref().unwrap().memory_used_mib, 512);
        assert_measurement(&baseline.measurements, "guest_cpu_percent", 7, "percent");
        assert_measurement(&baseline.measurements, "guest_memory_used_mib", 512, "MiB");
        assert!(baseline
            .measurements
            .iter()
            .any(|measurement| measurement.name == "runner_observed_uptime_seconds"));
        assert!(baseline
            .notes
            .iter()
            .any(|note| note.contains("metadata-only")));

        let artifact =
            fs::read_to_string(baseline.output.join("performance-baseline.json")).unwrap();
        let decoded: PerformanceBaselineMetadata = serde_json::from_str(&artifact).unwrap();
        assert_eq!(decoded.vm, "dev");
        assert_eq!(decoded.runner.unwrap().engine, "lightvm");
        assert_measurement(&decoded.measurements, "guest_cpu_percent", 7, "percent");
        assert_eq!(decoded.metrics.unwrap().memory_used_mib, 512);
    }

    #[test]
    fn handler_creates_host_side_performance_sample() {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "bridgevm-api-performance-sample-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = VmStore::new(root);
        let manifest = VmManifest::new(
            "dev",
            VmMode::Fast,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "arm64".to_string(),
            },
            "80GiB",
        );
        handle_request(&store, BridgeVmRequest::CreateVm { manifest })
            .into_result()
            .unwrap();

        let output = store.root().join("performance-sample-output");
        let response = handle_request(
            &store,
            BridgeVmRequest::CreatePerformanceSample {
                name: "dev".to_string(),
                output,
                artifact_bytes: Some(4096),
                iterations: Some(3),
                sync: false,
            },
        )
        .into_result()
        .unwrap();
        let BridgeVmResponse::PerformanceSample { sample } = response else {
            panic!("expected performance sample response");
        };

        assert!(sample.output.exists());
        assert!(sample.artifact.exists());
        assert!(sample.probe.exists());
        assert_eq!(sample.probes.len(), 3);
        assert_eq!(sample.iteration_results.len(), 3);
        assert_eq!(sample.artifact_bytes, 4096);
        assert_eq!(sample.iterations, 3);
        assert!(!sample.sync);
        for probe in &sample.probes {
            assert!(probe.exists());
            assert_eq!(fs::metadata(probe).unwrap().len(), 4096);
        }
        assert_non_metadata_measurement(
            &sample.measurements,
            "host_artifact_write_bytes",
            4096,
            "bytes",
        );
        assert_non_metadata_measurement(
            &sample.measurements,
            "host_artifact_write_iterations",
            3,
            "count",
        );
        assert_non_metadata_measurement(
            &sample.measurements,
            "host_artifact_write_total_bytes",
            12_288,
            "bytes",
        );
        assert_non_metadata_measurement_exists(
            &sample.measurements,
            "host_artifact_write_latency_microseconds",
            "microseconds",
        );
        assert_non_metadata_measurement_exists(
            &sample.measurements,
            "host_artifact_write_latency_p50_microseconds",
            "microseconds",
        );
        assert_non_metadata_measurement_exists(
            &sample.measurements,
            "bridgevm_state_read_latency_microseconds",
            "microseconds",
        );
        assert_non_metadata_measurement_exists(
            &sample.measurements,
            "bridgevm_runner_metadata_read_latency_microseconds",
            "microseconds",
        );
        assert_non_metadata_measurement_exists(
            &sample.measurements,
            "bridgevm_guest_tools_status_inspect_latency_microseconds",
            "microseconds",
        );
        assert!(!sample
            .measurements
            .iter()
            .any(|measurement| measurement.name == "disk_inspect_duration_microseconds"));
        assert!(sample
            .notes
            .iter()
            .any(|note| note.contains("disk inspect duration skipped")));
        assert!(sample.measurements.iter().any(|measurement| {
            measurement.name == "sample_generation_duration_microseconds"
                && measurement.unit == "microseconds"
                && !measurement.metadata_only
        }));

        let artifact = fs::read_to_string(sample.output.join("performance-sample.json")).unwrap();
        let decoded: PerformanceSampleMetadata = serde_json::from_str(&artifact).unwrap();
        assert_eq!(decoded.vm, "dev");
        assert_eq!(decoded.probes.len(), 3);
        assert_eq!(decoded.iteration_results.len(), 3);
        assert_non_metadata_measurement(
            &decoded.measurements,
            "host_artifact_write_bytes",
            4096,
            "bytes",
        );
    }

    #[test]
    fn handler_rejects_invalid_performance_sample_bounds_before_writing() {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "bridgevm-api-performance-sample-bounds-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = VmStore::new(root);
        let manifest = VmManifest::new(
            "dev",
            VmMode::Fast,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "arm64".to_string(),
            },
            "80GiB",
        );
        handle_request(&store, BridgeVmRequest::CreateVm { manifest })
            .into_result()
            .unwrap();

        let output = store.root().join("performance-sample-invalid-output");
        let cases = [
            (
                Some(4096),
                Some(0),
                "performance sample iterations must be greater than zero",
            ),
            (
                Some(4096),
                Some(MAX_PERFORMANCE_SAMPLE_ITERATIONS + 1),
                "performance sample iterations is too large",
            ),
            (
                Some(MAX_PERFORMANCE_SAMPLE_ARTIFACT_BYTES + 1),
                Some(1),
                "performance sample artifact is too large",
            ),
            (
                Some(MAX_PERFORMANCE_SAMPLE_ARTIFACT_BYTES),
                Some(5),
                "performance sample total artifact bytes is too large",
            ),
        ];

        for (artifact_bytes, iterations, expected) in cases {
            let response = handle_request(
                &store,
                BridgeVmRequest::CreatePerformanceSample {
                    name: "dev".to_string(),
                    output: output.clone(),
                    artifact_bytes,
                    iterations,
                    sync: false,
                },
            );
            let error = response.into_result().unwrap_err();
            assert!(
                error.contains(expected),
                "expected {expected:?} in {error:?}"
            );
        }

        assert!(
            !output.exists(),
            "invalid sample bounds should not create an output directory"
        );
    }

    fn assert_measurement(
        measurements: &[PerformanceMeasurementRecord],
        name: &str,
        value: u64,
        unit: &str,
    ) {
        let measurement = measurements
            .iter()
            .find(|measurement| measurement.name == name)
            .unwrap_or_else(|| panic!("missing performance measurement {name}"));
        assert_eq!(measurement.value, value);
        assert_eq!(measurement.unit, unit);
        assert!(measurement.metadata_only);
    }

    fn assert_non_metadata_measurement(
        measurements: &[PerformanceMeasurementRecord],
        name: &str,
        value: u64,
        unit: &str,
    ) {
        let measurement = measurements
            .iter()
            .find(|measurement| measurement.name == name)
            .unwrap_or_else(|| panic!("missing performance measurement {name}"));
        assert_eq!(measurement.value, value);
        assert_eq!(measurement.unit, unit);
        assert!(!measurement.metadata_only);
    }

    fn assert_non_metadata_measurement_exists(
        measurements: &[PerformanceMeasurementRecord],
        name: &str,
        unit: &str,
    ) {
        let measurement = measurements
            .iter()
            .find(|measurement| measurement.name == name)
            .unwrap_or_else(|| panic!("missing performance measurement {name}"));
        assert_eq!(measurement.unit, unit);
        assert!(!measurement.metadata_only);
    }

    #[test]
    fn handler_prepares_fast_run_without_qemu() {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "bridgevm-api-fast-prepare-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = VmStore::new(root);
        let manifest = VmManifest::new(
            "fast-linux",
            VmMode::Fast,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "arm64".to_string(),
            },
            "80GiB",
        );
        handle_request(&store, BridgeVmRequest::CreateVm { manifest })
            .into_result()
            .unwrap();

        let response = handle_request(
            &store,
            BridgeVmRequest::PrepareRun {
                name: "fast-linux".to_string(),
            },
        )
        .into_result()
        .unwrap();
        let BridgeVmResponse::RunnerStatus {
            metadata: Some(metadata),
            ..
        } = response
        else {
            panic!("expected runner status");
        };
        assert!(metadata.dry_run);
        assert_eq!(metadata.engine, "lightvm");
        assert_eq!(metadata.command.first().unwrap(), "lightvm-runner");
        let readiness = metadata
            .launch_readiness
            .expect("Fast Mode runner metadata includes launch readiness");
        assert!(!readiness.ready);
        assert!(readiness
            .blockers
            .iter()
            .any(|blocker| blocker.code == "missing-primary-disk"));
    }

    #[test]
    fn handler_plans_lifecycle_qmp_without_connecting_to_backend() {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "bridgevm-api-lifecycle-plan-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = VmStore::new(root);
        let manifest = VmManifest::new(
            "legacy",
            VmMode::Compatibility,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "x86_64".to_string(),
            },
            "80GiB",
        );
        handle_request(&store, BridgeVmRequest::CreateVm { manifest })
            .into_result()
            .unwrap();
        handle_request(
            &store,
            BridgeVmRequest::TransitionVm {
                name: "legacy".to_string(),
                state: VmRuntimeState::Running,
            },
        )
        .into_result()
        .unwrap();

        let response = handle_request(
            &store,
            BridgeVmRequest::LifecyclePlan {
                name: "legacy".to_string(),
                action: LifecycleAction::Suspend,
            },
        )
        .into_result()
        .unwrap();
        let BridgeVmResponse::LifecyclePlan { plan } = response else {
            panic!("expected lifecycle plan");
        };
        assert_eq!(plan.backend, "qemu-qmp");
        assert_eq!(plan.qmp_command.as_deref(), Some("stop"));
        assert!(!plan.socket_available);
        assert!(!plan.executable);
        assert!(plan
            .blockers
            .iter()
            .any(|blocker| blocker.starts_with("qmp-socket-unavailable:")));

        let socket_path = plan.socket_path.clone().expect("qmp socket path");
        fs::write(&socket_path, b"fake qmp presence marker").unwrap();
        let response = handle_request(
            &store,
            BridgeVmRequest::LifecyclePlan {
                name: "legacy".to_string(),
                action: LifecycleAction::Suspend,
            },
        )
        .into_result()
        .unwrap();
        let BridgeVmResponse::LifecyclePlan { plan } = response else {
            panic!("expected lifecycle plan");
        };
        assert!(plan.socket_available);
        assert!(plan.executable);
        assert!(plan.blockers.is_empty());
    }

    #[test]
    fn handler_plans_fast_lifecycle_as_metadata_only_blocked() {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "bridgevm-api-fast-lifecycle-plan-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = VmStore::new(root);
        let manifest = VmManifest::new(
            "fast-linux",
            VmMode::Fast,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "arm64".to_string(),
            },
            "80GiB",
        );
        handle_request(&store, BridgeVmRequest::CreateVm { manifest })
            .into_result()
            .unwrap();

        let response = handle_request(
            &store,
            BridgeVmRequest::LifecyclePlan {
                name: "fast-linux".to_string(),
                action: LifecycleAction::Suspend,
            },
        )
        .into_result()
        .unwrap();
        let BridgeVmResponse::LifecyclePlan { plan } = response else {
            panic!("expected lifecycle plan");
        };
        assert_eq!(plan.backend, "apple-vz");
        assert!(plan.metadata_only);
        // Not executable here because Stopped -> Suspended is an invalid direct
        // transition (the suspend backend itself goes Stopped -> Running ->
        // Suspended). Fast suspend/resume is no longer reported as unimplemented.
        assert!(!plan.executable);
        assert!(!plan
            .blockers
            .contains(&"fast-mode-suspend-resume-backend-unimplemented".to_string()));
        assert!(plan
            .blockers
            .iter()
            .any(|blocker| blocker.starts_with("invalid-lifecycle-transition:")));

        let response = handle_request(
            &store,
            BridgeVmRequest::LifecyclePlan {
                name: "fast-linux".to_string(),
                action: LifecycleAction::Resume,
            },
        )
        .into_result()
        .unwrap();
        let BridgeVmResponse::LifecyclePlan { plan } = response else {
            panic!("expected lifecycle plan");
        };
        assert_eq!(plan.target_state, VmRuntimeState::Running);
        assert!(!plan.executable);
        assert!(plan
            .blockers
            .iter()
            .any(|blocker| blocker == "invalid-lifecycle-transition:stopped->running"));
    }

    #[test]
    fn handler_fast_lifecycle_plan_requires_existing_runner_for_valid_transition() {
        let _guard = APPLE_VZ_RUNNER_ENV_LOCK.lock().unwrap();
        let _env = EnvVarGuard::capture("BRIDGEVM_APPLE_VZ_RUNNER");
        std::env::remove_var("BRIDGEVM_APPLE_VZ_RUNNER");

        let mut root = std::env::temp_dir();
        root.push(format!(
            "bridgevm-api-fast-lifecycle-runner-plan-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = VmStore::new(root);
        let manifest = VmManifest::new(
            "fast-linux",
            VmMode::Fast,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "arm64".to_string(),
            },
            "80GiB",
        );
        handle_request(&store, BridgeVmRequest::CreateVm { manifest })
            .into_result()
            .unwrap();
        handle_request(
            &store,
            BridgeVmRequest::TransitionVm {
                name: "fast-linux".to_string(),
                state: VmRuntimeState::Running,
            },
        )
        .into_result()
        .unwrap();

        let response = handle_request(
            &store,
            BridgeVmRequest::LifecyclePlan {
                name: "fast-linux".to_string(),
                action: LifecycleAction::Suspend,
            },
        )
        .into_result()
        .unwrap();
        let BridgeVmResponse::LifecyclePlan { plan } = response else {
            panic!("expected lifecycle plan");
        };
        assert_eq!(plan.backend, "apple-vz");
        assert!(!plan.executable);
        assert!(plan.blockers.iter().any(|blocker| {
            blocker.starts_with("apple-vz-runner-unavailable:set BRIDGEVM_APPLE_VZ_RUNNER")
        }));

        let runner = store.root().join("AppleVzRunner");
        fs::write(&runner, b"fake runner").unwrap();
        std::env::set_var("BRIDGEVM_APPLE_VZ_RUNNER", &runner);

        let response = handle_request(
            &store,
            BridgeVmRequest::LifecyclePlan {
                name: "fast-linux".to_string(),
                action: LifecycleAction::Suspend,
            },
        )
        .into_result()
        .unwrap();
        let BridgeVmResponse::LifecyclePlan { plan } = response else {
            panic!("expected lifecycle plan");
        };
        assert!(plan.executable);
        assert!(plan.blockers.is_empty());

        let _ = std::fs::remove_dir_all(store.root());
    }

    #[test]
    fn handler_fast_spawn_error_updates_runner_metadata_with_blocker() {
        let _guard = APPLE_VZ_RUNNER_ENV_LOCK.lock().unwrap();
        let _env = EnvVarGuard::capture("BRIDGEVM_APPLE_VZ_RUNNER");
        std::env::remove_var("BRIDGEVM_APPLE_VZ_RUNNER");

        let mut root = std::env::temp_dir();
        root.push(format!(
            "bridgevm-api-fast-spawn-blocker-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = VmStore::new(root);
        let manifest = VmManifest::new(
            "fast-linux",
            VmMode::Fast,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "arm64".to_string(),
            },
            "80GiB",
        );
        handle_request(&store, BridgeVmRequest::CreateVm { manifest })
            .into_result()
            .unwrap();

        let error = handle_request(
            &store,
            BridgeVmRequest::RunBackend {
                name: "fast-linux".to_string(),
                spawn: true,
            },
        )
        .into_result()
        .unwrap_err();

        assert!(
            error.contains("Fast Mode spawn requires BRIDGEVM_APPLE_VZ_RUNNER"),
            "{error}"
        );
        assert!(error.contains("launch blockers:"), "{error}");
        assert!(error.contains("missing-primary-disk"), "{error}");
        assert!(error.contains("apple-vz-runner-unavailable"), "{error}");
        let metadata = store
            .runner_metadata("fast-linux")
            .unwrap()
            .expect("Fast spawn blocker writes dry-run runner metadata");
        assert!(metadata.dry_run);
        assert_eq!(metadata.engine, "lightvm");
        let readiness = metadata
            .launch_readiness
            .expect("Fast Mode runner metadata includes launch readiness");
        assert!(!readiness.ready);
        assert!(readiness
            .blockers
            .iter()
            .any(|blocker| blocker.code == "apple-vz-runner-unavailable"));
    }

    #[test]
    fn handler_stops_dry_run_backend() {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "bridgevm-api-stop-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = VmStore::new(root);
        let manifest = VmManifest::new(
            "legacy",
            VmMode::Compatibility,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "x86_64".to_string(),
            },
            "64GiB",
        );
        handle_request(&store, BridgeVmRequest::CreateVm { manifest })
            .into_result()
            .unwrap();
        handle_request(
            &store,
            BridgeVmRequest::RunBackend {
                name: "legacy".to_string(),
                spawn: false,
            },
        )
        .into_result()
        .unwrap();

        let response = handle_request(
            &store,
            BridgeVmRequest::StopBackend {
                name: "legacy".to_string(),
            },
        )
        .into_result()
        .unwrap();
        assert_eq!(
            response,
            BridgeVmResponse::RunnerStatus {
                metadata: None,
                qmp_supervisor: None
            }
        );
        assert_eq!(store.runner_metadata("legacy").unwrap(), None);
        assert_eq!(
            store.state("legacy").unwrap().state,
            VmRuntimeState::Stopped
        );
    }

    #[test]
    fn handler_restores_snapshot_metadata() {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "bridgevm-api-restore-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = VmStore::new(root);
        let manifest = VmManifest::new(
            "dev",
            VmMode::Fast,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "arm64".to_string(),
            },
            "80GiB",
        );
        handle_request(&store, BridgeVmRequest::CreateVm { manifest })
            .into_result()
            .unwrap();
        let disk = store.prepare_primary_disk("dev").unwrap();
        fs::write(&disk.path, b"fake backing").unwrap();
        handle_request(
            &store,
            BridgeVmRequest::CreateSnapshot {
                vm: "dev".to_string(),
                name: "before-upgrade".to_string(),
                kind: SnapshotKind::Disk,
            },
        )
        .into_result()
        .unwrap();

        let response = handle_request(
            &store,
            BridgeVmRequest::RestoreSnapshot {
                vm: "dev".to_string(),
                name: "before-upgrade".to_string(),
            },
        )
        .into_result()
        .unwrap();
        let BridgeVmResponse::SnapshotRestored { restore } = response else {
            panic!("expected snapshot restore response");
        };
        assert_eq!(restore.snapshot, "before-upgrade");
        assert_eq!(restore.restored_state, VmRuntimeState::Stopped);
        assert!(restore.active_disk.is_some());
    }

    #[test]
    fn handler_reports_snapshot_preflight_status_from_guest_tools_runtime() {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "bridgevm-api-snapshot-preflight-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = VmStore::new(root);
        let manifest = VmManifest::new(
            "dev",
            VmMode::Fast,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "arm64".to_string(),
            },
            "80GiB",
        );
        handle_request(&store, BridgeVmRequest::CreateVm { manifest })
            .into_result()
            .unwrap();
        store
            .write_guest_tools_runtime_metadata(
                "dev",
                &GuestToolsRuntimeMetadata {
                    connected: true,
                    guest_os: Some("linux".to_string()),
                    agent_version: Some("0.1.0".to_string()),
                    capabilities: vec!["fs-freeze".to_string(), "fs-thaw".to_string()],
                    last_heartbeat_at_unix: Some(1),
                    guest_ip_addresses: Vec::new(),
                    shared_folders: Vec::new(),
                    metrics: None,
                    last_command_result: None,
                    agent_update: None,
                    clipboard: None,
                    updated_at_unix: 2,
                },
            )
            .unwrap();

        let response = handle_request(
            &store,
            BridgeVmRequest::SnapshotPreflightStatus {
                name: "dev".to_string(),
                consistency: SnapshotConsistency::ApplicationConsistent,
            },
        )
        .into_result()
        .unwrap();
        let BridgeVmResponse::SnapshotPreflightStatus { preflight } = response else {
            panic!("expected snapshot preflight response");
        };
        assert_eq!(preflight.vm, "dev");
        assert_eq!(
            preflight.consistency,
            SnapshotConsistency::ApplicationConsistent
        );
        assert!(preflight.guest_tools_connected);
        assert_eq!(
            preflight.capabilities,
            vec!["fs-freeze".to_string(), "fs-thaw".to_string()]
        );
        assert!(!preflight.backend_freeze_thaw_supported);
        assert!(!preflight.ready);
        assert_eq!(
            preflight.blockers[0].code,
            "backend-freeze-thaw-unavailable"
        );
    }

    #[test]
    fn handler_restores_suspend_snapshot_metadata() {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "bridgevm-api-suspend-restore-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = VmStore::new(root);
        let manifest = VmManifest::new(
            "dev",
            VmMode::Fast,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "arm64".to_string(),
            },
            "80GiB",
        );
        handle_request(&store, BridgeVmRequest::CreateVm { manifest })
            .into_result()
            .unwrap();
        store
            .transition_state("dev", VmRuntimeState::Running)
            .unwrap();
        store
            .transition_state("dev", VmRuntimeState::Suspended)
            .unwrap();
        handle_request(
            &store,
            BridgeVmRequest::CreateSnapshot {
                vm: "dev".to_string(),
                name: "paused".to_string(),
                kind: SnapshotKind::Suspend,
            },
        )
        .into_result()
        .unwrap();
        let image = store
            .snapshot_suspend_image_metadata("dev", "paused")
            .unwrap()
            .unwrap();
        fs::write(&image.image_path, b"fake suspend image").unwrap();

        let response = handle_request(
            &store,
            BridgeVmRequest::RestoreSnapshot {
                vm: "dev".to_string(),
                name: "paused".to_string(),
            },
        )
        .into_result()
        .unwrap();
        let BridgeVmResponse::SnapshotRestored { restore } = response else {
            panic!("expected snapshot restore response");
        };
        assert_eq!(restore.snapshot, "paused");
        assert_eq!(restore.restored_state, VmRuntimeState::Suspended);
        assert!(restore.active_disk.is_none());
        assert!(restore.suspend_image.unwrap().image_exists);
    }

    #[test]
    fn handler_creates_application_consistent_snapshot_preflight_metadata() {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "bridgevm-api-application-consistent-snapshot-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = VmStore::new(root);
        let manifest = VmManifest::new(
            "dev",
            VmMode::Fast,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "arm64".to_string(),
            },
            "80GiB",
        );
        handle_request(&store, BridgeVmRequest::CreateVm { manifest })
            .into_result()
            .unwrap();
        store
            .write_guest_tools_runtime_metadata(
                "dev",
                &GuestToolsRuntimeMetadata {
                    connected: true,
                    guest_os: Some("linux".to_string()),
                    agent_version: Some("1.0.0".to_string()),
                    capabilities: vec![
                        "heartbeat".to_string(),
                        "fs-freeze".to_string(),
                        "fs-thaw".to_string(),
                    ],
                    last_heartbeat_at_unix: Some(1),
                    guest_ip_addresses: Vec::new(),
                    shared_folders: Vec::new(),
                    metrics: None,
                    last_command_result: None,
                    agent_update: None,
                    clipboard: None,
                    updated_at_unix: 2,
                },
            )
            .unwrap();

        let response = handle_request(
            &store,
            BridgeVmRequest::CreateSnapshot {
                vm: "dev".to_string(),
                name: "app-ready".to_string(),
                kind: SnapshotKind::ApplicationConsistent,
            },
        )
        .into_result()
        .unwrap();
        let BridgeVmResponse::Snapshot {
            snapshot,
            disk,
            application_consistent_preflight,
        } = response
        else {
            panic!("expected snapshot response");
        };
        let preflight = application_consistent_preflight.expect("preflight metadata");

        assert_eq!(snapshot.kind, SnapshotKind::ApplicationConsistent);
        assert!(disk.is_none());
        assert!(preflight.connected);
        assert!(preflight.ready);
        assert!(preflight.missing_capabilities.is_empty());
        assert_eq!(preflight.runtime_updated_at_unix, Some(2));
    }

    #[test]
    fn handler_exports_vm_bundle() {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "bridgevm-api-export-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let output = root.join("exports").join("dev.vmbridge");
        let store = VmStore::new(root);
        let manifest = VmManifest::new(
            "dev",
            VmMode::Fast,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "arm64".to_string(),
            },
            "80GiB",
        );
        handle_request(&store, BridgeVmRequest::CreateVm { manifest })
            .into_result()
            .unwrap();

        let response = handle_request(
            &store,
            BridgeVmRequest::ExportVm {
                name: "dev".to_string(),
                output: output.clone(),
            },
        )
        .into_result()
        .unwrap();
        let BridgeVmResponse::Exported { export } = response else {
            panic!("expected export response");
        };
        assert_eq!(export.vm, "dev");
        assert_eq!(export.archive_format, "directory");
        assert!(export.manifest_preserved);
        assert!(export.copied_files.contains(&"manifest.yaml".to_string()));
        assert!(output.join("manifest.yaml").exists());
    }

    #[test]
    fn handler_preserves_export_hardening_error_messages() {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "bridgevm-api-export-hardening-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = VmStore::new(root);
        let manifest = VmManifest::new(
            "dev",
            VmMode::Fast,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "arm64".to_string(),
            },
            "80GiB",
        );
        handle_request(&store, BridgeVmRequest::CreateVm { manifest })
            .into_result()
            .unwrap();
        let bundle = store.bundle_path("dev");

        let error = handle_request(
            &store,
            BridgeVmRequest::ExportVm {
                name: "dev".to_string(),
                output: bundle.join("exports").join("dev.vmbridge"),
            },
        )
        .into_result()
        .unwrap_err();

        assert!(
            error.contains("export output must not be the source bundle or inside it"),
            "unexpected export error: {error}"
        );
    }

    #[test]
    fn handler_imports_vm_bundle() {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "bridgevm-api-import-source-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let output = root.join("exports").join("dev.vmbridge");
        let source = VmStore::new(root);
        let manifest = VmManifest::new(
            "dev",
            VmMode::Fast,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "arm64".to_string(),
            },
            "80GiB",
        );
        handle_request(&source, BridgeVmRequest::CreateVm { manifest })
            .into_result()
            .unwrap();
        handle_request(
            &source,
            BridgeVmRequest::ExportVm {
                name: "dev".to_string(),
                output: output.clone(),
            },
        )
        .into_result()
        .unwrap();

        let mut import_root = std::env::temp_dir();
        import_root.push(format!(
            "bridgevm-api-import-target-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let target = VmStore::new(import_root);
        let response = handle_request(
            &target,
            BridgeVmRequest::ImportVm {
                input: output,
                name: Some("dev-copy".to_string()),
            },
        )
        .into_result()
        .unwrap();
        let BridgeVmResponse::Imported { import } = response else {
            panic!("expected import response");
        };
        assert_eq!(import.vm, "dev-copy");
        assert_eq!(import.original_name, "dev");
        assert_eq!(import.requested_name.as_deref(), Some("dev-copy"));
        assert!(import.manifest_identity_rewritten);
        assert_eq!(record_for(&target, "dev-copy").unwrap().name, "dev-copy");
    }

    #[test]
    fn handler_restarts_vm_through_stop_then_start_state() {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "bridgevm-api-restart-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = VmStore::new(root);
        let manifest = VmManifest::new(
            "dev",
            VmMode::Fast,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "arm64".to_string(),
            },
            "80GiB",
        );
        handle_request(&store, BridgeVmRequest::CreateVm { manifest })
            .into_result()
            .unwrap();
        handle_request(
            &store,
            BridgeVmRequest::TransitionVm {
                name: "dev".to_string(),
                state: VmRuntimeState::Running,
            },
        )
        .into_result()
        .unwrap();

        let response = handle_request(
            &store,
            BridgeVmRequest::RestartVm {
                name: "dev".to_string(),
            },
        )
        .into_result()
        .unwrap();
        let BridgeVmResponse::State { name, metadata } = response else {
            panic!("expected restart state response");
        };

        assert_eq!(name, "dev");
        assert_eq!(metadata.state, VmRuntimeState::Running);
        assert_eq!(store.state("dev").unwrap().state, VmRuntimeState::Running);
        assert!(store.runner_metadata("dev").unwrap().is_none());
    }

    #[test]
    fn handler_clones_vm_bundle_with_new_manifest_identity() {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "bridgevm-api-clone-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = VmStore::new(root);
        let manifest = VmManifest::new(
            "dev",
            VmMode::Fast,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "arm64".to_string(),
            },
            "80GiB",
        );
        handle_request(&store, BridgeVmRequest::CreateVm { manifest })
            .into_result()
            .unwrap();

        let response = handle_request(
            &store,
            BridgeVmRequest::CloneVm {
                name: "dev".to_string(),
                new_name: "dev-copy".to_string(),
                linked: false,
            },
        )
        .into_result()
        .unwrap();
        let BridgeVmResponse::Cloned { clone } = response else {
            panic!("expected clone response");
        };

        assert_eq!(clone.vm, "dev-copy");
        assert!(clone.output.join("manifest.yaml").exists());
        assert!(clone.output.join("metadata").join("clone.json").exists());
        let (clone_bundle, manifest) = store.get_vm("dev-copy").unwrap();
        assert_eq!(manifest.name, "dev-copy");
        assert_eq!(manifest.network.hostname, "dev-copy.bridgevm.local");

        // The clone is a distinct VM and the source is left unchanged.
        let (source_bundle, source_manifest) = store.get_vm("dev").unwrap();
        assert_eq!(source_manifest.name, "dev");
        assert_eq!(source_manifest.network.hostname, "dev.bridgevm.local");
        assert_ne!(source_bundle, clone_bundle);
        assert_eq!(
            store.state("dev-copy").unwrap().state,
            VmRuntimeState::Stopped
        );
    }

    #[test]
    fn handler_preserves_import_hardening_error_messages() {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "bridgevm-api-import-hardening-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = VmStore::new(root);
        let manifest = VmManifest::new(
            "dev",
            VmMode::Fast,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "arm64".to_string(),
            },
            "80GiB",
        );
        handle_request(&store, BridgeVmRequest::CreateVm { manifest })
            .into_result()
            .unwrap();

        let error = handle_request(
            &store,
            BridgeVmRequest::ImportVm {
                input: store.bundle_path("dev"),
                name: None,
            },
        )
        .into_result()
        .unwrap_err();

        assert!(
            error.contains("import input conflicts with the destination store"),
            "unexpected import error: {error}"
        );
    }

    // Serialize tests that mutate the process-global BRIDGEVM_APPLE_VZ_RUNNER
    // env var so parallel test execution does not race on the gate.
    static APPLE_VZ_RUNNER_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    struct EnvVarGuard {
        key: &'static str,
        previous: Option<std::ffi::OsString>,
    }

    impl EnvVarGuard {
        fn capture(key: &'static str) -> Self {
            Self {
                key,
                previous: std::env::var_os(key),
            }
        }

        fn set(key: &'static str, value: &str) -> Self {
            let guard = Self::capture(key);
            std::env::set_var(key, value);
            guard
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            match &self.previous {
                Some(value) => std::env::set_var(self.key, value),
                None => std::env::remove_var(self.key),
            }
        }
    }

    fn fast_test_store(test: &str) -> (VmStore, String) {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "bridgevm-api-{test}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = VmStore::new(root);
        let name = "fast-cold".to_string();
        let manifest = VmManifest::new(
            &name,
            VmMode::Fast,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "arm64".to_string(),
            },
            "80GiB",
        );
        handle_request(&store, BridgeVmRequest::CreateVm { manifest })
            .into_result()
            .unwrap();
        (store, name)
    }

    fn stage_ready_fast_linux_kernel_vm(store: &VmStore, name: &str) -> PathBuf {
        let (bundle, mut manifest) = store.get_vm(name).unwrap();
        std::fs::create_dir_all(bundle.join("disks")).unwrap();
        std::fs::create_dir_all(bundle.join("boot")).unwrap();
        std::fs::write(bundle.join("disks/root.raw"), b"raw disk placeholder").unwrap();
        std::fs::write(bundle.join("boot/vmlinuz"), b"kernel placeholder").unwrap();

        manifest.storage.primary.path = "disks/root.raw".to_string();
        manifest.storage.primary.format = "raw".to_string();
        manifest.boot = Some(bridgevm_config::Boot {
            mode: BootMode::LinuxKernel,
            installer_image: None,
            kernel_path: Some("boot/vmlinuz".to_string()),
            initrd_path: None,
            kernel_command_line: Some("console=hvc0 root=/dev/vda".to_string()),
            macos_restore_image: None,
        });
        manifest.write(&bundle.join("manifest.yaml")).unwrap();
        bundle
    }

    #[cfg(unix)]
    fn write_executable(path: &Path, contents: &str) {
        use std::os::unix::fs::PermissionsExt;

        std::fs::write(path, contents).unwrap();
        let mut permissions = std::fs::metadata(path).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(path, permissions).unwrap();
    }

    #[cfg(unix)]
    fn process_group_id(pid: u32) -> Option<u32> {
        let output = Command::new("ps")
            .args(["-p", &pid.to_string(), "-o", "pgid="])
            .stderr(Stdio::null())
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        String::from_utf8_lossy(&output.stdout).trim().parse().ok()
    }

    #[test]
    fn handler_reapplies_runtime_resources_for_background_fast_vm() {
        let _battery = EnvVarGuard::set("BRIDGEVM_FORCE_ON_BATTERY", "0");
        let (store, name) = fast_test_store("runtime-resource-policy");
        store
            .transition_state(&name, VmRuntimeState::Running)
            .unwrap();
        store
            .write_runner_metadata(
                &name,
                &RunnerMetadata {
                    engine: "lightvm".to_string(),
                    pid: Some(42),
                    command: vec!["lightvm-runner".to_string()],
                    log_path: PathBuf::from("logs/lightvm.log"),
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

        let response = handle_request(
            &store,
            BridgeVmRequest::ReapplyRuntimeResources {
                name: name.clone(),
                visibility: RuntimeResourceVisibility::Background,
            },
        )
        .into_result()
        .unwrap();
        let BridgeVmResponse::RuntimeResourcePolicy { policy } = response else {
            panic!("expected runtime resource policy");
        };

        assert_eq!(policy.vm, name);
        assert_eq!(policy.visibility, RuntimeResourceVisibility::Background);
        assert_eq!(policy.state, VmRuntimeState::Running);
        assert!(!policy.on_battery);
        assert_eq!(policy.memory, "2048");
        assert_eq!(policy.cpu, "1");
        assert_eq!(policy.display_fps_cap, "10");
        assert!(!policy.live_applied);
        assert!(!policy.runtime_control_acknowledged);
        assert_eq!(
            policy.live_apply_blockers[0].code,
            "runtime-control-unavailable"
        );
        assert_eq!(
            store
                .runtime_resource_policy_metadata(&policy.vm)
                .unwrap()
                .as_ref(),
            Some(&policy)
        );
    }

    #[test]
    fn handler_acknowledges_runtime_policy_when_display_control_reads_it() {
        let _battery = EnvVarGuard::set("BRIDGEVM_FORCE_ON_BATTERY", "0");
        let (store, name) = fast_test_store("runtime-resource-policy-ack");
        let socket_path = {
            let mut path = PathBuf::from("/tmp");
            path.push(format!(
                "bridgevm-api-policy-ack-{}-{}.sock",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            path
        };
        let listener = std::os::unix::net::UnixListener::bind(&socket_path).unwrap();
        let server = std::thread::spawn({
            let expected_name = name.clone();
            move || {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = String::new();
                BufReader::new(stream.try_clone().unwrap())
                    .read_line(&mut request)
                    .unwrap();
                let request: serde_json::Value = serde_json::from_str(&request).unwrap();
                assert_eq!(
                    request.get("command").and_then(serde_json::Value::as_str),
                    Some("policy")
                );
                stream
                    .write_all(
                        serde_json::json!({
                            "ok": true,
                            "policy": {
                                "vm": expected_name,
                                "visibility": "background",
                                "display_fps_cap": "10"
                            },
                            "supported_commands": ["status", "stop", "policy", "pacing"]
                        })
                        .to_string()
                        .as_bytes(),
                    )
                    .unwrap();
                stream.write_all(b"\n").unwrap();
            }
        });
        store
            .transition_state(&name, VmRuntimeState::Running)
            .unwrap();
        store
            .write_runner_metadata(
                &name,
                &RunnerMetadata {
                    engine: "lightvm".to_string(),
                    pid: Some(42),
                    command: vec!["lightvm-runner".to_string()],
                    log_path: PathBuf::from("logs/lightvm.log"),
                    started_at_unix: now_unix(),
                    dry_run: false,
                    launch_spec_path: None,
                    guest_tools: None,
                    disk: None,
                    active_disk: None,
                    launch_readiness: None,
                    runtime_control: Some(RuntimeControlMetadata {
                        kind: "apple-vz-display".to_string(),
                        socket_path: socket_path.clone(),
                        commands: vec![
                            "status".to_string(),
                            "stop".to_string(),
                            "policy".to_string(),
                            "pacing".to_string(),
                        ],
                    }),
                },
            )
            .unwrap();

        let response = handle_request(
            &store,
            BridgeVmRequest::ReapplyRuntimeResources {
                name: name.clone(),
                visibility: RuntimeResourceVisibility::Background,
            },
        )
        .into_result()
        .unwrap();
        let BridgeVmResponse::RuntimeResourcePolicy { policy } = response else {
            panic!("expected runtime resource policy");
        };

        assert_eq!(policy.vm, name);
        assert_eq!(policy.visibility, RuntimeResourceVisibility::Background);
        assert!(policy.runtime_control_acknowledged);
        assert!(!policy.live_applied);
        assert_eq!(
            store
                .runtime_resource_policy_metadata(&policy.vm)
                .unwrap()
                .as_ref(),
            Some(&policy)
        );
        server.join().unwrap();
        let _ = fs::remove_file(socket_path);
    }

    #[test]
    fn handler_sends_runtime_control_command_to_recorded_socket() {
        let (store, name) = fast_test_store("runtime-control-command");
        let socket_path = {
            let mut path = PathBuf::from("/tmp");
            path.push(format!(
                "bridgevm-api-rc-{}-{}.sock",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            path
        };
        let listener = std::os::unix::net::UnixListener::bind(&socket_path).unwrap();
        let server = std::thread::spawn({
            let expected_name = name.clone();
            move || {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = String::new();
                BufReader::new(stream.try_clone().unwrap())
                    .read_line(&mut request)
                    .unwrap();
                let request: serde_json::Value = serde_json::from_str(&request).unwrap();
                assert_eq!(
                    request.get("command").and_then(serde_json::Value::as_str),
                    Some("status")
                );
                stream
                    .write_all(
                        serde_json::json!({
                            "ok": true,
                            "vm": expected_name,
                            "state": "running",
                            "stopping": false,
                            "display": {"width": 1024, "height": 768},
                            "supported_commands": ["status", "stop", "policy", "pacing"]
                        })
                        .to_string()
                        .as_bytes(),
                    )
                    .unwrap();
                stream.write_all(b"\n").unwrap();
            }
        });

        store
            .write_runner_metadata(
                &name,
                &RunnerMetadata {
                    engine: "lightvm".to_string(),
                    pid: Some(42),
                    command: vec!["lightvm-runner".to_string()],
                    log_path: PathBuf::from("logs/lightvm.log"),
                    started_at_unix: now_unix(),
                    dry_run: false,
                    launch_spec_path: None,
                    guest_tools: None,
                    disk: None,
                    active_disk: None,
                    launch_readiness: None,
                    runtime_control: Some(RuntimeControlMetadata {
                        kind: "apple-vz-display".to_string(),
                        socket_path: socket_path.clone(),
                        commands: vec![
                            "status".to_string(),
                            "stop".to_string(),
                            "policy".to_string(),
                            "pacing".to_string(),
                        ],
                    }),
                },
            )
            .unwrap();

        let response = handle_request(
            &store,
            BridgeVmRequest::RuntimeControl {
                name: name.clone(),
                command: "status".to_string(),
            },
        )
        .into_result()
        .unwrap();
        let BridgeVmResponse::RuntimeControl { control } = response else {
            panic!("expected runtime control response");
        };

        assert_eq!(control.vm, name);
        assert_eq!(control.kind, "apple-vz-display");
        assert_eq!(control.socket_path, socket_path);
        assert_eq!(control.command, "status");
        assert_eq!(
            control
                .response
                .get("state")
                .and_then(serde_json::Value::as_str),
            Some("running")
        );
        server.join().unwrap();
        let _ = fs::remove_file(socket_path);
    }

    #[test]
    fn runtime_control_reader_accepts_fragmented_response() {
        let socket_path = unique_runtime_control_test_socket("fragmented");
        let listener = std::os::unix::net::UnixListener::bind(&socket_path).unwrap();
        let server = std::thread::spawn({
            let socket_path = socket_path.clone();
            move || {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = String::new();
                BufReader::new(stream.try_clone().unwrap())
                    .read_line(&mut request)
                    .unwrap();
                stream.write_all(br#"{"ok":true,"state":"run"#).unwrap();
                std::thread::sleep(Duration::from_millis(10));
                stream.write_all(b"ning\"}\n").unwrap();
                drop(stream);
                let _ = fs::remove_file(socket_path);
            }
        });

        let response = send_runtime_control_command(&socket_path, "status").unwrap();
        assert_eq!(
            response.get("state").and_then(serde_json::Value::as_str),
            Some("running")
        );
        server.join().unwrap();
    }

    #[test]
    fn runtime_control_reader_rejects_oversized_response() {
        let socket_path = unique_runtime_control_test_socket("oversized");
        let listener = std::os::unix::net::UnixListener::bind(&socket_path).unwrap();
        let server = std::thread::spawn({
            let socket_path = socket_path.clone();
            move || {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = String::new();
                BufReader::new(stream.try_clone().unwrap())
                    .read_line(&mut request)
                    .unwrap();
                let oversized = vec![b'x'; MAX_RUNTIME_CONTROL_RESPONSE_BYTES as usize + 1];
                let _ = stream.write_all(&oversized);
                drop(stream);
                let _ = fs::remove_file(socket_path);
            }
        });

        let error = send_runtime_control_command(&socket_path, "status").unwrap_err();
        assert!(error.contains("exceeded 65536 bytes"), "{error}");
        server.join().unwrap();
    }

    fn unique_runtime_control_test_socket(label: &str) -> PathBuf {
        let mut path = PathBuf::from("/tmp");
        path.push(format!(
            "bridgevm-api-rc-{label}-{}-{}.sock",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        path
    }

    #[test]
    fn handler_reapply_runtime_resources_rejects_stopped_vm() {
        let (store, name) = fast_test_store("runtime-resource-stopped");

        let message = handle_request(
            &store,
            BridgeVmRequest::ReapplyRuntimeResources {
                name,
                visibility: RuntimeResourceVisibility::Foreground,
            },
        )
        .into_result()
        .expect_err("stopped VM should reject runtime resource reapply");

        assert!(message.contains("requires a running VM"));
    }

    #[test]
    fn display_runtime_policy_uses_foreground_visibility() {
        let _battery = EnvVarGuard::set("BRIDGEVM_FORCE_ON_BATTERY", "0");
        let mut manifest = VmManifest::new(
            "fast-display",
            VmMode::Fast,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "arm64".to_string(),
            },
            "80GiB",
        );
        apply_power_aware_fast_resources(&mut manifest);

        let policy = build_runtime_resource_policy_metadata(
            "fast-display",
            &manifest,
            RuntimeResourceVisibility::Foreground,
            VmRuntimeState::Running,
        );

        assert_eq!(policy.visibility, RuntimeResourceVisibility::Foreground);
        assert_eq!(policy.state, VmRuntimeState::Running);
        assert!(!policy.on_battery);
        assert_eq!(policy.memory, "4096");
        assert_eq!(policy.cpu, "2");
        assert_eq!(policy.display_fps_cap, "adaptive");
        assert!(!policy.live_applied);
        assert_eq!(
            policy.live_apply_blockers[0].code,
            "runtime-control-unavailable"
        );
    }

    #[test]
    fn fast_runner_args_cold_start_omits_restore_state() {
        let launch_spec = Path::new("/bundle/metadata/launch-spec.json");
        let runner = Path::new("/helpers/AppleVzRunner");
        let args = fast_runner_args(launch_spec, runner, None, false, None, None, None, None);
        assert_eq!(
            args,
            vec![
                "--launch-spec".to_string(),
                "/bundle/metadata/launch-spec.json".to_string(),
                "--require-ready".to_string(),
                "--launch".to_string(),
                "--apple-vz-runner".to_string(),
                "/helpers/AppleVzRunner".to_string(),
                "--apple-vz-allow-real-start".to_string(),
            ]
        );
        // A cold start never restores saved state.
        assert!(!args.iter().any(|arg| arg == "--apple-vz-restore-state"));
        // and a non-display cold start does not request a display window.
        assert!(!args.iter().any(|arg| arg == "--apple-vz-display"));
    }

    #[test]
    fn fast_runner_args_display_appends_display_flag() {
        let launch_spec = Path::new("/bundle/metadata/launch-spec.json");
        let runner = Path::new("/helpers/AppleVzRunner");
        let args = fast_runner_args(launch_spec, runner, None, true, None, None, None, None);
        assert!(args.iter().any(|arg| arg == "--apple-vz-display"));
        assert!(!args.iter().any(|arg| arg == "--apple-vz-restore-state"));
    }

    #[test]
    fn fast_runner_args_display_appends_display_dimensions() {
        let launch_spec = Path::new("/bundle/metadata/launch-spec.json");
        let runner = Path::new("/helpers/AppleVzRunner");
        let args = fast_runner_args(
            launch_spec,
            runner,
            None,
            true,
            Some((1440, 900)),
            None,
            None,
            None,
        );
        assert_eq!(
            args,
            vec![
                "--launch-spec".to_string(),
                "/bundle/metadata/launch-spec.json".to_string(),
                "--require-ready".to_string(),
                "--launch".to_string(),
                "--apple-vz-runner".to_string(),
                "/helpers/AppleVzRunner".to_string(),
                "--apple-vz-allow-real-start".to_string(),
                "--apple-vz-display".to_string(),
                "--apple-vz-display-width".to_string(),
                "1440".to_string(),
                "--apple-vz-display-height".to_string(),
                "900".to_string(),
            ]
        );
    }

    #[test]
    fn fast_runner_args_display_appends_runtime_control_socket() {
        let launch_spec = Path::new("/bundle/metadata/launch-spec.json");
        let runner = Path::new("/helpers/AppleVzRunner");
        let socket = Path::new("/bundle/run/apple-vz-display-control.sock");
        let args = fast_runner_args(
            launch_spec,
            runner,
            None,
            true,
            None,
            Some(socket),
            None,
            None,
        );

        assert!(args.iter().any(|arg| arg == "--apple-vz-display"));
        assert!(args.windows(2).any(|pair| pair
            == [
                "--apple-vz-runtime-control-socket",
                socket.to_str().unwrap()
            ]));
    }

    #[test]
    fn fast_runner_args_display_appends_proxy_framebuffer_export() {
        let launch_spec = Path::new("/bundle/metadata/launch-spec.json");
        let runner = Path::new("/helpers/AppleVzRunner");
        let framebuffer = Path::new("/bundle/metadata/apple-vz-display-framebuffer.rgba");
        let args = fast_runner_args(
            launch_spec,
            runner,
            None,
            true,
            None,
            None,
            Some(framebuffer),
            Some(250),
        );

        assert!(args.iter().any(|arg| arg == "--apple-vz-display"));
        assert!(args.windows(2).any(|pair| pair
            == [
                "--apple-vz-proxy-framebuffer-rgba-file",
                framebuffer.to_str().unwrap()
            ]));
        assert!(args
            .windows(2)
            .any(|pair| pair == ["--apple-vz-proxy-framebuffer-capture-interval-ms", "250"]));
    }

    #[test]
    fn apple_vz_display_control_socket_path_stays_short_for_macos_unix_sockets() {
        let bundle = Path::new("/Users/example/.bridgevm/vms/runtime-resources-fast.vmbridge");
        let socket = apple_vz_display_control_socket_path(bundle);

        assert_eq!(socket, PathBuf::from("/tmp/bvm-vz-50f391db705184f1.sock"));
        assert!(socket.to_string_lossy().len() < 104);
    }

    #[test]
    fn fast_runner_args_resume_appends_restore_state() {
        let launch_spec = Path::new("/bundle/metadata/launch-spec.json");
        let runner = Path::new("/helpers/AppleVzRunner");
        let state = Path::new("/bundle/metadata/suspend-images/fast.bin");
        let args = fast_runner_args(
            launch_spec,
            runner,
            Some(state),
            false,
            None,
            None,
            None,
            None,
        );
        assert_eq!(
            args,
            vec![
                "--launch-spec".to_string(),
                "/bundle/metadata/launch-spec.json".to_string(),
                "--require-ready".to_string(),
                "--launch".to_string(),
                "--apple-vz-runner".to_string(),
                "/helpers/AppleVzRunner".to_string(),
                "--apple-vz-allow-real-start".to_string(),
                "--apple-vz-restore-state".to_string(),
                "/bundle/metadata/suspend-images/fast.bin".to_string(),
            ]
        );
    }

    #[test]
    fn fast_spawn_without_runner_env_returns_runner_required_error() {
        let _guard = APPLE_VZ_RUNNER_ENV_LOCK.lock().unwrap();
        let _env = EnvVarGuard::capture("BRIDGEVM_APPLE_VZ_RUNNER");
        std::env::remove_var("BRIDGEVM_APPLE_VZ_RUNNER");

        let (store, name) = fast_test_store("fast-spawn-no-env");
        assert!(!apple_vz_runner_configured());

        let error = run_backend(&store, &name, true)
            .expect_err("Fast spawn without BRIDGEVM_APPLE_VZ_RUNNER must stay blocked");
        assert!(
            error.contains("Fast Mode spawn requires BRIDGEVM_APPLE_VZ_RUNNER"),
            "unexpected error: {error}"
        );
        assert!(
            error.contains("apple-vz-runner-unavailable"),
            "unexpected error: {error}"
        );

        // Back-compat: dry-run runner metadata is still written.
        let metadata = store
            .runner_metadata(&name)
            .unwrap()
            .expect("dry-run runner metadata is written when the env is unset");
        assert!(metadata.dry_run);
        assert!(metadata.pid.is_none());
        assert_eq!(metadata.engine, "lightvm");

        let _ = std::fs::remove_dir_all(store.root());
    }

    #[cfg(unix)]
    #[test]
    fn display_fast_backend_spawns_detached_runner_that_survives_return() {
        if !(cfg!(target_os = "macos") && cfg!(target_arch = "aarch64")) {
            return;
        }

        let _guard = APPLE_VZ_RUNNER_ENV_LOCK.lock().unwrap();
        let (store, name) = fast_test_store("display-spawn-detached");
        let bundle = stage_ready_fast_linux_kernel_vm(&store, &name);
        let helper_dir = store.root().join("helpers");
        std::fs::create_dir_all(&helper_dir).unwrap();
        let fake_lightvm_runner = helper_dir.join("lightvm-runner");
        let fake_apple_vz_runner = helper_dir.join("AppleVzRunner");
        let args_file = store.root().join("fake-lightvm-args.txt");
        let env_file = store.root().join("fake-lightvm-env.txt");

        write_executable(&fake_apple_vz_runner, "#!/bin/sh\nexit 0\n");
        write_executable(
            &fake_lightvm_runner,
            r#"#!/bin/sh
printf '%s\n' "$@" > "$BRIDGEVM_FAKE_RUNNER_ARGS"
printf '%s\n' "$BRIDGEVM_APPLE_VZ_ALLOW_REAL_START" > "$BRIDGEVM_FAKE_RUNNER_ENV"
exec sleep 60
"#,
        );

        let _apple_runner_env = EnvVarGuard::set(
            "BRIDGEVM_APPLE_VZ_RUNNER",
            fake_apple_vz_runner.to_str().unwrap(),
        );
        let _lightvm_runner_env = EnvVarGuard::set(
            "BRIDGEVM_LIGHTVM_RUNNER",
            fake_lightvm_runner.to_str().unwrap(),
        );
        let _fake_args_env =
            EnvVarGuard::set("BRIDGEVM_FAKE_RUNNER_ARGS", args_file.to_str().unwrap());
        let _fake_env_env =
            EnvVarGuard::set("BRIDGEVM_FAKE_RUNNER_ENV", env_file.to_str().unwrap());

        let metadata = display_fast_backend_with_size(&store, &name, Some((1440, 900))).unwrap();
        let pid = metadata
            .pid
            .expect("display spawn records the detached runner pid");
        for _ in 0..200 {
            if args_file.exists() && process_is_alive(pid) {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }

        assert!(
            process_is_alive(pid),
            "display runner pid {pid} should survive API return"
        );
        assert_eq!(
            process_group_id(pid),
            Some(pid),
            "display runner should launch in its own process group"
        );
        assert!(metadata
            .command
            .iter()
            .any(|arg| arg == "--apple-vz-display"));
        assert!(metadata
            .command
            .windows(2)
            .any(|pair| pair == ["--apple-vz-display-width", "1440"]));
        assert!(metadata
            .command
            .windows(2)
            .any(|pair| pair == ["--apple-vz-display-height", "900"]));
        let runtime_control = metadata
            .runtime_control
            .as_ref()
            .expect("display spawn records runtime-control metadata");
        assert_eq!(
            runtime_control.socket_path,
            apple_vz_display_control_socket_path(&bundle)
        );

        let args = std::fs::read_to_string(&args_file).unwrap();
        assert!(args.contains("--apple-vz-display\n"), "{args}");
        assert!(args.contains("--apple-vz-display-width\n1440\n"), "{args}");
        assert!(args.contains("--apple-vz-display-height\n900\n"), "{args}");
        assert!(
            args.contains(
                apple_vz_display_framebuffer_rgba_path(&bundle)
                    .to_str()
                    .unwrap()
            ),
            "{args}"
        );
        assert_eq!(std::fs::read_to_string(&env_file).unwrap().trim(), "1");

        let _ = signal_process(pid, "TERM");
        let _ = std::fs::remove_dir_all(store.root());
    }

    #[test]
    fn apple_vz_runner_configured_reflects_env() {
        let _guard = APPLE_VZ_RUNNER_ENV_LOCK.lock().unwrap();
        let _env = EnvVarGuard::capture("BRIDGEVM_APPLE_VZ_RUNNER");

        std::env::remove_var("BRIDGEVM_APPLE_VZ_RUNNER");
        assert!(!apple_vz_runner_configured());

        std::env::set_var("BRIDGEVM_APPLE_VZ_RUNNER", "/helpers/AppleVzRunner");
        assert!(apple_vz_runner_configured());

        // An empty value does not count as configured.
        std::env::set_var("BRIDGEVM_APPLE_VZ_RUNNER", "");
        assert!(!apple_vz_runner_configured());
    }

    fn unique_test_root(label: &str) -> PathBuf {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "bridgevm-api-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        root
    }

    /// Spawn a long-lived process that is NOT a direct child of the test
    /// process, mirroring production where the recorded backend pid is owned by
    /// launchd/init (not the api caller). This avoids the zombie/defunct state
    /// that a direct `Command` child would enter after SIGTERM (which `kill -0`
    /// still reports as "alive" until reaped). The double-fork reparents the
    /// `sleep` to init; we read back its real pid via a temp file.
    #[cfg(unix)]
    fn spawn_detached_sleep() -> u32 {
        let pid_file = unique_test_root("detached-sleep-pid");
        // Shell double-fork: the outer `sh` exits immediately, the backgrounded
        // subshell's `sleep` gets reparented to init, and we record its pid.
        let script = format!(
            "( sleep 300 </dev/null >/dev/null 2>&1 & printf '%d' \"$!\" > '{}' ) &",
            pid_file.display()
        );
        let status = Command::new("sh")
            .arg("-c")
            .arg(&script)
            .status()
            .expect("failed to spawn detached sleep");
        assert!(status.success());

        // The pid file is written by the backgrounded subshell; poll for it.
        let mut pid = None;
        for _ in 0..200 {
            if let Ok(contents) = std::fs::read_to_string(&pid_file) {
                if let Ok(parsed) = contents.trim().parse::<u32>() {
                    if parsed != 0 {
                        pid = Some(parsed);
                        break;
                    }
                }
            }
            thread::sleep(Duration::from_millis(10));
        }
        let _ = std::fs::remove_file(&pid_file);
        let pid = pid.expect("detached sleep pid was not recorded");
        // Wait until the detached process is actually alive before returning.
        for _ in 0..200 {
            if process_is_alive(pid) {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        pid
    }

    #[test]
    fn parse_ps_etime_handles_all_field_widths() {
        assert_eq!(parse_ps_etime("00:05"), Some(5));
        assert_eq!(parse_ps_etime("  01:30 "), Some(90));
        assert_eq!(parse_ps_etime("01:02:03"), Some(3723));
        assert_eq!(
            parse_ps_etime("2-03:04:05"),
            Some(2 * 86_400 + 3 * 3_600 + 4 * 60 + 5)
        );
        assert_eq!(parse_ps_etime("garbage"), None);
        assert_eq!(parse_ps_etime(""), None);
    }

    #[cfg(unix)]
    #[test]
    fn terminate_recorded_process_kills_live_child() {
        let pid = spawn_detached_sleep();
        assert!(process_is_alive(pid));

        let outcome = terminate_recorded_process(
            pid,
            now_unix(),
            Duration::from_secs(STOP_TERMINATION_GRACE_SECONDS),
        )
        .unwrap();
        // Release gate: the process is terminated. `sleep` normally exits on
        // SIGTERM (ExitedAfterTerm), but a reparented-to-init process can be
        // reaped slightly after the grace window, in which case the SIGKILL
        // fallback (Killed) takes over. Either is a successful termination; what
        // matters is that no process remains. AlreadyGone would mean we never
        // observed it live, which this test rules out via the assert above.
        assert_ne!(outcome, ProcessTerminationOutcome::AlreadyGone);
        // Poll briefly: init/launchd reaps the reparented process asynchronously.
        let mut gone = false;
        for _ in 0..200 {
            if !process_is_alive(pid) {
                gone = true;
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        assert!(gone, "process {pid} should be gone after termination");
    }

    #[cfg(unix)]
    #[test]
    fn terminate_recorded_process_is_noop_for_dead_pid() {
        let pid = spawn_detached_sleep();
        signal_process(pid, "KILL").unwrap();
        // Wait for the detached process to fully exit (init reaps it).
        for _ in 0..200 {
            if !process_is_alive(pid) {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        let outcome = terminate_recorded_process(
            pid,
            now_unix(),
            Duration::from_secs(STOP_TERMINATION_GRACE_SECONDS),
        )
        .unwrap();
        assert_eq!(outcome, ProcessTerminationOutcome::AlreadyGone);
    }

    #[cfg(unix)]
    #[test]
    fn stop_backend_terminates_recorded_child_process() {
        let store = VmStore::new(unique_test_root("stop-kills-child"));
        let manifest = VmManifest::new(
            "fast-linux",
            VmMode::Fast,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "arm64".to_string(),
            },
            "80GiB",
        );
        handle_request(&store, BridgeVmRequest::CreateVm { manifest })
            .into_result()
            .unwrap();
        store
            .transition_state("fast-linux", VmRuntimeState::Running)
            .unwrap();

        let pid = spawn_detached_sleep();
        let bundle = store.bundle_path("fast-linux");
        let runner = RunnerMetadata {
            engine: "lightvm".to_string(),
            pid: Some(pid),
            command: vec!["lightvm-runner".to_string()],
            log_path: bundle.join("logs").join("runner.log"),
            started_at_unix: now_unix(),
            dry_run: false,
            launch_spec_path: None,
            guest_tools: None,
            disk: None,
            active_disk: None,
            launch_readiness: None,
            runtime_control: None,
        };
        store.write_runner_metadata("fast-linux", &runner).unwrap();

        let result = stop_backend(&store, "fast-linux").unwrap();
        assert!(result.is_none());
        // Release gate: no VM process remains after stop.
        assert!(!process_is_alive(pid));
        // State cleared.
        assert_eq!(
            store.state("fast-linux").unwrap().state,
            VmRuntimeState::Stopped
        );
        assert!(store.runner_metadata("fast-linux").unwrap().is_none());
        let _ = std::fs::remove_dir_all(store.root());
    }

    #[test]
    fn stop_backend_leaves_dry_run_vm_as_metadata_only() {
        let store = VmStore::new(unique_test_root("stop-dry-run"));
        let manifest = VmManifest::new(
            "fast-linux",
            VmMode::Fast,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "arm64".to_string(),
            },
            "80GiB",
        );
        handle_request(&store, BridgeVmRequest::CreateVm { manifest })
            .into_result()
            .unwrap();
        store
            .transition_state("fast-linux", VmRuntimeState::Running)
            .unwrap();

        let bundle = store.bundle_path("fast-linux");
        let runner = RunnerMetadata {
            engine: "lightvm".to_string(),
            pid: None,
            command: vec!["lightvm-runner".to_string()],
            log_path: bundle.join("logs").join("runner.log"),
            started_at_unix: now_unix(),
            dry_run: true,
            launch_spec_path: None,
            guest_tools: None,
            disk: None,
            active_disk: None,
            launch_readiness: None,
            runtime_control: None,
        };
        store.write_runner_metadata("fast-linux", &runner).unwrap();

        // No real pid -> no termination attempted; metadata-only stop succeeds.
        let result = stop_backend(&store, "fast-linux").unwrap();
        assert!(result.is_none());
        assert_eq!(
            store.state("fast-linux").unwrap().state,
            VmRuntimeState::Stopped
        );
        assert!(store.runner_metadata("fast-linux").unwrap().is_none());
        let _ = std::fs::remove_dir_all(store.root());
    }

    #[test]
    fn compatibility_resume_command_appends_loadvm_tag() {
        let manifest = VmManifest::new(
            "compat",
            VmMode::Compatibility,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "x86_64".to_string(),
            },
            "40GiB",
        );
        let bundle = unique_test_root("compat-resume-cmd");
        let command = build_compatibility_resume_command(&manifest, &bundle).unwrap();
        // The last two args must be `-loadvm <tag>`.
        let tail = &command.args[command.args.len() - 2..];
        assert_eq!(
            tail,
            &["-loadvm".to_string(), "bridgevm-suspend".to_string()]
        );
    }

    #[test]
    fn compatibility_suspend_requires_running_qmp_socket() {
        let store = VmStore::new(unique_test_root("compat-suspend-no-sock"));
        let manifest = VmManifest::new(
            "compat",
            VmMode::Compatibility,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "x86_64".to_string(),
            },
            "40GiB",
        );
        handle_request(&store, BridgeVmRequest::CreateVm { manifest })
            .into_result()
            .unwrap();
        // No QMP socket present -> suspend should report the socket is unavailable.
        let error = suspend_backend(&store, "compat").unwrap_err();
        assert!(
            error.contains("QMP socket unavailable"),
            "unexpected error: {error}"
        );
        let _ = std::fs::remove_dir_all(store.root());
    }

    #[test]
    fn compatibility_resume_requires_suspend_marker() {
        let store = VmStore::new(unique_test_root("compat-resume-no-marker"));
        let manifest = VmManifest::new(
            "compat",
            VmMode::Compatibility,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "x86_64".to_string(),
            },
            "40GiB",
        );
        handle_request(&store, BridgeVmRequest::CreateVm { manifest })
            .into_result()
            .unwrap();
        let error = resume_backend(&store, "compat").unwrap_err();
        assert!(
            error.contains("no saved Compatibility Mode state to resume from"),
            "unexpected error: {error}"
        );
        let _ = std::fs::remove_dir_all(store.root());
    }

    #[test]
    fn compatibility_resume_load_failure_reports_preserved_snapshot() {
        let bundle = unique_test_root("compat-resume-load-failure");
        let log_path = bundle.join("logs").join("qemu.log");
        std::fs::create_dir_all(log_path.parent().unwrap()).unwrap();
        std::fs::write(&log_path, "qemu loadvm failed\n").unwrap();

        let mut child = Command::new("/bin/sh")
            .args(["-c", "exit 42"])
            .spawn()
            .expect("spawn exiting fake qemu");
        let error = verify_compatibility_resume_loaded(&mut child, &bundle, &log_path).unwrap_err();

        assert!(
            error.contains("Compatibility Mode resume failed: QEMU exited"),
            "unexpected error: {error}"
        );
        assert!(
            error.contains("the suspend snapshot is preserved"),
            "unexpected error: {error}"
        );
        assert!(error.contains(&log_path.display().to_string()));

        let _ = std::fs::remove_dir_all(bundle);
    }

    fn valid_guest_hello(token: &str, capabilities: &[&str]) -> AgentMessage {
        AgentMessage::GuestHello {
            version: PROTOCOL_VERSION,
            guest_os: "linux".to_string(),
            agent_version: Some("1.0.0".to_string()),
            capabilities: capabilities
                .iter()
                .map(|name| AgentCapability {
                    name: (*name).to_string(),
                    version: 1,
                })
                .collect(),
            auth: Some(AgentAuth::ToolsToken {
                token: token.to_string(),
            }),
        }
    }
}
