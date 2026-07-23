//! Split out of lib.rs by responsibility.

use crate::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BridgeVmRequest {
    Doctor,
    ListVms,
    ListTemplates,
    CreateVm {
        manifest: Box<VmManifest>,
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

impl BridgeVmRequest {
    pub fn create_vm(manifest: VmManifest) -> Self {
        Self::CreateVm {
            manifest: Box::new(manifest),
        }
    }
}

#[cfg(test)]
#[path = "request_tests/mod.rs"]
mod tests;
