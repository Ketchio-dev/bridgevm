//! Split out of lib.rs by responsibility.

use crate::*;

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

#[cfg(test)]
#[path = "response_tests/mod.rs"]
mod tests;
