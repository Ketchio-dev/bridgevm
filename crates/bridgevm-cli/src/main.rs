use anyhow::{bail, Context, Result};
use bridgevm_agent_protocol::{AgentEnvelope, AgentMessage};
use bridgevm_api::{
    accept_guest_tools_hello, add_fast_spawn_blocker, add_port, add_share,
    apple_vz_runner_configured, cold_start_fast_backend, display_fast_backend,
    compatibility_launch_readiness_metadata, create_diagnostic_bundle, create_performance_baseline,
    create_performance_sample, download_boot_media, fast_spawn_not_implemented_error,
    guest_tools_linux_command, guest_tools_token, import_boot_media, inspect_boot_media_status,
    inspect_guest_tools_status, launch_readiness_metadata, list_ports, list_shares, open_port_plan,
    plan_boot_media_download, remove_port, remove_share, resume_backend, stop_backend,
    suspend_backend, verify_boot_media, view_vm_log,
    ApplicationConsistentSnapshotExecutionRecord, BootMediaDownloadPlanMetadata,
    BootMediaDownloadResultMetadata, BootMediaImportMetadata, BootMediaKind, BootMediaStatus,
    BootMediaVerificationMetadata, BridgeVmRequest, BridgeVmResponse, DiagnosticBundleMetadata,
    GuestToolsLinuxCommandRecord, GuestToolsLinuxCommandTransport, GuestToolsSessionRecord,
    GuestToolsStatusRecord, GuestToolsTokenRecord, LifecycleAction, LifecyclePlanRecord,
    NetworkPlanRecord, OpenPortPlanRecord, PerformanceBaselineMetadata, PerformanceSampleMetadata,
    PortForwardListRecord, SharedFolderListRecord, SnapshotPreflightStatusRecord, SshPlanRecord,
    VmLogKind, VmLogViewRecord, VmReadinessReport, VmRecord,
};
use bridgevm_apple_vz::{
    build_fast_plan, write_launch_spec_artifact, AppleVzBootSpec, AppleVzPathSpec,
};
use bridgevm_config::{manifest_json_schema_v1, Boot, BootMode, Guest, VmManifest, VmMode};
use bridgevm_core::{
    available_boot_templates, boot_template_by_id, recommend_mode, BootTemplate, GuestChoice,
    ModeRecommendation,
};
use bridgevm_qemu::{
    build_compatibility_command, cont as qmp_cont, is_qmp_status_unavailable, qmp_socket_path,
    query_status, stop as qmp_stop, QemuError,
};
use bridgevm_storage::{
    ApplicationConsistentSnapshotPreflightMetadata, LaunchReadinessMetadata, QmpSupervisorMetadata,
    SnapshotKind, VmManifestMigrationMetadata, VmMetadataRepairMetadata, VmRuntimeState, VmStore,
};
use clap::{Parser, Subcommand, ValueEnum};
use std::{
    env,
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Write},
    os::unix::fs::PermissionsExt,
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    process::{Command as ProcessCommand, Stdio},
};

#[derive(Debug, Parser)]
#[command(name = "bridgevm", about = "BridgeVM developer CLI")]
struct Cli {
    #[command(subcommand)]
    command: Command,
    #[arg(long, global = true, value_name = "PATH")]
    store: Option<PathBuf>,
    #[arg(long, global = true, value_name = "SOCKET")]
    socket: Option<PathBuf>,
}

#[derive(Debug, Subcommand)]
enum Command {
    List,
    Templates,
    Create(CreateArgs),
    Status(VmNameArgs),
    Start(VmNameArgs),
    Stop(VmNameArgs),
    Restart(VmNameArgs),
    Suspend(VmNameArgs),
    Resume(VmNameArgs),
    /// Boot a Fast Mode VM with an embedded graphical display window (local GUI
    /// session only). Requires BRIDGEVM_APPLE_VZ_RUNNER.
    Display(VmNameArgs),
    Delete(DeleteArgs),
    Export(ExportArgs),
    Import(ImportArgs),
    Clone(CloneArgs),
    Diagnostics(DiagnosticsCommand),
    Logs(LogsCommand),
    Performance(PerformanceCommand),
    Metadata(MetadataCommand),
    Snapshot(SnapshotCommand),
    Disk(DiskCommand),
    Port(PortCommand),
    NetworkPlan(VmNameArgs),
    Share(ShareCommand),
    Media(MediaCommand),
    GuestTools(GuestToolsCommand),
    QemuArgs(VmNameArgs),
    PrepareRun(VmNameArgs),
    BootMedia(VmNameArgs),
    Ssh(SshArgs),
    Open(OpenArgs),
    Run(RunArgs),
    Readiness(ReadinessArgs),
    LifecyclePlan(LifecyclePlanArgs),
    QmpSocket(VmNameArgs),
    QmpStatus(VmNameArgs),
    QmpStop(VmNameArgs),
    QmpCont(VmNameArgs),
    RunnerStatus(VmNameArgs),
    Recommend(GuestArgs),
    #[command(subcommand)]
    Store(StoreCommand),
    Doctor,
}

#[derive(Debug, Subcommand)]
enum StoreCommand {
    Doctor,
}

#[derive(Debug, Parser)]
struct CreateArgs {
    name: String,
    #[arg(long, value_name = "ID")]
    template: Option<String>,
    #[arg(long)]
    os: Option<String>,
    #[arg(long)]
    version: Option<String>,
    #[arg(long)]
    arch: Option<String>,
    #[arg(long, value_enum, default_value_t = ModeChoice::Auto)]
    mode: ModeChoice,
    #[arg(long, default_value = "80GiB")]
    disk: String,
    #[arg(long, value_enum)]
    boot_mode: Option<BootModeChoice>,
    #[arg(long, value_name = "PATH")]
    installer_image: Option<String>,
    #[arg(long, value_name = "PATH")]
    kernel_path: Option<String>,
    #[arg(long, value_name = "PATH")]
    initrd_path: Option<String>,
    #[arg(long, value_name = "TEXT")]
    kernel_command_line: Option<String>,
    #[arg(long, value_name = "PATH")]
    macos_restore_image: Option<String>,
}

#[derive(Debug, Parser)]
struct VmNameArgs {
    name: String,
}

#[derive(Debug, Parser)]
struct ReadinessArgs {
    name: String,
    #[arg(long, value_name = "DIR")]
    live_evidence: Option<PathBuf>,
    #[arg(long)]
    record_live_evidence: bool,
    #[arg(long)]
    clear_live_evidence: bool,
}

#[derive(Debug, Parser)]
struct DeleteArgs {
    name: String,
    #[arg(long)]
    metadata_only: bool,
}

#[derive(Debug, Parser)]
struct RunArgs {
    name: String,
    #[arg(long)]
    spawn: bool,
}

#[derive(Debug, Parser)]
struct LifecyclePlanArgs {
    name: String,
    #[arg(long, value_enum)]
    action: LifecycleActionChoice,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum LifecycleActionChoice {
    Suspend,
    Resume,
}

impl From<LifecycleActionChoice> for LifecycleAction {
    fn from(value: LifecycleActionChoice) -> Self {
        match value {
            LifecycleActionChoice::Suspend => LifecycleAction::Suspend,
            LifecycleActionChoice::Resume => LifecycleAction::Resume,
        }
    }
}

#[derive(Debug, Parser)]
struct ExportArgs {
    name: String,
    #[arg(long, value_name = "PATH")]
    output: PathBuf,
}

#[derive(Debug, Parser)]
struct ImportArgs {
    input: PathBuf,
    #[arg(long, value_name = "NAME")]
    name: Option<String>,
}

#[derive(Debug, Parser)]
struct CloneArgs {
    name: String,
    new_name: String,
    #[arg(long)]
    linked: bool,
}

#[derive(Debug, Parser)]
struct DiagnosticsCommand {
    #[command(subcommand)]
    command: DiagnosticsSubcommand,
}

#[derive(Debug, Subcommand)]
enum DiagnosticsSubcommand {
    Bundle(DiagnosticsBundleArgs),
}

#[derive(Debug, Parser)]
struct DiagnosticsBundleArgs {
    vm: String,
    #[arg(long, value_name = "DIR")]
    output: PathBuf,
}

#[derive(Debug, Parser)]
struct LogsCommand {
    #[command(subcommand)]
    command: LogsSubcommand,
}

#[derive(Debug, Subcommand)]
enum LogsSubcommand {
    Qemu(LogViewArgs),
    Serial(LogViewArgs),
}

#[derive(Debug, Parser)]
struct LogViewArgs {
    vm: String,
    #[arg(long, value_name = "BYTES")]
    bytes: Option<u64>,
}

#[derive(Debug, Parser)]
struct PerformanceCommand {
    #[command(subcommand)]
    command: PerformanceSubcommand,
}

#[derive(Debug, Subcommand)]
enum PerformanceSubcommand {
    Baseline(PerformanceBaselineArgs),
    Sample(PerformanceSampleArgs),
}

#[derive(Debug, Parser)]
struct PerformanceBaselineArgs {
    vm: String,
    #[arg(long, value_name = "DIR")]
    output: PathBuf,
}

#[derive(Debug, Parser)]
struct PerformanceSampleArgs {
    vm: String,
    #[arg(long, value_name = "DIR")]
    output: PathBuf,
    #[arg(long, value_name = "BYTES")]
    artifact_bytes: Option<u64>,
    #[arg(long, value_name = "N")]
    iterations: Option<u16>,
    #[arg(long)]
    sync: bool,
}

#[derive(Debug, Parser)]
struct MetadataCommand {
    #[command(subcommand)]
    command: MetadataSubcommand,
}

#[derive(Debug, Subcommand)]
enum MetadataSubcommand {
    Repair(VmNameArgs),
    MigrateManifest(ManifestMigrateArgs),
    ManifestSchema,
    ValidateManifest(ManifestValidateArgs),
}

#[derive(Debug, Parser)]
struct ManifestMigrateArgs {
    name: String,
    #[arg(long)]
    dry_run: bool,
}

#[derive(Debug, Parser)]
struct ManifestValidateArgs {
    #[arg(value_name = "PATH")]
    path: PathBuf,
}

#[derive(Debug, Parser)]
struct SnapshotCommand {
    #[command(subcommand)]
    command: SnapshotSubcommand,
}

#[derive(Debug, Parser)]
struct DiskCommand {
    #[command(subcommand)]
    command: DiskSubcommand,
}

#[derive(Debug, Parser)]
struct PortCommand {
    #[command(subcommand)]
    command: PortSubcommand,
}

#[derive(Debug, Parser)]
struct MediaCommand {
    #[command(subcommand)]
    command: MediaSubcommand,
}

#[derive(Debug, Parser)]
struct GuestToolsCommand {
    #[command(subcommand)]
    command: GuestToolsSubcommand,
}

#[derive(Debug, Subcommand)]
enum DiskSubcommand {
    Prepare(VmNameArgs),
    Create(VmNameArgs),
    Inspect(VmNameArgs),
    Verify(VmNameArgs),
    Compact(VmNameArgs),
}

#[derive(Debug, Subcommand)]
enum PortSubcommand {
    List(VmNameArgs),
    Add(PortForwardArgs),
    Remove(PortForwardArgs),
}

#[derive(Debug, Parser)]
struct PortForwardArgs {
    vm: String,
    #[arg(value_name = "HOST:GUEST")]
    mapping: String,
}

#[derive(Debug, Parser)]
struct ShareCommand {
    #[command(subcommand)]
    command: ShareSubcommand,
}

#[derive(Debug, Subcommand)]
enum ShareSubcommand {
    List(VmNameArgs),
    Add(ShareAddArgs),
    Remove(ShareRemoveArgs),
}

#[derive(Debug, Parser)]
struct ShareAddArgs {
    vm: String,
    name: String,
    #[arg(value_name = "HOST_PATH")]
    host_path: String,
    #[arg(long)]
    read_only: bool,
    #[arg(long, value_name = "TOKEN")]
    host_path_token: Option<String>,
}

#[derive(Debug, Parser)]
struct ShareRemoveArgs {
    vm: String,
    name: String,
}

#[derive(Debug, Parser)]
struct SshArgs {
    vm: String,
    #[arg(long, default_value = "user")]
    user: String,
}

#[derive(Debug, Parser)]
struct OpenArgs {
    vm: String,
    #[arg(value_name = "GUEST_PORT")]
    guest: u16,
    #[arg(long, default_value = "http")]
    scheme: String,
}

#[derive(Debug, Subcommand)]
enum MediaSubcommand {
    Download(MediaDownloadArgs),
    DownloadPlan(MediaDownloadPlanArgs),
    Import(MediaImportArgs),
    Status(VmNameArgs),
    Verify(MediaVerifyArgs),
}

#[derive(Debug, Subcommand)]
enum GuestToolsSubcommand {
    Status(VmNameArgs),
    Token(VmNameArgs),
    LinuxCommand(GuestToolsLinuxCommandArgs),
    AcceptHello(GuestToolsAcceptHelloArgs),
    SendCommand(GuestToolsSendCommandArgs),
    FreezeFilesystem(GuestToolsFreezeFilesystemArgs),
    ThawFilesystem(GuestToolsRequestIdArgs),
    SetClipboard(GuestToolsSetClipboardArgs),
    ResizeDisplay(GuestToolsResizeDisplayArgs),
    MountShare(GuestToolsMountShareArgs),
    MountApprovedShare(GuestToolsMountApprovedShareArgs),
    UnmountShare(GuestToolsUnmountShareArgs),
    FileDropStart(GuestToolsFileDropStartArgs),
    FileDropChunk(GuestToolsFileDropChunkArgs),
    FileDropComplete(GuestToolsFileDropCompleteArgs),
    ListApplications(GuestToolsRequestIdArgs),
    LaunchApplication(GuestToolsIdCommandArgs),
    ListWindows(GuestToolsRequestIdArgs),
    FocusWindow(GuestToolsIdCommandArgs),
    CloseWindow(GuestToolsIdCommandArgs),
    TimeSync(GuestToolsTimeSyncArgs),
}

#[derive(Debug, Parser)]
struct GuestToolsLinuxCommandArgs {
    vm: String,
    #[arg(long, value_enum, default_value_t = GuestToolsLinuxCommandTransportChoice::Device)]
    transport: GuestToolsLinuxCommandTransportChoice,
    #[arg(long, value_name = "PATH")]
    token_file: Option<PathBuf>,
    #[arg(long, value_name = "PATH")]
    device: Option<PathBuf>,
}

#[derive(Debug, Parser)]
struct GuestToolsAcceptHelloArgs {
    vm: String,
    #[arg(long, value_name = "JSON")]
    hello_json: String,
}

#[derive(Debug, Parser)]
struct GuestToolsSendCommandArgs {
    vm: String,
    #[arg(long, value_name = "JSON")]
    envelope_json: String,
}

#[derive(Debug, Parser)]
struct GuestToolsFreezeFilesystemArgs {
    vm: String,
    #[arg(long, value_name = "ID")]
    request_id: Option<String>,
    #[arg(long)]
    timeout_millis: Option<u64>,
}

#[derive(Debug, Parser)]
struct GuestToolsSetClipboardArgs {
    vm: String,
    #[arg(long)]
    text: String,
    #[arg(long, value_name = "ID")]
    request_id: Option<String>,
}

#[derive(Debug, Parser)]
struct GuestToolsResizeDisplayArgs {
    vm: String,
    #[arg(long)]
    width: u32,
    #[arg(long)]
    height: u32,
    #[arg(long)]
    scale: u16,
    #[arg(long, value_name = "ID")]
    request_id: Option<String>,
}

#[derive(Debug, Parser)]
struct GuestToolsMountShareArgs {
    vm: String,
    #[arg(long)]
    name: String,
    #[arg(long, value_name = "TOKEN")]
    host_path_token: String,
    #[arg(long, value_name = "ID")]
    request_id: Option<String>,
}

#[derive(Debug, Parser)]
struct GuestToolsMountApprovedShareArgs {
    vm: String,
    #[arg(long)]
    share: String,
    #[arg(long, value_name = "ID")]
    request_id: Option<String>,
}

#[derive(Debug, Parser)]
struct GuestToolsUnmountShareArgs {
    vm: String,
    #[arg(long)]
    name: String,
    #[arg(long, value_name = "ID")]
    request_id: Option<String>,
}

#[derive(Debug, Parser)]
struct GuestToolsFileDropStartArgs {
    vm: String,
    #[arg(long, value_name = "ID")]
    transfer_id: String,
    #[arg(long)]
    file_name: String,
    #[arg(long)]
    size_bytes: u64,
    #[arg(long, value_name = "ID")]
    request_id: Option<String>,
}

#[derive(Debug, Parser)]
struct GuestToolsFileDropChunkArgs {
    vm: String,
    #[arg(long, value_name = "ID")]
    transfer_id: String,
    #[arg(long)]
    chunk_index: u32,
    #[arg(long)]
    data_base64: String,
    #[arg(long, value_name = "ID")]
    request_id: Option<String>,
}

#[derive(Debug, Parser)]
struct GuestToolsFileDropCompleteArgs {
    vm: String,
    #[arg(long, value_name = "ID")]
    transfer_id: String,
    #[arg(long, value_name = "ID")]
    request_id: Option<String>,
}

#[derive(Debug, Parser)]
struct GuestToolsRequestIdArgs {
    vm: String,
    #[arg(long, value_name = "ID")]
    request_id: Option<String>,
}

#[derive(Debug, Parser)]
struct GuestToolsIdCommandArgs {
    vm: String,
    #[arg(long)]
    id: String,
    #[arg(long, value_name = "ID")]
    request_id: Option<String>,
}

#[derive(Debug, Parser)]
struct GuestToolsTimeSyncArgs {
    vm: String,
    #[arg(long, value_name = "MILLIS")]
    unix_epoch_millis: Option<u64>,
    #[arg(long, value_name = "ID")]
    request_id: Option<String>,
}

#[derive(Debug, Parser)]
struct MediaDownloadArgs {
    vm: String,
    #[arg(long, value_enum)]
    kind: Option<BootMediaKindChoice>,
}

#[derive(Debug, Parser)]
struct MediaDownloadPlanArgs {
    vm: String,
    #[arg(long, value_name = "URL")]
    url: String,
    #[arg(long, value_name = "SHA256")]
    sha256: Option<String>,
    #[arg(long, value_enum)]
    kind: Option<BootMediaKindChoice>,
}

#[derive(Debug, Parser)]
struct MediaImportArgs {
    vm: String,
    #[arg(long, value_name = "PATH")]
    source: PathBuf,
    #[arg(long, value_enum)]
    kind: Option<BootMediaKindChoice>,
}

#[derive(Debug, Parser)]
struct MediaVerifyArgs {
    vm: String,
    #[arg(long, value_name = "SHA256")]
    sha256: String,
    #[arg(long, value_enum)]
    kind: Option<BootMediaKindChoice>,
}

#[derive(Debug, Subcommand)]
enum SnapshotSubcommand {
    Create(SnapshotCreateArgs),
    ExecuteApplicationConsistent(SnapshotApplicationConsistentExecuteArgs),
    DiskCreate(SnapshotDiskCreateArgs),
    Chain(VmNameArgs),
    List(VmNameArgs),
    Restore(SnapshotRestoreArgs),
}

#[derive(Debug, Parser)]
struct SnapshotCreateArgs {
    vm: String,
    name: String,
    #[arg(long, value_enum, default_value_t = SnapshotKindChoice::Disk)]
    kind: SnapshotKindChoice,
}

#[derive(Debug, Parser)]
struct SnapshotApplicationConsistentExecuteArgs {
    vm: String,
    name: String,
    #[arg(long)]
    freeze_timeout_millis: Option<u64>,
}

#[derive(Debug, Parser)]
struct SnapshotDiskCreateArgs {
    vm: String,
    name: String,
}

#[derive(Debug, Parser)]
struct SnapshotRestoreArgs {
    vm: String,
    name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum SnapshotKindChoice {
    Disk,
    Suspend,
    ApplicationConsistent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum GuestToolsLinuxCommandTransportChoice {
    Device,
    Socket,
}

impl From<GuestToolsLinuxCommandTransportChoice> for GuestToolsLinuxCommandTransport {
    fn from(value: GuestToolsLinuxCommandTransportChoice) -> Self {
        match value {
            GuestToolsLinuxCommandTransportChoice::Device => {
                GuestToolsLinuxCommandTransport::Device
            }
            GuestToolsLinuxCommandTransportChoice::Socket => {
                GuestToolsLinuxCommandTransport::Socket
            }
        }
    }
}

#[derive(Debug, Parser)]
struct GuestArgs {
    #[arg(long)]
    os: String,
    #[arg(long)]
    version: Option<String>,
    #[arg(long, default_value = "arm64")]
    arch: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum ModeChoice {
    Auto,
    Fast,
    Compatibility,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum BootModeChoice {
    ExistingDisk,
    LinuxKernel,
    LinuxInstaller,
    WindowsInstaller,
    MacosRestore,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum BootMediaKindChoice {
    InstallerImage,
    Kernel,
    Initrd,
    MacosRestoreImage,
}

impl From<BootModeChoice> for BootMode {
    fn from(value: BootModeChoice) -> Self {
        match value {
            BootModeChoice::ExistingDisk => BootMode::ExistingDisk,
            BootModeChoice::LinuxKernel => BootMode::LinuxKernel,
            BootModeChoice::LinuxInstaller => BootMode::LinuxInstaller,
            BootModeChoice::WindowsInstaller => BootMode::WindowsInstaller,
            BootModeChoice::MacosRestore => BootMode::MacosRestore,
        }
    }
}

impl From<BootMediaKindChoice> for BootMediaKind {
    fn from(value: BootMediaKindChoice) -> Self {
        match value {
            BootMediaKindChoice::InstallerImage => BootMediaKind::InstallerImage,
            BootMediaKindChoice::Kernel => BootMediaKind::Kernel,
            BootMediaKindChoice::Initrd => BootMediaKind::Initrd,
            BootMediaKindChoice::MacosRestoreImage => BootMediaKind::MacosRestoreImage,
        }
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    if let Some(socket) = cli.socket {
        return run_via_daemon(&socket, cli.command);
    }

    let store = cli.store.map(VmStore::new).unwrap_or_else(VmStore::default);

    match cli.command {
        Command::List => list(&store),
        Command::Templates => templates(),
        Command::Create(args) => create(&store, args),
        Command::Status(args) => status(&store, args),
        Command::Start(args) => transition(
            &store,
            args,
            VmRuntimeState::Running,
            "Metadata state recorded for",
        ),
        Command::Stop(args) => stop_backend_local(&store, args),
        Command::Restart(args) => restart_local(&store, args),
        Command::Suspend(args) => suspend_backend_local(&store, args),
        Command::Resume(args) => resume_backend_local(&store, args),
        Command::Display(args) => display_backend_local(&store, args),
        Command::Delete(args) => delete(&store, args),
        Command::Export(args) => export_vm(&store, args),
        Command::Import(args) => import_vm(&store, args),
        Command::Clone(args) => clone_vm(&store, args),
        Command::Diagnostics(args) => diagnostics(&store, args),
        Command::Logs(args) => logs(&store, args),
        Command::Performance(args) => performance(&store, args),
        Command::Metadata(args) => metadata(&store, args),
        Command::Snapshot(args) => snapshot(&store, args),
        Command::Disk(args) => disk(&store, args),
        Command::Port(args) => port(&store, args),
        Command::NetworkPlan(args) => network_plan(&store, args),
        Command::Share(args) => share(&store, args),
        Command::Media(args) => media(&store, args),
        Command::GuestTools(args) => guest_tools(&store, args),
        Command::QemuArgs(args) => qemu_args(&store, args),
        Command::PrepareRun(args) => prepare_run(&store, args),
        Command::BootMedia(args) => boot_media(&store, args),
        Command::Ssh(args) => ssh(&store, args),
        Command::Open(args) => open_port(&store, args),
        Command::Run(args) => run_backend_local(&store, args),
        Command::Readiness(args) => readiness(&store, args),
        Command::LifecyclePlan(args) => lifecycle_plan(&store, args),
        Command::QmpSocket(args) => qmp_socket(&store, args),
        Command::QmpStatus(args) => qmp_status(&store, args),
        Command::QmpStop(args) => qmp_control(&store, args, "stop", qmp_stop),
        Command::QmpCont(args) => qmp_control(&store, args, "cont", qmp_cont),
        Command::RunnerStatus(args) => runner_status(&store, args),
        Command::Recommend(args) => recommend(args),
        Command::Store(StoreCommand::Doctor) => doctor(&store),
        Command::Doctor => doctor(&store),
    }
}

fn run_via_daemon(socket: &Path, command: Command) -> Result<()> {
    let request = request_for(command)?;
    let response = send_request(socket, request)?;
    print_daemon_response(response)
}

fn request_for(command: Command) -> Result<BridgeVmRequest> {
    match command {
        Command::List => Ok(BridgeVmRequest::ListVms),
        Command::Templates => Ok(BridgeVmRequest::ListTemplates),
        Command::Create(args) => {
            let manifest = manifest_for_create(args)?;
            Ok(BridgeVmRequest::CreateVm { manifest })
        }
        Command::Status(args) => Ok(BridgeVmRequest::GetVm { name: args.name }),
        Command::Start(args) => Ok(BridgeVmRequest::TransitionVm {
            name: args.name,
            state: VmRuntimeState::Running,
        }),
        Command::Stop(args) => Ok(BridgeVmRequest::StopBackend { name: args.name }),
        Command::Restart(args) => Ok(BridgeVmRequest::RestartVm { name: args.name }),
        Command::Suspend(args) => Ok(BridgeVmRequest::SuspendBackend { name: args.name }),
        Command::Resume(args) => Ok(BridgeVmRequest::ResumeBackend { name: args.name }),
        Command::Delete(args) => Ok(BridgeVmRequest::DeleteVm {
            name: args.name,
            metadata_only: args.metadata_only,
        }),
        Command::Export(args) => Ok(BridgeVmRequest::ExportVm {
            name: args.name,
            output: args.output,
        }),
        Command::Import(args) => Ok(BridgeVmRequest::ImportVm {
            input: args.input,
            name: args.name,
        }),
        Command::Clone(args) => Ok(BridgeVmRequest::CloneVm {
            name: args.name,
            new_name: args.new_name,
            linked: args.linked,
        }),
        Command::Diagnostics(args) => match args.command {
            DiagnosticsSubcommand::Bundle(args) => Ok(BridgeVmRequest::CreateDiagnosticBundle {
                name: args.vm,
                output: args.output,
            }),
        },
        Command::Logs(args) => match args.command {
            LogsSubcommand::Qemu(args) => Ok(BridgeVmRequest::ViewLogs {
                name: args.vm,
                kind: VmLogKind::Qemu,
                max_bytes: args.bytes,
            }),
            LogsSubcommand::Serial(args) => Ok(BridgeVmRequest::ViewLogs {
                name: args.vm,
                kind: VmLogKind::Serial,
                max_bytes: args.bytes,
            }),
        },
        Command::Performance(args) => match args.command {
            PerformanceSubcommand::Baseline(args) => {
                Ok(BridgeVmRequest::CreatePerformanceBaseline {
                    name: args.vm,
                    output: args.output,
                })
            }
            PerformanceSubcommand::Sample(args) => Ok(BridgeVmRequest::CreatePerformanceSample {
                name: args.vm,
                output: args.output,
                artifact_bytes: args.artifact_bytes,
                iterations: args.iterations,
                sync: args.sync,
            }),
        },
        Command::Metadata(args) => match args.command {
            MetadataSubcommand::Repair(args) => {
                Ok(BridgeVmRequest::RepairMetadata { name: args.name })
            }
            MetadataSubcommand::MigrateManifest(args) => Ok(BridgeVmRequest::MigrateManifest {
                name: args.name,
                dry_run: args.dry_run,
            }),
            MetadataSubcommand::ManifestSchema | MetadataSubcommand::ValidateManifest(_) => {
                bail!("metadata manifest-schema and validate-manifest are local-only commands")
            }
        },
        Command::Snapshot(args) => match args.command {
            SnapshotSubcommand::Create(args) => Ok(BridgeVmRequest::CreateSnapshot {
                vm: args.vm,
                name: args.name,
                kind: args.kind.into(),
            }),
            SnapshotSubcommand::ExecuteApplicationConsistent(args) => {
                Ok(BridgeVmRequest::ExecuteApplicationConsistentSnapshot {
                    vm: args.vm,
                    name: args.name,
                    freeze_timeout_millis: args.freeze_timeout_millis,
                })
            }
            SnapshotSubcommand::List(args) => Ok(BridgeVmRequest::ListSnapshots { vm: args.name }),
            SnapshotSubcommand::Chain(args) => Ok(BridgeVmRequest::SnapshotChain { vm: args.name }),
            SnapshotSubcommand::Restore(args) => Ok(BridgeVmRequest::RestoreSnapshot {
                vm: args.vm,
                name: args.name,
            }),
            SnapshotSubcommand::DiskCreate(args) => Ok(BridgeVmRequest::CreateSnapshotDisk {
                vm: args.vm,
                name: args.name,
            }),
        },
        Command::Disk(args) => match args.command {
            DiskSubcommand::Prepare(args) => Ok(BridgeVmRequest::PrepareDisk { name: args.name }),
            DiskSubcommand::Create(args) => Ok(BridgeVmRequest::CreateDisk { name: args.name }),
            DiskSubcommand::Inspect(args) => Ok(BridgeVmRequest::InspectDisk { name: args.name }),
            DiskSubcommand::Verify(args) => Ok(BridgeVmRequest::VerifyDisk { name: args.name }),
            DiskSubcommand::Compact(args) => Ok(BridgeVmRequest::CompactDisk { name: args.name }),
        },
        Command::Port(args) => match args.command {
            PortSubcommand::List(args) => Ok(BridgeVmRequest::ListPorts { name: args.name }),
            PortSubcommand::Add(args) => {
                let (host, guest) = parse_port_mapping(&args.mapping)?;
                Ok(BridgeVmRequest::AddPort {
                    name: args.vm,
                    host,
                    guest,
                })
            }
            PortSubcommand::Remove(args) => {
                let (host, guest) = parse_port_mapping(&args.mapping)?;
                Ok(BridgeVmRequest::RemovePort {
                    name: args.vm,
                    host,
                    guest,
                })
            }
        },
        Command::NetworkPlan(args) => Ok(BridgeVmRequest::PlanNetwork { name: args.name }),
        Command::Share(args) => match args.command {
            ShareSubcommand::List(args) => Ok(BridgeVmRequest::ListShares { name: args.name }),
            ShareSubcommand::Add(args) => Ok(BridgeVmRequest::AddShare {
                name: args.vm,
                share: args.name,
                host_path: args.host_path,
                read_only: args.read_only,
                host_path_token: args.host_path_token,
            }),
            ShareSubcommand::Remove(args) => Ok(BridgeVmRequest::RemoveShare {
                name: args.vm,
                share: args.name,
            }),
        },
        Command::Media(args) => match args.command {
            MediaSubcommand::Download(args) => Ok(BridgeVmRequest::DownloadBootMedia {
                name: args.vm,
                kind: args.kind.map(Into::into),
            }),
            MediaSubcommand::DownloadPlan(args) => Ok(BridgeVmRequest::PlanBootMediaDownload {
                name: args.vm,
                url: args.url,
                expected_sha256: args.sha256,
                kind: args.kind.map(Into::into),
            }),
            MediaSubcommand::Import(args) => Ok(BridgeVmRequest::ImportBootMedia {
                name: args.vm,
                source: args.source,
                kind: args.kind.map(Into::into),
            }),
            MediaSubcommand::Status(args) => {
                Ok(BridgeVmRequest::InspectBootMediaStatus { name: args.name })
            }
            MediaSubcommand::Verify(args) => Ok(BridgeVmRequest::VerifyBootMedia {
                name: args.vm,
                expected_sha256: args.sha256,
                kind: args.kind.map(Into::into),
            }),
        },
        Command::GuestTools(args) => match args.command {
            GuestToolsSubcommand::Status(args) => {
                Ok(BridgeVmRequest::GuestToolsStatus { name: args.name })
            }
            GuestToolsSubcommand::Token(args) => {
                Ok(BridgeVmRequest::GuestToolsToken { name: args.name })
            }
            GuestToolsSubcommand::LinuxCommand(args) => {
                Ok(BridgeVmRequest::GuestToolsLinuxCommand {
                    name: args.vm,
                    transport: args.transport.into(),
                    token_file: args.token_file,
                    device: args.device,
                })
            }
            GuestToolsSubcommand::AcceptHello(args) => Ok(BridgeVmRequest::GuestToolsAcceptHello {
                name: args.vm,
                envelope: parse_agent_envelope(&args.hello_json)?,
            }),
            GuestToolsSubcommand::SendCommand(args) => Ok(BridgeVmRequest::GuestToolsSendCommand {
                name: args.vm,
                envelope: parse_agent_envelope(&args.envelope_json)?,
            }),
            GuestToolsSubcommand::FreezeFilesystem(args) => {
                Ok(BridgeVmRequest::GuestToolsSendCommand {
                    name: args.vm,
                    envelope: agent_command_envelope(
                        AgentMessage::FreezeFilesystem {
                            timeout_millis: args.timeout_millis,
                        },
                        args.request_id,
                    ),
                })
            }
            GuestToolsSubcommand::ThawFilesystem(args) => {
                Ok(BridgeVmRequest::GuestToolsSendCommand {
                    name: args.vm,
                    envelope: agent_command_envelope(AgentMessage::ThawFilesystem, args.request_id),
                })
            }
            GuestToolsSubcommand::SetClipboard(args) => {
                Ok(BridgeVmRequest::GuestToolsSendCommand {
                    name: args.vm,
                    envelope: agent_command_envelope(
                        AgentMessage::SetClipboard { text: args.text },
                        args.request_id,
                    ),
                })
            }
            GuestToolsSubcommand::ResizeDisplay(args) => {
                Ok(BridgeVmRequest::GuestToolsSendCommand {
                    name: args.vm,
                    envelope: agent_command_envelope(
                        AgentMessage::ResizeDisplay {
                            width: args.width,
                            height: args.height,
                            scale: args.scale,
                        },
                        args.request_id,
                    ),
                })
            }
            GuestToolsSubcommand::MountShare(args) => Ok(BridgeVmRequest::GuestToolsSendCommand {
                name: args.vm,
                envelope: agent_command_envelope(
                    AgentMessage::MountShare {
                        name: args.name,
                        host_path_token: args.host_path_token,
                    },
                    args.request_id,
                ),
            }),
            GuestToolsSubcommand::MountApprovedShare(args) => {
                Ok(BridgeVmRequest::GuestToolsMountApprovedShare {
                    name: args.vm,
                    share: args.share,
                    request_id: args.request_id,
                })
            }
            GuestToolsSubcommand::UnmountShare(args) => {
                Ok(BridgeVmRequest::GuestToolsSendCommand {
                    name: args.vm,
                    envelope: agent_command_envelope(
                        AgentMessage::UnmountShare { name: args.name },
                        args.request_id,
                    ),
                })
            }
            GuestToolsSubcommand::FileDropStart(args) => {
                Ok(BridgeVmRequest::GuestToolsSendCommand {
                    name: args.vm,
                    envelope: agent_command_envelope(
                        AgentMessage::FileDropStart {
                            transfer_id: args.transfer_id,
                            file_name: args.file_name,
                            size_bytes: args.size_bytes,
                        },
                        args.request_id,
                    ),
                })
            }
            GuestToolsSubcommand::FileDropChunk(args) => {
                Ok(BridgeVmRequest::GuestToolsSendCommand {
                    name: args.vm,
                    envelope: agent_command_envelope(
                        AgentMessage::FileDropChunk {
                            transfer_id: args.transfer_id,
                            chunk_index: args.chunk_index,
                            data_base64: args.data_base64,
                        },
                        args.request_id,
                    ),
                })
            }
            GuestToolsSubcommand::FileDropComplete(args) => {
                Ok(BridgeVmRequest::GuestToolsSendCommand {
                    name: args.vm,
                    envelope: agent_command_envelope(
                        AgentMessage::FileDropComplete {
                            transfer_id: args.transfer_id,
                        },
                        args.request_id,
                    ),
                })
            }
            GuestToolsSubcommand::ListApplications(args) => {
                Ok(BridgeVmRequest::GuestToolsSendCommand {
                    name: args.vm,
                    envelope: agent_command_envelope(
                        AgentMessage::ListApplications,
                        args.request_id,
                    ),
                })
            }
            GuestToolsSubcommand::LaunchApplication(args) => {
                Ok(BridgeVmRequest::GuestToolsSendCommand {
                    name: args.vm,
                    envelope: agent_command_envelope(
                        AgentMessage::LaunchApplication { id: args.id },
                        args.request_id,
                    ),
                })
            }
            GuestToolsSubcommand::ListWindows(args) => Ok(BridgeVmRequest::GuestToolsSendCommand {
                name: args.vm,
                envelope: agent_command_envelope(AgentMessage::ListWindows, args.request_id),
            }),
            GuestToolsSubcommand::FocusWindow(args) => Ok(BridgeVmRequest::GuestToolsSendCommand {
                name: args.vm,
                envelope: agent_command_envelope(
                    AgentMessage::FocusWindow { id: args.id },
                    args.request_id,
                ),
            }),
            GuestToolsSubcommand::CloseWindow(args) => Ok(BridgeVmRequest::GuestToolsSendCommand {
                name: args.vm,
                envelope: agent_command_envelope(
                    AgentMessage::CloseWindow { id: args.id },
                    args.request_id,
                ),
            }),
            GuestToolsSubcommand::TimeSync(args) => Ok(BridgeVmRequest::GuestToolsSendCommand {
                name: args.vm,
                envelope: agent_command_envelope(
                    AgentMessage::TimeSync {
                        unix_epoch_millis: args
                            .unix_epoch_millis
                            .unwrap_or_else(current_unix_epoch_millis),
                    },
                    args.request_id,
                ),
            }),
        },
        Command::QemuArgs(args) => Ok(BridgeVmRequest::QemuArgs { name: args.name }),
        Command::PrepareRun(args) => Ok(BridgeVmRequest::PrepareRun { name: args.name }),
        Command::BootMedia(args) => Ok(BridgeVmRequest::InspectBootMedia { name: args.name }),
        Command::Ssh(args) => Ok(BridgeVmRequest::SshPlan {
            name: args.vm,
            user: Some(args.user),
        }),
        Command::Open(args) => Ok(BridgeVmRequest::OpenPort {
            name: args.vm,
            guest: args.guest,
            scheme: Some(args.scheme),
        }),
        Command::Run(args) => Ok(BridgeVmRequest::RunBackend {
            name: args.name,
            spawn: args.spawn,
        }),
        Command::Display(_) => Err(anyhow::anyhow!(
            "the embedded display window must run on the local GUI session; run `bridgevm display <vm>` with --store (not --socket)"
        )),
        Command::Readiness(args) => Ok(BridgeVmRequest::ReadinessReport {
            name: args.name,
            live_evidence: args.live_evidence,
            record_live_evidence: args.record_live_evidence,
            clear_live_evidence: args.clear_live_evidence,
        }),
        Command::LifecyclePlan(args) => Ok(BridgeVmRequest::LifecyclePlan {
            name: args.name,
            action: args.action.into(),
        }),
        Command::QmpSocket(args) => Ok(BridgeVmRequest::QmpSocket { name: args.name }),
        Command::QmpStatus(args) => Ok(BridgeVmRequest::QmpStatus { name: args.name }),
        Command::QmpStop(args) => Ok(BridgeVmRequest::QmpStop { name: args.name }),
        Command::QmpCont(args) => Ok(BridgeVmRequest::QmpCont { name: args.name }),
        Command::RunnerStatus(args) => Ok(BridgeVmRequest::RunnerStatus { name: args.name }),
        Command::Recommend(args) => Ok(BridgeVmRequest::RecommendMode {
            choice: GuestChoice {
                os: args.os,
                version: args.version,
                arch: args.arch,
            },
        }),
        Command::Store(StoreCommand::Doctor) => Ok(BridgeVmRequest::Doctor),
        Command::Doctor => Ok(BridgeVmRequest::Doctor),
    }
}

fn send_request(socket: &Path, request: BridgeVmRequest) -> Result<BridgeVmResponse> {
    let mut stream = UnixStream::connect(socket)
        .with_context(|| format!("failed to connect to daemon socket {}", socket.display()))?;
    serde_json::to_writer(&mut stream, &request).context("failed to write daemon request")?;
    stream.write_all(b"\n")?;

    let mut line = String::new();
    BufReader::new(stream)
        .read_line(&mut line)
        .context("failed to read daemon response")?;
    let response =
        serde_json::from_str::<BridgeVmResponse>(&line).context("invalid daemon response JSON")?;
    response.into_result().map_err(anyhow::Error::msg)
}

fn print_daemon_response(response: BridgeVmResponse) -> Result<()> {
    match response {
        BridgeVmResponse::Doctor {
            store_root,
            vms_dir,
            status,
        } => {
            println!("BridgeVM store: {}", store_root.display());
            println!("VM bundles: {}", vms_dir.display());
            print_doctor_audit(&doctor_audit_for_paths(&store_root, &vms_dir));
            println!("Status: {}", status);
        }
        BridgeVmResponse::VmList { vms } => {
            if vms.is_empty() {
                println!("No VMs found");
            } else {
                for vm in vms {
                    print_vm_record(&vm);
                }
            }
        }
        BridgeVmResponse::BootTemplates { templates } => print_boot_templates(&templates),
        BridgeVmResponse::Vm { vm } => print_vm_record(&vm),
        BridgeVmResponse::Deleted {
            path,
            metadata_only,
            metadata,
        } => {
            if metadata_only {
                if let Some(metadata) = metadata {
                    println!(
                        "Deleted VM metadata for {} at {} (bundle preserved: {})",
                        metadata.vm,
                        metadata.metadata_path.display(),
                        path.display()
                    );
                } else {
                    println!("Deleted VM metadata at {}", path.display());
                }
            } else {
                println!("Deleted VM bundle {}", path.display());
            }
        }
        BridgeVmResponse::Exported { export } => println!(
            "Exported {} from {} to {}",
            export.vm,
            export.source.display(),
            export.output.display()
        ),
        BridgeVmResponse::Imported { import } => println!(
            "Imported {} from {} to {}",
            import.vm,
            import.source.display(),
            import.output.display()
        ),
        BridgeVmResponse::Cloned { clone } => print_clone(&clone),
        BridgeVmResponse::DiagnosticBundle { bundle } => print_diagnostic_bundle(&bundle),
        BridgeVmResponse::LogsViewed { log } => print_vm_log(&log),
        BridgeVmResponse::PerformanceBaseline { baseline } => print_performance_baseline(&baseline),
        BridgeVmResponse::PerformanceSample { sample } => print_performance_sample(&sample),
        BridgeVmResponse::MetadataRepaired { repair } => print_metadata_repair(&repair),
        BridgeVmResponse::ManifestMigrated { migration } => print_manifest_migration(&migration),
        BridgeVmResponse::State { name, metadata } => {
            println!("Metadata state recorded for {} ({})", name, metadata.state);
        }
        BridgeVmResponse::Snapshot {
            snapshot,
            disk,
            application_consistent_preflight,
        } => {
            println!(
                "Created {} snapshot '{}' ({})",
                snapshot.kind, snapshot.name, snapshot.vm_state
            );
            if let Some(disk) = disk {
                print_snapshot_disk_status(&disk);
            }
            if let Some(preflight) = application_consistent_preflight {
                print_application_consistent_snapshot_preflight(&preflight);
            }
        }
        BridgeVmResponse::SnapshotList { snapshots } => {
            if snapshots.is_empty() {
                println!("No snapshots found");
            } else {
                for snapshot in snapshots {
                    println!(
                        "{}\t{}\t{}\t{}",
                        snapshot.name, snapshot.kind, snapshot.vm_state, snapshot.created_at_unix
                    );
                }
            }
        }
        BridgeVmResponse::SnapshotChain { chain } => print_snapshot_chain(&chain),
        BridgeVmResponse::SnapshotPreflightStatus { preflight } => {
            print_snapshot_preflight_status(&preflight)
        }
        BridgeVmResponse::ApplicationConsistentSnapshotExecution { execution } => {
            print_application_consistent_snapshot_execution(&execution)
        }
        BridgeVmResponse::SnapshotRestored { restore } => {
            println!(
                "Restored snapshot '{}' metadata; recorded state: {}",
                restore.snapshot, restore.restored_state
            );
            if let Some(active_disk) = restore.active_disk {
                print_active_disk(&active_disk);
            }
            if let Some(suspend_image) = restore.suspend_image {
                print_snapshot_suspend_image_status(&suspend_image);
            }
        }
        BridgeVmResponse::SnapshotDiskCreated { metadata } => {
            print_snapshot_disk_create_status(&metadata)
        }
        BridgeVmResponse::QemuCommand { command } => {
            for word in command.render_shell_words() {
                println!("{word}");
            }
        }
        BridgeVmResponse::DiskPrepared { metadata } => print_disk_status(&metadata),
        BridgeVmResponse::DiskCreated { metadata } => print_disk_create_status(&metadata),
        BridgeVmResponse::DiskInspected { metadata } => print_disk_inspect_status(&metadata),
        BridgeVmResponse::DiskVerified { metadata } => print_disk_verify_status(&metadata),
        BridgeVmResponse::DiskCompacted { metadata } => print_disk_compact_status(&metadata),
        BridgeVmResponse::PortForwards { ports } => print_port_forwards(&ports),
        BridgeVmResponse::NetworkPlanned { plan } => print_network_plan(&plan),
        BridgeVmResponse::SharedFolders { shares } => print_shared_folders(&shares),
        BridgeVmResponse::SshPlan { plan } => print_ssh_plan(&plan),
        BridgeVmResponse::OpenPortPlan { plan } => print_open_port_plan(&plan),
        BridgeVmResponse::RunnerStatus {
            metadata,
            qmp_supervisor,
        } => print_runner_status(metadata, qmp_supervisor.as_ref()),
        BridgeVmResponse::ReadinessReport { report } => print_readiness_report(&report),
        BridgeVmResponse::LifecyclePlan { plan } => print_lifecycle_plan(&plan),
        BridgeVmResponse::BootMedia { name, boot } => print_boot_media(&name, &boot),
        BridgeVmResponse::BootMediaImported { import } => print_boot_media_import(&import),
        BridgeVmResponse::BootMediaStatus { status } => print_boot_media_status(&status),
        BridgeVmResponse::BootMediaVerified { verification } => {
            print_boot_media_verification(&verification)
        }
        BridgeVmResponse::BootMediaDownloadPlanned { plan } => {
            print_boot_media_download_plan(&plan)
        }
        BridgeVmResponse::BootMediaDownloaded { download } => print_boot_media_download(&download),
        BridgeVmResponse::QmpSocket { path } => println!("{}", path.display()),
        BridgeVmResponse::QmpStatus { status } => {
            if !status.available {
                println!("QMP socket unavailable: {}", status.socket_path.display());
            } else {
                println!(
                    "QMP status: {}",
                    status.status.unwrap_or_else(|| "unknown".to_string())
                );
                println!("Running: {}", status.running.unwrap_or(false));
            }
            if let Some(supervisor) = &status.supervisor {
                print_qmp_supervisor(supervisor);
            }
        }
        BridgeVmResponse::QmpCommandExecuted { command } => {
            println!("QMP command sent: {}", command.command);
            println!("VM: {}", command.vm);
            println!("QMP socket: {}", command.socket_path.display());
        }
        BridgeVmResponse::GuestToolsStatus { status } => print_guest_tools_status(&status),
        BridgeVmResponse::GuestToolsToken { token } => print_guest_tools_token(&token),
        BridgeVmResponse::GuestToolsSession { session } => print_guest_tools_session(&session),
        BridgeVmResponse::GuestToolsLinuxCommand { command } => {
            print_guest_tools_linux_command(&command)
        }
        BridgeVmResponse::GuestToolsCommand { command } => {
            println!("Guest tools command sent for {}", command.vm);
            println!(
                "Request ID: {}",
                command.request_id.as_deref().unwrap_or("none")
            );
            println!("Pending commands: {}", command.pending_commands);
        }
        BridgeVmResponse::ModeRecommendation { recommendation } => {
            print_mode_recommendation(&recommendation);
        }
        BridgeVmResponse::Error { message } => bail!(message),
    }
    Ok(())
}

fn print_vm_record(vm: &VmRecord) {
    println!(
        "{}\t{}\t{}\t{} {}\t{}",
        vm.name,
        vm.state,
        vm.mode,
        vm.guest_os,
        vm.guest_arch,
        vm.path.display()
    );
    if let Some(supervisor) = &vm.qmp_supervisor {
        print_qmp_supervisor(supervisor);
    }
}

fn list(store: &VmStore) -> Result<()> {
    let vms = store.list_vms().context("failed to list VMs")?;
    if vms.is_empty() {
        println!("No VMs found in {}", store.vms_dir().display());
        return Ok(());
    }
    for (path, manifest) in vms {
        let state = store
            .state(&manifest.name)
            .map(|metadata| metadata.state.to_string())
            .unwrap_or_else(|_| "unknown".to_string());
        println!(
            "{}\t{}\t{}\t{} {}\t{}",
            manifest.name,
            state,
            manifest.mode,
            manifest.guest.os,
            manifest.guest.arch,
            path.display()
        );
    }
    Ok(())
}

fn templates() -> Result<()> {
    let templates = available_boot_templates();
    print_boot_templates(&templates);
    Ok(())
}

fn create(store: &VmStore, args: CreateArgs) -> Result<()> {
    let manifest = manifest_for_create(args)?;
    let rec = recommend_mode(&GuestChoice {
        os: manifest.guest.os.clone(),
        version: manifest.guest.version.clone(),
        arch: manifest.guest.arch.clone(),
    });
    let path = store
        .create_vm(&manifest)
        .context("failed to create VM bundle")?;
    println!("Created {} VM at {}", manifest.mode, path.display());
    println!("{}", rec.message);
    Ok(())
}

fn manifest_for_create(args: CreateArgs) -> Result<VmManifest> {
    let template = args
        .template
        .as_deref()
        .map(|id| boot_template_by_id(id).with_context(|| format!("unknown template id: {id}")))
        .transpose()?;
    let os = args
        .os
        .clone()
        .or_else(|| template.as_ref().map(|template| template.guest_os.clone()))
        .context("create requires --os unless --template provides a guest")?;
    let version = args.version.clone().or_else(|| {
        template
            .as_ref()
            .and_then(|template| template.guest_version.clone())
    });
    let arch = args
        .arch
        .clone()
        .or_else(|| {
            template
                .as_ref()
                .map(|template| template.guest_arch.clone())
        })
        .unwrap_or_else(|| "arm64".to_string());
    let choice = GuestChoice {
        os: os.clone(),
        version: version.clone(),
        arch: arch.clone(),
    };
    let rec = recommend_mode(&choice);
    let mode = match args.mode {
        ModeChoice::Auto => rec.mode,
        ModeChoice::Fast if rec.fast_mode_available => VmMode::Fast,
        ModeChoice::Fast => bail!("{}", rec.message),
        ModeChoice::Compatibility => VmMode::Compatibility,
    };

    let boot = boot_for_create(&args, mode, &rec, template.as_ref());
    let mut manifest = VmManifest::new(args.name, mode, Guest { os, version, arch }, args.disk);
    manifest.boot = Some(boot);
    Ok(manifest)
}

fn boot_for_create(
    args: &CreateArgs,
    mode: VmMode,
    rec: &ModeRecommendation,
    template: Option<&BootTemplate>,
) -> Boot {
    let explicit_boot = args.boot_mode.is_some()
        || args.installer_image.is_some()
        || args.kernel_path.is_some()
        || args.initrd_path.is_some()
        || args.kernel_command_line.is_some()
        || args.macos_restore_image.is_some();
    if !explicit_boot && mode == VmMode::Fast {
        if let Some(template) = template {
            return template.as_boot();
        }
        if let Some(template) = &rec.boot_template {
            return template.as_boot();
        }
    }

    let inferred_boot_mode = args
        .boot_mode
        .map(BootMode::from)
        .or_else(|| {
            args.installer_image
                .as_ref()
                .map(|_| BootMode::LinuxInstaller)
        })
        .or_else(|| args.kernel_path.as_ref().map(|_| BootMode::LinuxKernel))
        .or_else(|| {
            args.macos_restore_image
                .as_ref()
                .map(|_| BootMode::MacosRestore)
        })
        .unwrap_or(BootMode::ExistingDisk);
    Boot {
        mode: inferred_boot_mode,
        installer_image: args.installer_image.clone(),
        kernel_path: args.kernel_path.clone(),
        initrd_path: args.initrd_path.clone(),
        kernel_command_line: args.kernel_command_line.clone(),
        macos_restore_image: args.macos_restore_image.clone(),
    }
}

fn status(store: &VmStore, args: VmNameArgs) -> Result<()> {
    let (_, manifest) = store.get_vm(&args.name).context("failed to read VM")?;
    let state = store.state(&args.name).context("failed to read VM state")?;
    println!("Name: {}", manifest.name);
    println!("Mode: {}", manifest.mode);
    println!("Guest: {} {}", manifest.guest.os, manifest.guest.arch);
    println!("State: {}", state.state);
    println!("Updated: {}", state.updated_at_unix);
    Ok(())
}

fn transition(store: &VmStore, args: VmNameArgs, to: VmRuntimeState, verb: &str) -> Result<()> {
    let state = store
        .transition_state(&args.name, to)
        .with_context(|| format!("failed to transition VM '{}'", args.name))?;
    println!("{} {} ({})", verb, args.name, state.state);
    Ok(())
}

fn delete(store: &VmStore, args: DeleteArgs) -> Result<()> {
    let state = store.state(&args.name).context("failed to read VM state")?;
    if state.state == VmRuntimeState::Running {
        bail!("refusing to delete a running VM; stop it first");
    }
    if args.metadata_only {
        let metadata = store
            .delete_vm_metadata_only(&args.name)
            .with_context(|| format!("failed to delete VM metadata '{}'", args.name))?;
        println!(
            "Deleted VM metadata for {} at {} (bundle preserved: {})",
            metadata.vm,
            metadata.metadata_path.display(),
            metadata.bundle.display()
        );
        return Ok(());
    }
    let path = store
        .delete_vm(&args.name)
        .with_context(|| format!("failed to delete VM '{}'", args.name))?;
    println!("Deleted VM bundle {}", path.display());
    Ok(())
}

fn export_vm(store: &VmStore, args: ExportArgs) -> Result<()> {
    let export = store
        .export_vm(&args.name, &args.output)
        .with_context(|| format!("failed to export VM '{}'", args.name))?;
    println!(
        "Exported {} from {} to {}",
        export.vm,
        export.source.display(),
        export.output.display()
    );
    Ok(())
}

fn import_vm(store: &VmStore, args: ImportArgs) -> Result<()> {
    let import = store
        .import_vm(&args.input, args.name.as_deref())
        .with_context(|| format!("failed to import VM bundle '{}'", args.input.display()))?;
    println!(
        "Imported {} from {} to {}",
        import.vm,
        import.source.display(),
        import.output.display()
    );
    Ok(())
}

fn clone_vm(store: &VmStore, args: CloneArgs) -> Result<()> {
    let clone = store
        .clone_vm(&args.name, &args.new_name, args.linked)
        .with_context(|| format!("failed to clone VM '{}'", args.name))?;
    print_clone(&clone);
    Ok(())
}

fn print_clone(clone: &bridgevm_storage::VmCloneMetadata) {
    println!(
        "Cloned {} from {} to {}",
        clone.vm,
        clone.source.display(),
        clone.output.display()
    );
    if clone.linked {
        println!("Linked clone: true");
        if let Some(backing_path) = &clone.backing_path {
            println!("Backing disk: {}", backing_path.display());
        }
        if let Some(backing_format) = &clone.backing_format {
            println!("Backing format: {backing_format}");
        }
        if let Some(command) = &clone.create_command {
            println!("Clone disk create command: {}", command.join(" "));
        }
    }
}

fn diagnostics(store: &VmStore, args: DiagnosticsCommand) -> Result<()> {
    match args.command {
        DiagnosticsSubcommand::Bundle(args) => {
            let bundle = create_diagnostic_bundle(store, &args.vm, args.output)
                .map_err(anyhow::Error::msg)?;
            print_diagnostic_bundle(&bundle);
        }
    }
    Ok(())
}

fn logs(store: &VmStore, args: LogsCommand) -> Result<()> {
    let log = match args.command {
        LogsSubcommand::Qemu(args) => {
            view_vm_log(store, &args.vm, VmLogKind::Qemu, args.bytes).map_err(anyhow::Error::msg)?
        }
        LogsSubcommand::Serial(args) => view_vm_log(store, &args.vm, VmLogKind::Serial, args.bytes)
            .map_err(anyhow::Error::msg)?,
    };
    print_vm_log(&log);
    Ok(())
}

fn performance(store: &VmStore, args: PerformanceCommand) -> Result<()> {
    match args.command {
        PerformanceSubcommand::Baseline(args) => {
            let baseline = create_performance_baseline(store, &args.vm, args.output)
                .map_err(anyhow::Error::msg)?;
            print_performance_baseline(&baseline);
        }
        PerformanceSubcommand::Sample(args) => {
            let sample = create_performance_sample(
                store,
                &args.vm,
                args.output,
                args.artifact_bytes,
                args.iterations,
                args.sync,
            )
            .map_err(anyhow::Error::msg)?;
            print_performance_sample(&sample);
        }
    }
    Ok(())
}

fn metadata(store: &VmStore, args: MetadataCommand) -> Result<()> {
    match args.command {
        MetadataSubcommand::Repair(args) => {
            let repair = store
                .repair_metadata(&args.name)
                .with_context(|| format!("failed to repair metadata for VM '{}'", args.name))?;
            print_metadata_repair(&repair);
        }
        MetadataSubcommand::MigrateManifest(args) => {
            let migration = store
                .migrate_manifest(&args.name, args.dry_run)
                .with_context(|| format!("failed to migrate manifest for VM '{}'", args.name))?;
            print_manifest_migration(&migration);
        }
        MetadataSubcommand::ManifestSchema => {
            println!("{}", manifest_json_schema_v1());
        }
        MetadataSubcommand::ValidateManifest(args) => {
            let manifest = VmManifest::read(&args.path)
                .with_context(|| format!("failed to validate manifest {}", args.path.display()))?;
            println!("Manifest valid: {}", args.path.display());
            println!("Schema version: {}", manifest.schema_version);
            println!("Name: {}", manifest.name);
            println!("Mode: {}", manifest.mode);
        }
    }
    Ok(())
}

fn print_metadata_repair(repair: &VmMetadataRepairMetadata) {
    println!("Metadata repair for {}", repair.vm);
    println!("Metadata repaired: {}", repair.repaired);
    println!("Bundle: {}", repair.bundle.display());
    println!("Timestamp: {}", repair.repaired_at_unix);
    if repair.actions.is_empty() {
        println!("No metadata repairs needed");
        return;
    }
    for action in &repair.actions {
        println!(
            "{}: {} ({})",
            action.action,
            action.path.display(),
            action.detail
        );
    }
}

fn print_manifest_migration(migration: &VmManifestMigrationMetadata) {
    println!("Manifest migration for {}", migration.vm);
    println!("Dry run: {}", migration.dry_run);
    println!("Migrated: {}", migration.migrated);
    println!("From schema: {}", migration.from_schema);
    println!("To schema: {}", migration.to_schema);
    println!("Bundle: {}", migration.bundle.display());
    println!("Manifest: {}", migration.manifest_path.display());
    println!("Timestamp: {}", migration.migrated_at_unix);
    if let Some(path) = &migration.backup_path {
        println!("Backup: {}", path.display());
    }
    if let Some(path) = &migration.receipt_path {
        println!("Receipt: {}", path.display());
    }
    for action in &migration.actions {
        println!(
            "{}: {} ({})",
            action.action,
            action.path.display(),
            action.detail
        );
    }
}

fn snapshot(store: &VmStore, args: SnapshotCommand) -> Result<()> {
    match args.command {
        SnapshotSubcommand::Create(args) => {
            let snapshot = store
                .create_snapshot(&args.vm, &args.name, args.kind.into())
                .context("failed to create snapshot metadata")?;
            println!(
                "Created {} snapshot '{}' for {}",
                snapshot.kind, snapshot.name, args.vm
            );
            if let Some(disk) = store
                .snapshot_disk_metadata(&args.vm, &args.name)
                .context("failed to read snapshot disk metadata")?
            {
                print_snapshot_disk_status(&disk);
            }
            if let Some(preflight) = store
                .application_consistent_snapshot_preflight_metadata(&args.vm, &args.name)
                .context("failed to read application-consistent snapshot preflight metadata")?
            {
                print_application_consistent_snapshot_preflight(&preflight);
            }
            Ok(())
        }
        SnapshotSubcommand::ExecuteApplicationConsistent(_) => {
            bail!("application-consistent snapshot execution requires --socket bridgevmd access")
        }
        SnapshotSubcommand::List(args) => {
            let snapshots = store
                .snapshots(&args.name)
                .context("failed to list snapshots")?;
            if snapshots.is_empty() {
                println!("No snapshots found for {}", args.name);
                return Ok(());
            }
            for snapshot in snapshots {
                println!(
                    "{}\t{}\t{}\t{}",
                    snapshot.name, snapshot.kind, snapshot.vm_state, snapshot.created_at_unix
                );
            }
            Ok(())
        }
        SnapshotSubcommand::Chain(args) => {
            let chain = store
                .snapshot_chain(&args.name)
                .context("failed to inspect snapshot chain")?;
            print_snapshot_chain(&chain);
            Ok(())
        }
        SnapshotSubcommand::Restore(args) => {
            let restore = store
                .restore_snapshot(&args.vm, &args.name)
                .context("failed to restore snapshot metadata")?;
            println!(
                "Restored snapshot '{}' metadata for {}; recorded state: {}",
                restore.snapshot, args.vm, restore.restored_state
            );
            if let Some(active_disk) = restore.active_disk {
                print_active_disk(&active_disk);
            }
            if let Some(suspend_image) = restore.suspend_image {
                print_snapshot_suspend_image_status(&suspend_image);
            }
            Ok(())
        }
        SnapshotSubcommand::DiskCreate(args) => {
            let metadata = store
                .create_snapshot_disk(&args.vm, &args.name)
                .context("failed to create snapshot disk overlay")?;
            print_snapshot_disk_create_status(&metadata);
            Ok(())
        }
    }
}

fn disk(store: &VmStore, args: DiskCommand) -> Result<()> {
    match args.command {
        DiskSubcommand::Prepare(args) => {
            let metadata = store
                .prepare_primary_disk(&args.name)
                .context("failed to prepare primary disk")?;
            print_disk_status(&metadata);
            Ok(())
        }
        DiskSubcommand::Create(args) => {
            let metadata = store
                .create_primary_disk(&args.name)
                .context("failed to create primary disk")?;
            print_disk_create_status(&metadata);
            Ok(())
        }
        DiskSubcommand::Inspect(args) => {
            let metadata = store
                .inspect_primary_disk(&args.name)
                .context("failed to inspect primary disk")?;
            print_disk_inspect_status(&metadata);
            Ok(())
        }
        DiskSubcommand::Verify(args) => {
            let metadata = store
                .verify_active_disk(&args.name)
                .context("failed to verify active disk")?;
            print_disk_verify_status(&metadata);
            Ok(())
        }
        DiskSubcommand::Compact(args) => {
            let metadata = store
                .compact_active_disk(&args.name)
                .context("failed to compact active disk")?;
            print_disk_compact_status(&metadata);
            Ok(())
        }
    }
}

fn port(store: &VmStore, args: PortCommand) -> Result<()> {
    match args.command {
        PortSubcommand::List(args) => {
            let ports = list_ports(store, &args.name).map_err(anyhow::Error::msg)?;
            print_port_forwards(&ports);
        }
        PortSubcommand::Add(args) => {
            let (host, guest) = parse_port_mapping(&args.mapping)?;
            let ports = add_port(store, &args.vm, host, guest).map_err(anyhow::Error::msg)?;
            print_port_forwards(&ports);
        }
        PortSubcommand::Remove(args) => {
            let (host, guest) = parse_port_mapping(&args.mapping)?;
            let ports = remove_port(store, &args.vm, host, guest).map_err(anyhow::Error::msg)?;
            print_port_forwards(&ports);
        }
    }
    Ok(())
}

fn network_plan(store: &VmStore, args: VmNameArgs) -> Result<()> {
    let plan = bridgevm_api::network_plan(store, &args.name).map_err(anyhow::Error::msg)?;
    print_network_plan(&plan);
    Ok(())
}

fn share(store: &VmStore, args: ShareCommand) -> Result<()> {
    match args.command {
        ShareSubcommand::List(args) => {
            let shares = list_shares(store, &args.name).map_err(anyhow::Error::msg)?;
            print_shared_folders(&shares);
        }
        ShareSubcommand::Add(args) => {
            let shares = add_share(
                store,
                &args.vm,
                args.name,
                args.host_path,
                args.read_only,
                args.host_path_token,
            )
            .map_err(anyhow::Error::msg)?;
            print_shared_folders(&shares);
        }
        ShareSubcommand::Remove(args) => {
            let shares = remove_share(store, &args.vm, &args.name).map_err(anyhow::Error::msg)?;
            print_shared_folders(&shares);
        }
    }
    Ok(())
}

fn ssh(store: &VmStore, args: SshArgs) -> Result<()> {
    let plan =
        bridgevm_api::ssh_plan(store, &args.vm, Some(&args.user)).map_err(anyhow::Error::msg)?;
    print_ssh_plan(&plan);
    Ok(())
}

fn open_port(store: &VmStore, args: OpenArgs) -> Result<()> {
    let plan = open_port_plan(store, &args.vm, args.guest, Some(&args.scheme))
        .map_err(anyhow::Error::msg)?;
    print_open_port_plan(&plan);
    Ok(())
}

fn parse_port_mapping(mapping: &str) -> Result<(u16, u16)> {
    let (host, guest) = mapping
        .split_once(':')
        .ok_or_else(|| anyhow::anyhow!("port mapping must be HOST:GUEST"))?;
    let host = parse_port_number("host", host)?;
    let guest = parse_port_number("guest", guest)?;
    Ok((host, guest))
}

fn parse_port_number(label: &str, value: &str) -> Result<u16> {
    let port = value
        .parse::<u16>()
        .with_context(|| format!("{label} port must be between 1 and 65535"))?;
    if port == 0 {
        bail!("{label} port must be between 1 and 65535");
    }
    Ok(port)
}

fn media(store: &VmStore, args: MediaCommand) -> Result<()> {
    match args.command {
        MediaSubcommand::Download(args) => {
            let download = download_boot_media(store, &args.vm, args.kind.map(Into::into))
                .map_err(anyhow::Error::msg)?;
            print_boot_media_download(&download);
        }
        MediaSubcommand::DownloadPlan(args) => {
            let plan = plan_boot_media_download(
                store,
                &args.vm,
                &args.url,
                args.sha256.as_deref(),
                args.kind.map(Into::into),
            )
            .map_err(anyhow::Error::msg)?;
            print_boot_media_download_plan(&plan);
        }
        MediaSubcommand::Import(args) => {
            let metadata =
                import_boot_media(store, &args.vm, args.source, args.kind.map(Into::into))
                    .map_err(anyhow::Error::msg)?;
            print_boot_media_import(&metadata);
        }
        MediaSubcommand::Status(args) => {
            let status =
                inspect_boot_media_status(store, &args.name).map_err(anyhow::Error::msg)?;
            print_boot_media_status(&status);
        }
        MediaSubcommand::Verify(args) => {
            let verification =
                verify_boot_media(store, &args.vm, &args.sha256, args.kind.map(Into::into))
                    .map_err(anyhow::Error::msg)?;
            print_boot_media_verification(&verification);
        }
    }
    Ok(())
}

fn guest_tools(store: &VmStore, args: GuestToolsCommand) -> Result<()> {
    match args.command {
        GuestToolsSubcommand::Status(args) => {
            let status =
                inspect_guest_tools_status(store, &args.name).map_err(anyhow::Error::msg)?;
            print_guest_tools_status(&status);
        }
        GuestToolsSubcommand::Token(args) => {
            let token = guest_tools_token(store, &args.name).map_err(anyhow::Error::msg)?;
            print_guest_tools_token(&token);
        }
        GuestToolsSubcommand::LinuxCommand(args) => {
            let command = guest_tools_linux_command(
                store,
                &args.vm,
                args.transport.into(),
                args.token_file,
                args.device,
            )
            .map_err(anyhow::Error::msg)?;
            print_guest_tools_linux_command(&command);
        }
        GuestToolsSubcommand::AcceptHello(args) => {
            let envelope = parse_agent_envelope(&args.hello_json)?;
            let session =
                accept_guest_tools_hello(store, &args.vm, &envelope).map_err(anyhow::Error::msg)?;
            print_guest_tools_session(&session);
        }
        GuestToolsSubcommand::SendCommand(_)
        | GuestToolsSubcommand::FreezeFilesystem(_)
        | GuestToolsSubcommand::ThawFilesystem(_)
        | GuestToolsSubcommand::SetClipboard(_)
        | GuestToolsSubcommand::ResizeDisplay(_)
        | GuestToolsSubcommand::MountShare(_)
        | GuestToolsSubcommand::MountApprovedShare(_)
        | GuestToolsSubcommand::UnmountShare(_)
        | GuestToolsSubcommand::FileDropStart(_)
        | GuestToolsSubcommand::FileDropChunk(_)
        | GuestToolsSubcommand::FileDropComplete(_)
        | GuestToolsSubcommand::ListApplications(_)
        | GuestToolsSubcommand::LaunchApplication(_)
        | GuestToolsSubcommand::ListWindows(_)
        | GuestToolsSubcommand::FocusWindow(_)
        | GuestToolsSubcommand::CloseWindow(_)
        | GuestToolsSubcommand::TimeSync(_) => {
            bail!("guest-tools command dispatch requires --socket bridgevmd access")
        }
    }
    Ok(())
}

fn parse_agent_envelope(value: &str) -> Result<AgentEnvelope> {
    serde_json::from_str(value).context("invalid guest tools envelope JSON")
}

fn agent_command_envelope(message: AgentMessage, request_id: Option<String>) -> AgentEnvelope {
    match request_id {
        Some(request_id) => AgentEnvelope::with_request_id(message, request_id),
        None => AgentEnvelope::new(message),
    }
}

fn current_unix_epoch_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn qemu_args(store: &VmStore, args: VmNameArgs) -> Result<()> {
    let (bundle, manifest, _) = store
        .get_vm_with_active_disk(&args.name)
        .context("failed to read VM")?;
    let command = build_compatibility_command(&manifest, &bundle)
        .map_err(|error| anyhow::anyhow!("{}", compatibility_qemu_command_error(error)))?;
    for word in command.render_shell_words() {
        println!("{word}");
    }
    Ok(())
}

fn prepare_run(store: &VmStore, args: VmNameArgs) -> Result<()> {
    let metadata = build_runner_metadata(store, &args.name, false)?;
    let qmp_supervisor = store
        .qmp_supervisor_metadata(&args.name)
        .context("failed to read QMP supervisor metadata")?;
    print_runner_status(Some(metadata), qmp_supervisor.as_ref());
    Ok(())
}

fn boot_media(store: &VmStore, args: VmNameArgs) -> Result<()> {
    let (bundle, manifest, _) = store
        .get_vm_with_active_disk(&args.name)
        .context("failed to read VM")?;
    let plan =
        build_fast_plan(&manifest, &bundle).context("failed to inspect Fast Mode boot media")?;
    print_boot_media(&args.name, &plan.launch_spec().boot);
    Ok(())
}

fn run_backend_local(store: &VmStore, args: RunArgs) -> Result<()> {
    let metadata = build_runner_metadata(store, &args.name, args.spawn)?;
    let qmp_supervisor = store
        .qmp_supervisor_metadata(&args.name)
        .context("failed to read QMP supervisor metadata")?;
    print_runner_status(Some(metadata), qmp_supervisor.as_ref());
    Ok(())
}

fn suspend_backend_local(store: &VmStore, args: VmNameArgs) -> Result<()> {
    let metadata = suspend_backend(store, &args.name)
        .map_err(anyhow::Error::msg)
        .with_context(|| format!("failed to suspend VM '{}'", args.name))?;
    let qmp_supervisor = store
        .qmp_supervisor_metadata(&args.name)
        .context("failed to read QMP supervisor metadata")?;
    println!("Suspended {}", args.name);
    print_runner_status(Some(metadata), qmp_supervisor.as_ref());
    Ok(())
}

fn resume_backend_local(store: &VmStore, args: VmNameArgs) -> Result<()> {
    let metadata = resume_backend(store, &args.name)
        .map_err(anyhow::Error::msg)
        .with_context(|| format!("failed to resume VM '{}'", args.name))?;
    let qmp_supervisor = store
        .qmp_supervisor_metadata(&args.name)
        .context("failed to read QMP supervisor metadata")?;
    println!("Resumed {}", args.name);
    print_runner_status(Some(metadata), qmp_supervisor.as_ref());
    Ok(())
}

fn display_backend_local(store: &VmStore, args: VmNameArgs) -> Result<()> {
    if !apple_vz_runner_configured() {
        anyhow::bail!(
            "embedded display requires BRIDGEVM_APPLE_VZ_RUNNER to point at a signed AppleVzRunner"
        );
    }
    let metadata = display_fast_backend(store, &args.name)
        .map_err(anyhow::Error::msg)
        .with_context(|| format!("failed to launch embedded display for VM '{}'", args.name))?;
    println!(
        "Launched embedded display window for {} (close the window to stop the VM)",
        args.name
    );
    print_runner_status(Some(metadata), None);
    Ok(())
}

fn build_runner_metadata(
    store: &VmStore,
    name: &str,
    spawn: bool,
) -> Result<bridgevm_storage::RunnerMetadata> {
    let (bundle, manifest, _) = store
        .get_vm_with_active_disk(name)
        .context("failed to read VM")?;

    let (disk, active_disk) = store
        .prepare_active_disk(name)
        .context("failed to prepare active disk")?;
    if manifest.mode == VmMode::Fast {
        // Gated REAL cold-start launch: when `BRIDGEVM_APPLE_VZ_RUNNER` is set
        // and the caller asked to spawn, boot a real Apple VZ VM. When unset,
        // preserve the legacy dry-run + not-implemented behavior.
        if spawn && apple_vz_runner_configured() {
            return cold_start_fast_backend(store, name)
                .map_err(anyhow::Error::msg)
                .with_context(|| format!("failed to launch Fast Mode VM '{name}'"));
        }
        let plan = build_fast_plan(&manifest, &bundle).context("failed to build Fast Mode plan")?;
        let launch_spec_path = write_launch_spec_artifact(&bundle, plan.launch_spec())
            .context("failed to write Fast Mode launch spec")?;
        let mut readiness = launch_readiness_metadata(&plan.launch_spec().readiness);
        if spawn {
            add_fast_spawn_blocker(&mut readiness);
        }
        let spawn_error = spawn.then(|| fast_spawn_not_implemented_error(&readiness));
        let metadata = bridgevm_storage::RunnerMetadata {
            engine: "lightvm".to_string(),
            pid: None,
            command: plan.render_runner_words(),
            log_path: plan.launch_spec().logs.runner_log_path.clone().into(),
            started_at_unix: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            dry_run: true,
            launch_spec_path: Some(launch_spec_path),
            guest_tools: None,
            disk: Some(disk),
            active_disk: Some(active_disk),
            launch_readiness: Some(readiness),
        };
        store
            .write_runner_metadata(name, &metadata)
            .context("failed to write runner metadata")?;
        if let Some(error) = spawn_error {
            bail!("{}", error);
        }
        return Ok(metadata);
    }

    let command = build_compatibility_command(&manifest, &bundle)
        .map_err(|error| anyhow::anyhow!("{}", compatibility_qemu_command_error(error)))?;
    let readiness = compatibility_launch_readiness_metadata(&disk, None);
    if spawn && !readiness.ready {
        bail!("{}", compatibility_launch_readiness_summary(&readiness));
    }
    let log_path = bundle.join("logs").join("qemu.log");
    let guest_tools = store
        .guest_tools_runner_metadata(name)
        .context("failed to prepare guest tools runner metadata")?;

    if spawn {
        fs::create_dir_all(bundle.join("logs"))?;
        let stdout = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .context("failed to open QEMU log file")?;
        let stderr = stdout
            .try_clone()
            .context("failed to clone QEMU log file")?;
        let child = ProcessCommand::new(&command.program)
            .args(&command.args)
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr))
            .spawn()
            .with_context(|| format!("failed to spawn {}", command.program))?;
        let metadata = bridgevm_storage::RunnerMetadata {
            engine: "fullvm".to_string(),
            pid: Some(child.id()),
            command: command.render_shell_words(),
            log_path,
            started_at_unix: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            dry_run: false,
            launch_spec_path: None,
            guest_tools: Some(guest_tools),
            disk: Some(disk),
            active_disk: Some(active_disk),
            launch_readiness: Some(readiness),
        };
        store
            .write_runner_metadata(name, &metadata)
            .context("failed to write runner metadata")?;
        store
            .transition_state(name, VmRuntimeState::Running)
            .context("failed to mark VM running")?;
        return Ok(metadata);
    }

    let metadata = bridgevm_storage::RunnerMetadata {
        engine: "fullvm".to_string(),
        pid: None,
        command: command.render_shell_words(),
        log_path,
        started_at_unix: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        dry_run: true,
        launch_spec_path: None,
        guest_tools: Some(guest_tools),
        disk: Some(disk),
        active_disk: Some(active_disk),
        launch_readiness: Some(readiness),
    };
    store
        .write_runner_metadata(name, &metadata)
        .context("failed to write runner metadata")?;
    Ok(metadata)
}

fn lifecycle_plan(store: &VmStore, args: LifecyclePlanArgs) -> Result<()> {
    match bridgevm_api::handle_request(
        store,
        BridgeVmRequest::LifecyclePlan {
            name: args.name,
            action: args.action.into(),
        },
    ) {
        BridgeVmResponse::LifecyclePlan { plan } => {
            print_lifecycle_plan(&plan);
            Ok(())
        }
        BridgeVmResponse::Error { message } => bail!(message),
        _ => bail!("unexpected lifecycle plan response"),
    }
}

fn readiness(store: &VmStore, args: ReadinessArgs) -> Result<()> {
    let report = bridgevm_api::readiness_report_with_live_evidence_options(
        store,
        &args.name,
        args.live_evidence.as_deref(),
        args.record_live_evidence,
        args.clear_live_evidence,
    )
    .map_err(anyhow::Error::msg)?;
    print_readiness_report(&report);
    Ok(())
}

fn stop_backend_local(store: &VmStore, args: VmNameArgs) -> Result<()> {
    // Delegate to the shared backend so the CLI's direct-stop path also
    // terminates the recorded child process (SIGTERM -> SIGKILL) and clears
    // state, matching the daemon. This guarantees no AppleVzRunner / qemu
    // orphan remains after `bridgevm stop`.
    stop_backend(store, &args.name)
        .map_err(anyhow::Error::msg)
        .with_context(|| format!("failed to stop VM '{}'", args.name))?;
    println!("Stopped {}", args.name);
    Ok(())
}

fn restart_local(store: &VmStore, args: VmNameArgs) -> Result<()> {
    stop_backend_local(
        store,
        VmNameArgs {
            name: args.name.clone(),
        },
    )?;
    let state = store
        .transition_state(&args.name, VmRuntimeState::Running)
        .with_context(|| format!("failed to restart VM '{}'", args.name))?;
    println!(
        "Metadata state recorded for {} ({})",
        args.name, state.state
    );
    Ok(())
}

fn qmp_socket(store: &VmStore, args: VmNameArgs) -> Result<()> {
    let (bundle, _) = store.get_vm(&args.name).context("failed to read VM")?;
    println!("{}", qmp_socket_path(&bundle).display());
    Ok(())
}

fn qmp_status(store: &VmStore, args: VmNameArgs) -> Result<()> {
    let (bundle, _) = store.get_vm(&args.name).context("failed to read VM")?;
    let path = qmp_socket_path(&bundle);
    if !path.exists() {
        println!("QMP socket unavailable: {}", path.display());
        return Ok(());
    }

    let status = match query_status(&path) {
        Ok(status) => status,
        Err(error) if is_qmp_status_unavailable(&error) => {
            println!("QMP socket unavailable: {}", path.display());
            return Ok(());
        }
        Err(error) => return Err(error).context("failed to query QMP status"),
    };
    println!("QMP status: {}", status.status);
    println!("Running: {}", status.running);
    Ok(())
}

fn qmp_control<F>(store: &VmStore, args: VmNameArgs, command: &str, execute: F) -> Result<()>
where
    F: FnOnce(&Path) -> std::result::Result<(), QemuError>,
{
    let (bundle, _) = store.get_vm(&args.name).context("failed to read VM")?;
    let path = qmp_socket_path(&bundle);
    if !path.exists() {
        bail!("QMP socket unavailable: {}", path.display());
    }

    execute(&path).with_context(|| format!("failed to send QMP {command}"))?;
    println!("QMP command sent: {command}");
    println!("VM: {}", args.name);
    println!("QMP socket: {}", path.display());
    Ok(())
}

fn runner_status(store: &VmStore, args: VmNameArgs) -> Result<()> {
    let metadata = store
        .runner_metadata(&args.name)
        .context("failed to read runner metadata")?;
    let qmp_supervisor = store
        .qmp_supervisor_metadata(&args.name)
        .context("failed to read QMP supervisor metadata")?;
    print_runner_status(metadata, qmp_supervisor.as_ref());
    Ok(())
}

fn print_runner_status(
    metadata: Option<bridgevm_storage::RunnerMetadata>,
    qmp_supervisor: Option<&QmpSupervisorMetadata>,
) {
    match metadata {
        Some(metadata) => {
            println!("Engine: {}", metadata.engine);
            println!(
                "PID: {}",
                metadata
                    .pid
                    .map_or("none".to_string(), |pid| pid.to_string())
            );
            println!("Dry run: {}", metadata.dry_run);
            println!("Metadata recorded: {}", metadata.started_at_unix);
            println!("Log: {}", metadata.log_path.display());
            if let Some(path) = &metadata.launch_spec_path {
                println!("Launch spec: {}", path.display());
            }
            if let Some(guest_tools) = &metadata.guest_tools {
                println!("Guest tools transport: {}", guest_tools.transport);
                println!("Guest tools channel: {}", guest_tools.channel_name);
                println!("Guest tools socket: {}", guest_tools.socket_path.display());
                println!(
                    "Guest tools token file: {}",
                    guest_tools.token_path.display()
                );
                println!(
                    "Guest tools token created: {}",
                    guest_tools.token_created_at_unix
                );
            }
            if let Some(disk) = metadata.disk {
                print_disk_status(&disk);
            }
            if let Some(readiness) = metadata.launch_readiness {
                print_launch_readiness(&readiness);
            }
            println!("Command: {}", metadata.command.join(" "));
        }
        None => println!("No runner metadata"),
    }
    if let Some(supervisor) = qmp_supervisor {
        print_qmp_supervisor(supervisor);
    }
}

fn compatibility_qemu_command_error(error: QemuError) -> String {
    format!("failed to build Compatibility Mode QEMU command: {error}")
}

fn compatibility_launch_readiness_summary(readiness: &LaunchReadinessMetadata) -> String {
    let summary = readiness
        .blockers
        .iter()
        .map(|blocker| blocker.message.as_str())
        .collect::<Vec<_>>()
        .join("; ");
    if summary.is_empty() {
        "Compatibility Mode launch readiness failed".to_string()
    } else {
        format!("Compatibility Mode launch readiness failed: {summary}")
    }
}

fn print_launch_readiness(readiness: &LaunchReadinessMetadata) {
    println!("Launch ready: {}", readiness.ready);
    if readiness.blockers.is_empty() {
        return;
    }
    println!("Launch blockers:");
    for blocker in &readiness.blockers {
        match &blocker.path {
            Some(path) => println!(
                "- {}: {} ({})",
                blocker.code,
                blocker.message,
                path.display()
            ),
            None => println!("- {}: {}", blocker.code, blocker.message),
        }
    }
}

fn print_lifecycle_plan(plan: &LifecyclePlanRecord) {
    println!("Lifecycle plan for {}", plan.vm);
    println!("Action: {}", plan.action);
    println!("Current state: {}", plan.current_state);
    println!("Target state: {}", plan.target_state);
    println!("Backend: {}", plan.backend);
    println!("Metadata only: {}", plan.metadata_only);
    println!("Executable: {}", plan.executable);
    if let Some(command) = &plan.qmp_command {
        println!("QMP command: {}", command);
    }
    if let Some(path) = &plan.socket_path {
        println!("QMP socket: {}", path.display());
    }
    println!("QMP socket available: {}", plan.socket_available);
    if let Some(supervisor) = &plan.qmp_supervisor {
        print_qmp_supervisor(supervisor);
    }
    if plan.blockers.is_empty() {
        println!("Blockers: none");
    } else {
        for blocker in &plan.blockers {
            println!("Blocker: {}", blocker);
        }
    }
    for note in &plan.notes {
        println!("Note: {}", note);
    }
}

fn print_qmp_supervisor(supervisor: &QmpSupervisorMetadata) {
    println!("QMP supervisor events: {}", supervisor.events.len());
    println!(
        "QMP supervisor envelopes read: {}",
        supervisor.envelopes_read
    );
    println!("QMP supervisor limit reached: {}", supervisor.limit_reached);
    println!("QMP supervisor updated at: {}", supervisor.updated_at_unix);
    if let Some(event) = &supervisor.terminal_event {
        println!("QMP supervisor terminal event: {}", event.name);
    }
    for event in &supervisor.events {
        println!("QMP supervisor event: {}", event.name);
    }
}

fn print_readiness_report(report: &VmReadinessReport) {
    println!("Readiness report for {}", report.vm);
    println!("Mode: {}", report.mode);
    println!("State: {}", report.state);
    println!("Metadata only: {}", report.metadata_only);
    println!("Live E2E required: {}", report.live_e2e_required);
    if let Some(evidence) = &report.live_evidence {
        println!("Live evidence: verified ({})", evidence.path.display());
        println!("Live evidence backend: {}", evidence.backend);
        println!("Live evidence VM: {}", evidence.vm_name);
        println!("Live evidence boot mode: {}", evidence.boot_mode);
        println!("Live evidence disk: {}", evidence.disk_format);
        println!("Live evidence network: {}", evidence.network);
        println!(
            "Live evidence serial sentinel: required={} proven={}",
            evidence.serial_sentinel_required, evidence.serial_sentinel_proven
        );
        println!(
            "Live evidence viewer/console: proven={}",
            evidence.viewer_evidence_proven
        );
        println!("Live evidence QMP: proven={}", evidence.qmp_evidence_proven);
        println!(
            "Live evidence guest-tools effects: proven={}",
            evidence.guest_tools_effects_proven
        );
    }
    if !report.evidence_requirements.is_empty() {
        println!("Evidence requirements:");
        for requirement in &report.evidence_requirements {
            println!(
                "- {}: required={} proven={} ({})",
                requirement.kind, requirement.required, requirement.proven, requirement.note
            );
        }
    }
    match &report.boot_media {
        Some(status) => {
            println!("Boot media entries: {}", status.entries.len());
            for entry in &status.entries {
                println!(
                    "Boot media {}: {} ({})",
                    entry.kind,
                    entry.path.display(),
                    if entry.exists { "exists" } else { "missing" }
                );
            }
        }
        None => {
            if let Some(error) = &report.boot_media_error {
                println!("Boot media status error: {}", error);
            } else {
                println!("Boot media: not applicable");
            }
        }
    }
    match &report.snapshot_chain {
        Some(chain) => {
            println!("Active disk: {}", chain.active_disk.path.display());
            println!("Active disk exists: {}", chain.active_disk.exists);
            println!("Snapshot disk entries: {}", chain.disks.len());
        }
        None => {
            if let Some(error) = &report.snapshot_chain_error {
                println!("Snapshot chain error: {}", error);
            }
        }
    }
    match &report.runner {
        Some(runner) => {
            println!("Runner: {}", runner.engine);
            println!("Runner dry run: {}", runner.dry_run);
            if let Some(readiness) = &runner.launch_readiness {
                print_launch_readiness(readiness);
            }
        }
        None => {
            if let Some(error) = &report.runner_error {
                println!("Runner metadata error: {}", error);
            } else {
                println!("Runner: missing metadata");
            }
            if let Some(readiness) = &report.pre_run_launch_readiness {
                println!("Pre-run launch readiness:");
                print_launch_readiness(readiness);
            }
        }
    }
    if let Some(supervisor) = &report.qmp_supervisor {
        print_qmp_supervisor(supervisor);
    }
    if report.blockers.is_empty() {
        println!("Blockers: none");
    } else {
        println!("Blockers:");
        for blocker in &report.blockers {
            println!("- {}", blocker);
        }
    }
    for note in &report.notes {
        println!("Note: {}", note);
    }
}

fn print_network_plan(plan: &NetworkPlanRecord) {
    println!("Network plan for {}", plan.vm);
    println!("Backend: {}", plan.backend);
    println!("Mode: {}", plan.mode);
    println!("Hostname: {}", plan.hostname);
    println!("Dry run: {}", plan.dry_run);
    println!("Executable: {}", plan.executable);
    if let Some(capabilities) = &plan.capabilities {
        println!("Guest outbound: {}", capabilities.guest_outbound);
        println!("Host to guest: {}", capabilities.host_to_guest);
        println!("Guest to host: {}", capabilities.guest_to_host);
        println!(
            "Host visible hostname: {}",
            capabilities.host_visible_hostname
        );
        println!(
            "Supports port forwarding: {}",
            capabilities.supports_port_forwarding
        );
        println!(
            "Requires privileged helper: {}",
            capabilities.requires_privileged_helper
        );
    }
    if plan.port_forwards.is_empty() {
        println!("Port forwards: none");
    } else {
        for forward in &plan.port_forwards {
            println!("Port forward: {}:{}", forward.host, forward.guest);
        }
    }
    if plan.blockers.is_empty() {
        println!("Blockers: none");
    } else {
        for blocker in &plan.blockers {
            println!("Blocker: {} - {}", blocker.code, blocker.message);
        }
    }
    for note in &plan.notes {
        println!("Note: {}", note);
    }
}

fn print_boot_media(name: &str, boot: &AppleVzBootSpec) {
    println!("VM: {name}");
    println!("Boot mode: {}", boot.mode);
    if let Some(path) = &boot.installer_image {
        print_boot_path("Installer image", path);
    }
    if let Some(path) = &boot.kernel {
        print_boot_path("Kernel", path);
    }
    if let Some(path) = &boot.initrd {
        print_boot_path("Initrd", path);
    }
    if let Some(command_line) = &boot.kernel_command_line {
        println!("Kernel command line: {command_line}");
    }
    if let Some(path) = &boot.macos_restore_image {
        print_boot_path("macOS restore image", path);
    }
}

fn print_boot_path(label: &str, path: &AppleVzPathSpec) {
    println!("{label}: {}", path.path);
    println!("{label} exists: {}", path.exists);
}

fn print_boot_media_import(import: &BootMediaImportMetadata) {
    println!("Imported boot media for {}", import.vm);
    println!("Boot media kind: {}", import.kind);
    println!("Source: {}", import.source.display());
    println!("Destination: {}", import.destination.display());
    println!("Bytes: {}", import.bytes);
    println!("Replaced existing media: {}", import.replaced);
    println!("Imported: {}", import.imported_at_unix);
}

fn print_boot_media_status(status: &BootMediaStatus) {
    println!("VM: {}", status.vm);
    if status.entries.is_empty() {
        println!("No boot media entries");
        return;
    }
    for (index, entry) in status.entries.iter().enumerate() {
        if index > 0 {
            println!();
        }
        println!("Boot media kind: {}", entry.kind);
        println!("Path: {}", entry.path.display());
        println!("Exists: {}", entry.exists);
        println!(
            "Bytes: {}",
            entry
                .bytes
                .map_or("unknown".to_string(), |bytes| bytes.to_string())
        );
        if let Some(import) = &entry.last_import {
            println!("Last import source: {}", import.source.display());
            println!("Last import bytes: {}", import.bytes);
            println!("Last import time: {}", import.imported_at_unix);
        } else {
            println!("Last import: none");
        }
        if let Some(verification) = &entry.last_verification {
            println!(
                "Last verification expected: {}",
                verification.expected_sha256
            );
            println!("Last verification actual: {}", verification.actual_sha256);
            println!("Last verification passed: {}", verification.verified);
            println!("Last verification time: {}", verification.verified_at_unix);
        } else {
            println!("Last verification: none");
        }
        if let Some(plan) = &entry.last_download_plan {
            println!("Last download URL: {}", plan.url);
            println!(
                "Last download expected SHA-256: {}",
                plan.expected_sha256.as_deref().unwrap_or("unspecified")
            );
            println!("Last download planned: {}", plan.planned_at_unix);
        } else {
            println!("Last download plan: none");
        }
        if let Some(download) = &entry.last_download {
            println!("Last download completed: {}", download.downloaded);
            println!(
                "Last download bytes: {}",
                download
                    .bytes
                    .map_or("unknown".to_string(), |bytes| bytes.to_string())
            );
            println!("Last download time: {}", download.downloaded_at_unix);
        } else {
            println!("Last download result: none");
        }
    }
}

fn print_guest_tools_status(status: &GuestToolsStatusRecord) {
    println!("Guest tools status for {}", status.vm);
    println!("Tools requirement: {}", status.tools);
    println!("Tools token created: {}", status.token_created_at_unix);
    if status.capabilities.is_empty() {
        println!("No guest tools capabilities allowed");
        return;
    }
    for capability in &status.capabilities {
        println!("Capability: {}", capability.name);
        println!("Max version: {}", capability.max_version);
        println!("Enabled by: {}", capability.enabled_by);
    }
    if status.approved_shared_folders.is_empty() {
        println!("Approved shared folders: 0");
    } else {
        for folder in &status.approved_shared_folders {
            println!("Approved shared folder: {}", folder.name);
            println!("Approved shared folder host path: {}", folder.host_path);
            println!("Approved shared folder token: {}", folder.host_path_token);
            println!("Approved shared folder read-only: {}", folder.read_only);
            println!("Approved shared folder approval: {}", folder.approval);
        }
    }
    if let Some(runtime) = &status.runtime {
        println!("Runtime connected: {}", runtime.connected);
        println!(
            "Runtime guest OS: {}",
            runtime.guest_os.as_deref().unwrap_or("unknown")
        );
        println!(
            "Runtime agent version: {}",
            runtime.agent_version.as_deref().unwrap_or("unknown")
        );
        println!("Runtime updated: {}", runtime.updated_at_unix);
        println!(
            "Runtime last heartbeat: {}",
            runtime
                .last_heartbeat_at_unix
                .map_or("none".to_string(), |timestamp| timestamp.to_string())
        );
        for address in &runtime.guest_ip_addresses {
            println!("Guest IP: {}", address.address);
            println!(
                "Guest IP interface: {}",
                address.interface.as_deref().unwrap_or("unknown")
            );
        }
        if runtime.shared_folders.is_empty() {
            println!("Shared folders mounted: 0");
        } else {
            for folder in &runtime.shared_folders {
                println!("Shared folder: {}", folder.name);
                println!("Shared folder token: {}", folder.host_path_token);
                println!("Shared folder mounted: {}", folder.mounted_at_unix);
            }
        }
        if let Some(metrics) = &runtime.metrics {
            println!("Guest CPU percent: {}", metrics.cpu_percent);
            println!("Guest memory used MiB: {}", metrics.memory_used_mib);
            println!("Guest metrics updated: {}", metrics.updated_at_unix);
        }
        if let Some(clipboard) = &runtime.clipboard {
            println!("Guest clipboard text: {}", clipboard.text);
            println!("Guest clipboard updated: {}", clipboard.updated_at_unix);
        }
        if let Some(result) = &runtime.last_command_result {
            println!("Last command request ID: {}", result.request_id);
            println!(
                "Last command capability: {}",
                result.capability.as_deref().unwrap_or("none")
            );
            println!("Last command OK: {}", result.ok);
            println!(
                "Last command error code: {}",
                result.error_code.as_deref().unwrap_or("none")
            );
            println!(
                "Last command message: {}",
                result.message.as_deref().unwrap_or("none")
            );
            if let Some(payload) = &result.result {
                println!("Last command result JSON:");
                println!(
                    "{}",
                    serde_json::to_string_pretty(payload).unwrap_or_else(|_| payload.to_string())
                );
            }
            if let Some(metadata) = &result.metadata {
                println!("Last command metadata JSON:");
                println!(
                    "{}",
                    serde_json::to_string_pretty(metadata).unwrap_or_else(|_| metadata.to_string())
                );
            }
            println!("Last command completed: {}", result.completed_at_unix);
        }
        if let Some(update) = &runtime.agent_update {
            println!("Agent update current: {}", update.current_version);
            println!("Agent update available: {}", update.available_version);
            println!(
                "Agent update URL: {}",
                update.download_url.as_deref().unwrap_or("none")
            );
            println!(
                "Agent update signature: {}",
                if update.signature.is_some() {
                    "present"
                } else {
                    "none"
                }
            );
            println!("Agent update observed: {}", update.observed_at_unix);
        }
    } else {
        println!("Runtime connected: false");
    }
}

fn print_guest_tools_token(token: &GuestToolsTokenRecord) {
    println!("Guest tools token for {}", token.vm);
    println!("Token: {}", token.token);
    println!("Created: {}", token.created_at_unix);
}

fn print_guest_tools_linux_command(command: &GuestToolsLinuxCommandRecord) {
    for word in &command.command {
        println!("{word}");
    }
}

fn print_guest_tools_session(session: &GuestToolsSessionRecord) {
    println!("Accepted guest tools session for {}", session.vm);
    println!("Guest OS: {}", session.guest_os);
    println!(
        "Agent version: {}",
        session.agent_version.as_deref().unwrap_or("unknown")
    );
    if session.capabilities.is_empty() {
        println!("No advertised capabilities");
        return;
    }
    for capability in &session.capabilities {
        println!("Capability: {}", capability.name);
        println!("Version: {}", capability.version);
    }
}

fn print_boot_media_download(download: &BootMediaDownloadResultMetadata) {
    println!("Downloaded boot media for {}", download.vm);
    println!("Boot media kind: {}", download.kind);
    println!("URL: {}", download.url);
    println!("Destination: {}", download.destination.display());
    println!("Downloaded: {}", download.downloaded);
    println!("Replaced existing media: {}", download.replaced);
    println!(
        "Bytes: {}",
        download
            .bytes
            .map_or("unknown".to_string(), |bytes| bytes.to_string())
    );
    println!(
        "Expected SHA-256: {}",
        download.expected_sha256.as_deref().unwrap_or("unspecified")
    );
    println!(
        "Actual SHA-256: {}",
        download.actual_sha256.as_deref().unwrap_or("unknown")
    );
    println!(
        "Verified: {}",
        download
            .verified
            .map_or("not requested".to_string(), |verified| verified.to_string())
    );
    println!("Downloaded at: {}", download.downloaded_at_unix);
}

fn print_boot_media_download_plan(plan: &BootMediaDownloadPlanMetadata) {
    println!("Planned boot media download for {}", plan.vm);
    println!("Boot media kind: {}", plan.kind);
    println!("URL: {}", plan.url);
    println!("Destination: {}", plan.destination.display());
    println!("Destination exists: {}", plan.exists);
    println!(
        "Destination bytes: {}",
        plan.bytes
            .map_or("unknown".to_string(), |bytes| bytes.to_string())
    );
    println!(
        "Expected SHA-256: {}",
        plan.expected_sha256.as_deref().unwrap_or("unspecified")
    );
    if let Some(import) = &plan.last_import {
        println!("Last import source: {}", import.source.display());
        println!("Last import time: {}", import.imported_at_unix);
    } else {
        println!("Last import: none");
    }
    if let Some(verification) = &plan.last_verification {
        println!("Last verification passed: {}", verification.verified);
        println!("Last verification time: {}", verification.verified_at_unix);
    } else {
        println!("Last verification: none");
    }
    println!("Planned at: {}", plan.planned_at_unix);
}

fn print_boot_media_verification(verification: &BootMediaVerificationMetadata) {
    println!("Verified boot media for {}", verification.vm);
    println!("Boot media kind: {}", verification.kind);
    println!("Path: {}", verification.path.display());
    println!("Bytes: {}", verification.bytes);
    println!("Expected SHA-256: {}", verification.expected_sha256);
    println!("Actual SHA-256: {}", verification.actual_sha256);
    println!("Verified: {}", verification.verified);
    println!("Verified at: {}", verification.verified_at_unix);
}

fn print_disk_status(disk: &bridgevm_storage::DiskPreparationMetadata) {
    println!("Disk: {}", disk.path.display());
    println!("Disk format: {}", disk.format);
    println!("Disk size: {}", disk.size);
    println!("Disk ready: {}", disk.exists);
    println!("Disk created: {}", disk.created);
    if let Some(command) = &disk.create_command {
        println!("Disk create command: {}", command.join(" "));
    }
}

fn print_disk_create_status(metadata: &bridgevm_storage::DiskCreateMetadata) {
    println!("Disk create executed: {}", metadata.executed);
    if let Some(command) = &metadata.command {
        println!("Disk create command: {}", command.join(" "));
    }
    if let Some(status) = &metadata.exit_status {
        println!("Disk create status: {}", status);
    }
    if !metadata.stdout.is_empty() {
        println!("Disk create stdout: {}", metadata.stdout.trim_end());
    }
    if !metadata.stderr.is_empty() {
        println!("Disk create stderr: {}", metadata.stderr.trim_end());
    }
    print_disk_status(&metadata.preparation);
}

fn print_disk_inspect_status(metadata: &bridgevm_storage::DiskInspectMetadata) {
    println!("Disk inspect command: {}", metadata.command.join(" "));
    println!("Disk inspect status: {}", metadata.exit_status);
    println!(
        "Disk inspect duration: {} microseconds",
        metadata.inspect_duration_microseconds
    );
    if !metadata.stderr.is_empty() {
        println!("Disk inspect stderr: {}", metadata.stderr.trim_end());
    }
    print_disk_status(&metadata.preparation);
    println!(
        "Disk info: {}",
        serde_json::to_string_pretty(&metadata.info).unwrap_or_else(|_| metadata.info.to_string())
    );
}

fn print_disk_verify_status(metadata: &bridgevm_storage::DiskVerifyMetadata) {
    println!("Disk verify command: {}", metadata.command.join(" "));
    println!("Disk verify status: {}", metadata.exit_status);
    println!(
        "Disk verify duration: {} microseconds",
        metadata.verify_duration_microseconds
    );
    if !metadata.stderr.is_empty() {
        println!("Disk verify stderr: {}", metadata.stderr.trim_end());
    }
    print_active_disk(&metadata.active_disk);
    println!(
        "Disk verify report: {}",
        serde_json::to_string_pretty(&metadata.report)
            .unwrap_or_else(|_| metadata.report.to_string())
    );
}

fn print_disk_compact_status(metadata: &bridgevm_storage::DiskCompactMetadata) {
    println!("Disk compact command: {}", metadata.command.join(" "));
    println!("Disk compact status: {}", metadata.exit_status);
    println!(
        "Disk compact duration: {} microseconds",
        metadata.compact_duration_microseconds
    );
    println!("Disk compact backup: {}", metadata.backup_path.display());
    println!(
        "Disk compact original bytes: {}",
        metadata.original_size_bytes
    );
    println!(
        "Disk compact compacted bytes: {}",
        metadata.compacted_size_bytes
    );
    if !metadata.stdout.is_empty() {
        println!("Disk compact stdout: {}", metadata.stdout.trim_end());
    }
    if !metadata.stderr.is_empty() {
        println!("Disk compact stderr: {}", metadata.stderr.trim_end());
    }
    print_active_disk(&metadata.active_disk);
}

fn print_port_forwards(ports: &PortForwardListRecord) {
    println!("Port forwards for {}", ports.vm);
    if ports.forwards.is_empty() {
        println!("No port forwards configured");
        return;
    }
    for forward in &ports.forwards {
        println!("{}:{}", forward.host, forward.guest);
    }
}

fn print_shared_folders(shares: &SharedFolderListRecord) {
    println!("Shared folders for {}", shares.vm);
    if shares.shared_folders.is_empty() {
        println!("No shared folders configured");
        return;
    }
    for folder in &shares.shared_folders {
        println!("Shared folder: {}", folder.name);
        println!("Host path: {}", folder.host_path);
        println!("Read-only: {}", folder.read_only);
        println!("Host path token: {}", folder.host_path_token);
    }
}

fn print_ssh_plan(plan: &SshPlanRecord) {
    println!("SSH target for {}", plan.vm);
    println!("Source: {:?}", plan.source);
    println!("Host: {}", plan.host);
    println!("Port: {}", plan.port);
    println!("User: {}", plan.user);
    println!("Command: {}", plan.command.join(" "));
}

fn print_open_port_plan(plan: &OpenPortPlanRecord) {
    println!("Open target for {}", plan.vm);
    println!("Scheme: {}", plan.scheme);
    println!("Host: {}", plan.host);
    println!("URL: {}", plan.url);
    println!("Guest port: {}", plan.guest_port);
    println!("Host port: {}", plan.host_port);
    println!("Command: {}", plan.command.join(" "));
}

fn print_diagnostic_bundle(bundle: &DiagnosticBundleMetadata) {
    println!("Diagnostic bundle for {}", bundle.vm);
    println!("Output: {}", bundle.output.display());
    println!("Files: {}", bundle.files.len());
}

fn print_vm_log(log: &VmLogViewRecord) {
    println!("Log for {}", log.vm);
    println!("Kind: {:?}", log.kind);
    println!("Path: {}", log.path.display());
    println!("Exists: {}", log.exists);
    println!("Bytes: {}", log.bytes);
    println!("Returned bytes: {}", log.returned_bytes);
    println!("Truncated: {}", log.truncated);
    if !log.content.is_empty() {
        println!("--- log tail ---");
        print!("{}", log.content);
        if !log.content.ends_with('\n') {
            println!();
        }
    }
}

fn print_performance_baseline(baseline: &PerformanceBaselineMetadata) {
    println!("Performance baseline for {}", baseline.vm);
    println!("Output: {}", baseline.output.display());
    println!("Artifact: {}", baseline.artifact.display());
    println!("Metadata only: {}", baseline.metadata_only);
    println!("State: {}", baseline.state.state);
    match &baseline.runner {
        Some(runner) => {
            println!("Runner: {}", runner.engine);
            println!("Runner dry run: {}", runner.dry_run);
        }
        None => println!("Runner: unavailable"),
    }
    match &baseline.metrics {
        Some(metrics) => {
            println!("Guest CPU: {}%", metrics.cpu_percent);
            println!("Guest memory: {} MiB", metrics.memory_used_mib);
        }
        None => println!("Guest metrics: unavailable"),
    }
    print_performance_measurements(&baseline.measurements);
}

fn print_performance_sample(sample: &PerformanceSampleMetadata) {
    println!("Performance sample for {}", sample.vm);
    println!("Output: {}", sample.output.display());
    println!("Artifact: {}", sample.artifact.display());
    println!("Probe: {}", sample.probe.display());
    println!("Probes: {}", sample.probes.len());
    println!("Probe bytes: {}", sample.artifact_bytes);
    println!("Iterations: {}", sample.iterations);
    println!("Sync: {}", sample.sync);
    println!("State: {}", sample.state.state);
    print_performance_measurements(&sample.measurements);
}

fn print_performance_measurements(measurements: &[bridgevm_api::PerformanceMeasurementRecord]) {
    println!("Measurements: {}", measurements.len());
    for measurement in measurements {
        println!(
            "Measurement: {}={} {} ({})",
            measurement.name, measurement.value, measurement.unit, measurement.source
        );
    }
}

fn print_snapshot_chain(chain: &bridgevm_storage::SnapshotChainMetadata) {
    print_active_disk(&chain.active_disk);
    if chain.disks.is_empty() {
        println!("No disk snapshot chain metadata");
        return;
    }
    for disk in &chain.disks {
        println!("Snapshot disk: {}", disk.snapshot);
        print_snapshot_disk_status(disk);
    }
}

fn print_active_disk(active_disk: &bridgevm_storage::ActiveDiskMetadata) {
    println!("Active disk source: {}", active_disk.source);
    if let Some(snapshot) = &active_disk.snapshot {
        println!("Active disk snapshot: {}", snapshot);
    }
    println!("Active disk: {}", active_disk.path.display());
    println!("Active disk format: {}", active_disk.format);
    println!("Active disk ready: {}", active_disk.exists);
    println!("Active disk activated: {}", active_disk.activated_at_unix);
}

fn print_snapshot_disk_status(metadata: &bridgevm_storage::SnapshotDiskMetadata) {
    println!("Snapshot disk overlay: {}", metadata.overlay_path.display());
    println!("Snapshot disk overlay ready: {}", metadata.overlay_exists);
    println!("Snapshot disk backing: {}", metadata.backing_path.display());
    println!("Snapshot disk backing format: {}", metadata.backing_format);
    println!("Snapshot disk backing ready: {}", metadata.backing_exists);
    println!(
        "Snapshot disk create command: {}",
        metadata.create_command.join(" ")
    );
}

fn print_snapshot_disk_create_status(metadata: &bridgevm_storage::SnapshotDiskCreateMetadata) {
    println!("Snapshot disk create executed: {}", metadata.executed);
    println!(
        "Snapshot disk create command: {}",
        metadata.command.join(" ")
    );
    if let Some(status) = &metadata.exit_status {
        println!("Snapshot disk create status: {}", status);
    }
    if !metadata.stdout.is_empty() {
        println!(
            "Snapshot disk create stdout: {}",
            metadata.stdout.trim_end()
        );
    }
    if !metadata.stderr.is_empty() {
        println!(
            "Snapshot disk create stderr: {}",
            metadata.stderr.trim_end()
        );
    }
    print_snapshot_disk_status(&metadata.disk);
}

fn print_snapshot_suspend_image_status(metadata: &bridgevm_storage::SnapshotSuspendImageMetadata) {
    println!("Suspend image: {}", metadata.image_path.display());
    println!("Suspend image format: {}", metadata.image_format);
    println!("Suspend image ready: {}", metadata.image_exists);
    println!("Suspend image prepared: {}", metadata.prepared_at_unix);
}

fn print_application_consistent_snapshot_preflight(
    metadata: &ApplicationConsistentSnapshotPreflightMetadata,
) {
    println!("Application-consistent preflight: {}", metadata.snapshot);
    println!("Guest tools connected: {}", metadata.connected);
    println!(
        "Required capabilities: {}",
        metadata.required_capabilities.join(", ")
    );
    println!(
        "Available capabilities: {}",
        if metadata.available_capabilities.is_empty() {
            "none".to_string()
        } else {
            metadata.available_capabilities.join(", ")
        }
    );
    println!(
        "Missing capabilities: {}",
        if metadata.missing_capabilities.is_empty() {
            "none".to_string()
        } else {
            metadata.missing_capabilities.join(", ")
        }
    );
    println!("Application-consistent ready: {}", metadata.ready);
    println!("Planned freeze: {}", metadata.planned_freeze_semantics);
    println!("Planned thaw: {}", metadata.planned_thaw_semantics);
    println!(
        "Guest tools runtime updated: {}",
        metadata
            .runtime_updated_at_unix
            .map_or("unknown".to_string(), |updated| updated.to_string())
    );
    println!("Preflight prepared: {}", metadata.prepared_at_unix);
}

fn print_snapshot_preflight_status(metadata: &SnapshotPreflightStatusRecord) {
    println!("Snapshot preflight for {}", metadata.vm);
    println!("Consistency: {:?}", metadata.consistency);
    println!(
        "Backend freeze/thaw supported: {}",
        metadata.backend_freeze_thaw_supported
    );
    println!("Guest tools connected: {}", metadata.guest_tools_connected);
    println!(
        "Capabilities: {}",
        if metadata.capabilities.is_empty() {
            "none".to_string()
        } else {
            metadata.capabilities.join(", ")
        }
    );
    println!("Preflight ready: {}", metadata.ready);
    if metadata.blockers.is_empty() {
        println!("Blockers: none");
    } else {
        for blocker in &metadata.blockers {
            if let Some(path) = &blocker.path {
                println!(
                    "Blocker: {} - {} ({})",
                    blocker.code,
                    blocker.message,
                    path.display()
                );
            } else {
                println!("Blocker: {} - {}", blocker.code, blocker.message);
            }
        }
    }
    println!("Checked: {}", metadata.checked_at_unix);
}

fn print_application_consistent_snapshot_execution(
    execution: &ApplicationConsistentSnapshotExecutionRecord,
) {
    println!(
        "Application-consistent snapshot execution for {}",
        execution.vm
    );
    println!("Snapshot: {}", execution.snapshot);
    println!("Freeze request ID: {}", execution.freeze_request_id);
    println!("Thaw request ID: {}", execution.thaw_request_id);
    println!(
        "Pending after freeze: {}",
        execution.pending_commands_after_freeze
    );
    println!(
        "Pending after thaw: {}",
        execution.pending_commands_after_thaw
    );
    println!(
        "Freeze result: {} ({})",
        execution.freeze_result.ok,
        execution
            .freeze_result
            .message
            .as_deref()
            .unwrap_or("no message")
    );
    println!(
        "Thaw result: {} ({})",
        execution.thaw_result.ok,
        execution
            .thaw_result
            .message
            .as_deref()
            .unwrap_or("no message")
    );
    println!("Preflight ready: {}", execution.preflight_ready);
    println!("Snapshot created: {}", execution.snapshot_created_at_unix);
    println!("Note: {}", execution.note);
}

fn recommend(args: GuestArgs) -> Result<()> {
    let rec = recommend_mode(&GuestChoice {
        os: args.os,
        version: args.version,
        arch: args.arch,
    });
    print_mode_recommendation(&rec);
    Ok(())
}

fn print_mode_recommendation(rec: &ModeRecommendation) {
    println!("Recommended mode: {}", rec.mode);
    println!("Expected performance: {}", rec.performance);
    println!("Battery impact: {}", rec.battery_impact);
    println!("Integration: {}", rec.integration);
    println!("{}", rec.message);
    if let Some(template) = &rec.boot_template {
        print_boot_template(template);
    }
}

fn print_boot_template(template: &BootTemplate) {
    println!("Boot template id: {}", template.id);
    println!("Guest: {} {}", template.guest_os, template.guest_arch);
    println!("Boot template: {}", template.mode);
    println!("Boot media: {}", template.media_label);
    println!("Boot media source: {}", template.source);
    if let Some(path) = &template.installer_image {
        println!("Installer image: {path}");
    }
    if let Some(path) = &template.kernel_path {
        println!("Kernel path: {path}");
    }
    if let Some(path) = &template.initrd_path {
        println!("Initrd path: {path}");
    }
    if let Some(command_line) = &template.kernel_command_line {
        println!("Kernel command line: {command_line}");
    }
    if let Some(path) = &template.macos_restore_image {
        println!("macOS restore image: {path}");
    }
    println!("Boot note: {}", template.note);
}

fn print_boot_templates(templates: &[BootTemplate]) {
    if templates.is_empty() {
        println!("No boot templates available");
        return;
    }
    for (index, template) in templates.iter().enumerate() {
        if index > 0 {
            println!();
        }
        print_boot_template(template);
    }
}

fn doctor(store: &VmStore) -> Result<()> {
    store.ensure().context("failed to prepare BridgeVM store")?;
    println!("BridgeVM store: {}", store.root().display());
    println!("VM bundles: {}", store.vms_dir().display());
    print_doctor_audit(&doctor_audit_for_current_host(store));
    println!("Status: OK");
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DoctorCheckStatus {
    Ok,
    Warn,
    Missing,
}

impl DoctorCheckStatus {
    fn as_str(self) -> &'static str {
        match self {
            DoctorCheckStatus::Ok => "OK",
            DoctorCheckStatus::Warn => "WARN",
            DoctorCheckStatus::Missing => "MISSING",
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
struct DoctorCheck {
    status: DoctorCheckStatus,
    name: String,
    detail: String,
}

#[derive(Debug)]
struct DoctorAuditInput {
    store_root: PathBuf,
    vms_dir: PathBuf,
    path_dirs: Vec<PathBuf>,
    os: String,
    arch: String,
}

fn doctor_audit_for_current_host(store: &VmStore) -> Vec<DoctorCheck> {
    doctor_audit_for_paths(store.root(), &store.vms_dir())
}

fn doctor_audit_for_paths(store_root: &Path, vms_dir: &Path) -> Vec<DoctorCheck> {
    let path_dirs = env::var_os("PATH")
        .map(|path| env::split_paths(&path).collect())
        .unwrap_or_default();
    doctor_audit(&DoctorAuditInput {
        store_root: store_root.to_path_buf(),
        vms_dir: vms_dir.to_path_buf(),
        path_dirs,
        os: env::consts::OS.to_string(),
        arch: env::consts::ARCH.to_string(),
    })
}

fn doctor_audit(input: &DoctorAuditInput) -> Vec<DoctorCheck> {
    let qemu_img = find_executable("qemu-img", &input.path_dirs);
    let qemu_aarch64 = find_executable("qemu-system-aarch64", &input.path_dirs);
    let qemu_x86_64 = find_executable("qemu-system-x86_64", &input.path_dirs);
    let lightvm_runner = find_executable("lightvm-runner", &input.path_dirs);
    let fullvm_runner = find_executable("fullvm-runner", &input.path_dirs);
    let networkd = find_executable("networkd", &input.path_dirs);
    let is_macos = input.os == "macos";
    let is_apple_silicon = matches!(input.arch.as_str(), "aarch64" | "arm64");

    let mut checks = Vec::new();
    checks.push(path_dir_check("Store root", &input.store_root));
    checks.push(path_dir_check("VM bundles dir", &input.vms_dir));
    checks.push(executable_check(
        "qemu-img",
        qemu_img.as_deref(),
        "required for qcow2 disk creation and snapshot overlays",
    ));

    match (qemu_aarch64.as_deref(), qemu_x86_64.as_deref()) {
        (Some(aarch64), Some(x86_64)) => checks.push(DoctorCheck {
            status: DoctorCheckStatus::Ok,
            name: "QEMU system binary".to_string(),
            detail: format!(
                "found qemu-system-aarch64 at {} and qemu-system-x86_64 at {}",
                aarch64.display(),
                x86_64.display()
            ),
        }),
        (Some(path), None) => checks.push(DoctorCheck {
            status: DoctorCheckStatus::Ok,
            name: "QEMU system binary".to_string(),
            detail: format!("found qemu-system-aarch64 at {}", path.display()),
        }),
        (None, Some(path)) => checks.push(DoctorCheck {
            status: DoctorCheckStatus::Ok,
            name: "QEMU system binary".to_string(),
            detail: format!("found qemu-system-x86_64 at {}", path.display()),
        }),
        (None, None) => checks.push(DoctorCheck {
            status: DoctorCheckStatus::Missing,
            name: "QEMU system binary".to_string(),
            detail: "qemu-system-aarch64 or qemu-system-x86_64 was not found on PATH".to_string(),
        }),
    }

    checks.push(optional_executable_check(
        "lightvm-runner",
        lightvm_runner.as_deref(),
        "Fast Mode runner candidate was not found on PATH",
    ));
    checks.push(optional_executable_check(
        "fullvm-runner",
        fullvm_runner.as_deref(),
        "Compatibility Mode runner candidate was not found on PATH",
    ));
    checks.push(optional_executable_check(
        "networkd",
        networkd.as_deref(),
        "network helper candidate was not found on PATH",
    ));
    checks.push(DoctorCheck {
        status: if is_macos {
            DoctorCheckStatus::Ok
        } else {
            DoctorCheckStatus::Warn
        },
        name: "macOS host".to_string(),
        detail: if is_macos {
            "current host reports macos".to_string()
        } else {
            format!(
                "current host reports {}; Apple Virtualization is macOS-only",
                input.os
            )
        },
    });
    checks.push(DoctorCheck {
        status: if is_apple_silicon {
            DoctorCheckStatus::Ok
        } else {
            DoctorCheckStatus::Warn
        },
        name: "Apple Silicon host".to_string(),
        detail: if is_apple_silicon {
            format!("current host arch is {}", input.arch)
        } else {
            format!(
                "current host arch is {}; arm64/aarch64 is expected for Fast Mode",
                input.arch
            )
        },
    });
    checks.push(DoctorCheck {
        status: if is_macos && is_apple_silicon && lightvm_runner.is_some() {
            DoctorCheckStatus::Ok
        } else if is_macos && is_apple_silicon {
            DoctorCheckStatus::Warn
        } else {
            DoctorCheckStatus::Missing
        },
        name: "Fast Mode possibility".to_string(),
        detail: fast_mode_detail(is_macos, is_apple_silicon, lightvm_runner.as_deref()),
    });

    checks
}

fn path_dir_check(name: &str, path: &Path) -> DoctorCheck {
    if path.is_dir() {
        DoctorCheck {
            status: DoctorCheckStatus::Ok,
            name: name.to_string(),
            detail: format!("{} exists", path.display()),
        }
    } else if path.exists() {
        DoctorCheck {
            status: DoctorCheckStatus::Missing,
            name: name.to_string(),
            detail: format!("{} exists but is not a directory", path.display()),
        }
    } else {
        DoctorCheck {
            status: DoctorCheckStatus::Missing,
            name: name.to_string(),
            detail: format!("{} does not exist", path.display()),
        }
    }
}

fn executable_check(name: &str, path: Option<&Path>, missing_detail: &str) -> DoctorCheck {
    match path {
        Some(path) => DoctorCheck {
            status: DoctorCheckStatus::Ok,
            name: name.to_string(),
            detail: format!("found at {}", path.display()),
        },
        None => DoctorCheck {
            status: DoctorCheckStatus::Missing,
            name: name.to_string(),
            detail: missing_detail.to_string(),
        },
    }
}

fn optional_executable_check(name: &str, path: Option<&Path>, missing_detail: &str) -> DoctorCheck {
    match path {
        Some(path) => DoctorCheck {
            status: DoctorCheckStatus::Ok,
            name: name.to_string(),
            detail: format!("found at {}", path.display()),
        },
        None => DoctorCheck {
            status: DoctorCheckStatus::Warn,
            name: name.to_string(),
            detail: missing_detail.to_string(),
        },
    }
}

fn fast_mode_detail(is_macos: bool, is_apple_silicon: bool, runner: Option<&Path>) -> String {
    match (is_macos, is_apple_silicon, runner) {
        (true, true, Some(path)) => {
            format!(
                "macOS Apple Silicon host with lightvm-runner at {}",
                path.display()
            )
        }
        (true, true, None) => {
            "macOS Apple Silicon host detected, but lightvm-runner is not on PATH".to_string()
        }
        (false, _, _) => "Fast Mode requires macOS with Apple Virtualization".to_string(),
        (true, false, _) => "Fast Mode requires an Apple Silicon host".to_string(),
    }
}

fn find_executable(name: &str, path_dirs: &[PathBuf]) -> Option<PathBuf> {
    path_dirs
        .iter()
        .map(|dir| dir.join(name))
        .find(|candidate| is_executable_file(candidate))
}

fn is_executable_file(path: &Path) -> bool {
    path.is_file()
        && path
            .metadata()
            .map(|m| m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
}

fn print_doctor_audit(checks: &[DoctorCheck]) {
    println!("Host capability audit:");
    for check in checks {
        println!(
            "[{}] {}: {}",
            check.status.as_str(),
            check.name,
            check.detail
        );
    }
}

impl From<SnapshotKindChoice> for SnapshotKind {
    fn from(value: SnapshotKindChoice) -> Self {
        match value {
            SnapshotKindChoice::Disk => SnapshotKind::Disk,
            SnapshotKindChoice::Suspend => SnapshotKind::Suspend,
            SnapshotKindChoice::ApplicationConsistent => SnapshotKind::ApplicationConsistent,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_store(prefix: &str) -> VmStore {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "{prefix}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        VmStore::new(root)
    }

    fn write_executable(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, "#!/bin/sh\n").unwrap();
        let mut permissions = fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&path, permissions).unwrap();
        path
    }

    fn test_manifest(name: &str) -> VmManifest {
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

    fn compatibility_manifest(name: &str) -> VmManifest {
        VmManifest::new(
            name,
            VmMode::Compatibility,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "x86_64".to_string(),
            },
            "64GiB",
        )
    }

    #[test]
    fn doctor_audit_reports_ready_macos_apple_silicon_host() {
        let store = unique_store("bridgevm-cli-doctor-ready-test");
        store.ensure().unwrap();
        let bin_dir = store.root().join("bin");
        fs::create_dir_all(&bin_dir).unwrap();
        let qemu_img = write_executable(&bin_dir, "qemu-img");
        let qemu_system = write_executable(&bin_dir, "qemu-system-aarch64");
        let lightvm_runner = write_executable(&bin_dir, "lightvm-runner");
        let fullvm_runner = write_executable(&bin_dir, "fullvm-runner");
        let networkd = write_executable(&bin_dir, "networkd");

        let checks = doctor_audit(&DoctorAuditInput {
            store_root: store.root().to_path_buf(),
            vms_dir: store.vms_dir().to_path_buf(),
            path_dirs: vec![bin_dir],
            os: "macos".to_string(),
            arch: "aarch64".to_string(),
        });

        assert!(checks.contains(&DoctorCheck {
            status: DoctorCheckStatus::Ok,
            name: "Store root".to_string(),
            detail: format!("{} exists", store.root().display()),
        }));
        assert!(checks.contains(&DoctorCheck {
            status: DoctorCheckStatus::Ok,
            name: "qemu-img".to_string(),
            detail: format!("found at {}", qemu_img.display()),
        }));
        assert!(checks.contains(&DoctorCheck {
            status: DoctorCheckStatus::Ok,
            name: "QEMU system binary".to_string(),
            detail: format!("found qemu-system-aarch64 at {}", qemu_system.display()),
        }));
        assert!(checks.contains(&DoctorCheck {
            status: DoctorCheckStatus::Ok,
            name: "lightvm-runner".to_string(),
            detail: format!("found at {}", lightvm_runner.display()),
        }));
        assert!(checks.contains(&DoctorCheck {
            status: DoctorCheckStatus::Ok,
            name: "fullvm-runner".to_string(),
            detail: format!("found at {}", fullvm_runner.display()),
        }));
        assert!(checks.contains(&DoctorCheck {
            status: DoctorCheckStatus::Ok,
            name: "networkd".to_string(),
            detail: format!("found at {}", networkd.display()),
        }));
        assert!(checks.contains(&DoctorCheck {
            status: DoctorCheckStatus::Ok,
            name: "Fast Mode possibility".to_string(),
            detail: format!(
                "macOS Apple Silicon host with lightvm-runner at {}",
                lightvm_runner.display()
            ),
        }));
    }

    #[test]
    fn doctor_audit_reports_missing_tools_without_machine_dependencies() {
        let store = unique_store("bridgevm-cli-doctor-missing-test");
        let checks = doctor_audit(&DoctorAuditInput {
            store_root: store.root().to_path_buf(),
            vms_dir: store.vms_dir().to_path_buf(),
            path_dirs: Vec::new(),
            os: "linux".to_string(),
            arch: "x86_64".to_string(),
        });

        assert!(checks.contains(&DoctorCheck {
            status: DoctorCheckStatus::Missing,
            name: "Store root".to_string(),
            detail: format!("{} does not exist", store.root().display()),
        }));
        assert!(checks.contains(&DoctorCheck {
            status: DoctorCheckStatus::Missing,
            name: "qemu-img".to_string(),
            detail: "required for qcow2 disk creation and snapshot overlays".to_string(),
        }));
        assert!(checks.contains(&DoctorCheck {
            status: DoctorCheckStatus::Missing,
            name: "QEMU system binary".to_string(),
            detail: "qemu-system-aarch64 or qemu-system-x86_64 was not found on PATH".to_string(),
        }));
        assert!(checks.contains(&DoctorCheck {
            status: DoctorCheckStatus::Warn,
            name: "networkd".to_string(),
            detail: "network helper candidate was not found on PATH".to_string(),
        }));
        assert!(checks.contains(&DoctorCheck {
            status: DoctorCheckStatus::Warn,
            name: "macOS host".to_string(),
            detail: "current host reports linux; Apple Virtualization is macOS-only".to_string(),
        }));
        assert!(checks.contains(&DoctorCheck {
            status: DoctorCheckStatus::Missing,
            name: "Fast Mode possibility".to_string(),
            detail: "Fast Mode requires macOS with Apple Virtualization".to_string(),
        }));
    }

    #[test]
    fn guest_tools_mount_share_cli_builds_host_command_envelope() {
        let cli = Cli::try_parse_from([
            "bridgevm",
            "guest-tools",
            "mount-share",
            "dev",
            "--name",
            "work",
            "--host-path-token",
            "share-token-1",
            "--request-id",
            "mount-1",
        ])
        .unwrap();

        let request = request_for(cli.command).unwrap();
        let BridgeVmRequest::GuestToolsSendCommand { name, envelope } = request else {
            panic!("expected guest tools send command request");
        };

        assert_eq!(name, "dev");
        assert_eq!(envelope.request_id.as_deref(), Some("mount-1"));
        assert_eq!(
            envelope.message,
            AgentMessage::MountShare {
                name: "work".to_string(),
                host_path_token: "share-token-1".to_string(),
            }
        );
    }

    #[test]
    fn guest_tools_set_clipboard_cli_builds_host_command_envelope() {
        let cli = Cli::try_parse_from([
            "bridgevm",
            "guest-tools",
            "set-clipboard",
            "dev",
            "--text",
            "hello from host",
            "--request-id",
            "clipboard-1",
        ])
        .unwrap();

        let request = request_for(cli.command).unwrap();
        let BridgeVmRequest::GuestToolsSendCommand { name, envelope } = request else {
            panic!("expected guest tools send command request");
        };

        assert_eq!(name, "dev");
        assert_eq!(envelope.request_id.as_deref(), Some("clipboard-1"));
        assert_eq!(
            envelope.message,
            AgentMessage::SetClipboard {
                text: "hello from host".to_string(),
            }
        );
    }

    #[test]
    fn guest_tools_resize_display_cli_builds_host_command_envelope() {
        let cli = Cli::try_parse_from([
            "bridgevm",
            "guest-tools",
            "resize-display",
            "dev",
            "--width",
            "1440",
            "--height",
            "900",
            "--scale",
            "2",
            "--request-id",
            "resize-1",
        ])
        .unwrap();

        let request = request_for(cli.command).unwrap();
        let BridgeVmRequest::GuestToolsSendCommand { name, envelope } = request else {
            panic!("expected guest tools send command request");
        };

        assert_eq!(name, "dev");
        assert_eq!(envelope.request_id.as_deref(), Some("resize-1"));
        assert_eq!(
            envelope.message,
            AgentMessage::ResizeDisplay {
                width: 1440,
                height: 900,
                scale: 2,
            }
        );
    }

    #[test]
    fn qmp_control_cli_builds_typed_requests() {
        let cli = Cli::try_parse_from(["bridgevm", "qmp-stop", "dev"]).unwrap();
        let request = request_for(cli.command).unwrap();
        let BridgeVmRequest::QmpStop { name } = request else {
            panic!("expected qmp stop request");
        };
        assert_eq!(name, "dev");

        let cli = Cli::try_parse_from(["bridgevm", "qmp-cont", "dev"]).unwrap();
        let request = request_for(cli.command).unwrap();
        let BridgeVmRequest::QmpCont { name } = request else {
            panic!("expected qmp cont request");
        };
        assert_eq!(name, "dev");
    }

    #[test]
    fn lifecycle_plan_cli_builds_typed_request() {
        let cli = Cli::try_parse_from(["bridgevm", "lifecycle-plan", "dev", "--action", "resume"])
            .unwrap();
        let request = request_for(cli.command).unwrap();
        assert_eq!(
            request,
            BridgeVmRequest::LifecyclePlan {
                name: "dev".to_string(),
                action: LifecycleAction::Resume,
            }
        );
    }

    #[test]
    fn readiness_cli_builds_typed_request() {
        let cli = Cli::try_parse_from(["bridgevm", "readiness", "dev"]).unwrap();
        let request = request_for(cli.command).unwrap();
        assert_eq!(
            request,
            BridgeVmRequest::ReadinessReport {
                name: "dev".to_string(),
                live_evidence: None,
                record_live_evidence: false,
                clear_live_evidence: false,
            }
        );

        let cli = Cli::try_parse_from([
            "bridgevm",
            "readiness",
            "dev",
            "--live-evidence",
            "/tmp/live",
            "--record-live-evidence",
        ])
        .unwrap();
        let request = request_for(cli.command).unwrap();
        assert_eq!(
            request,
            BridgeVmRequest::ReadinessReport {
                name: "dev".to_string(),
                live_evidence: Some(PathBuf::from("/tmp/live")),
                record_live_evidence: true,
                clear_live_evidence: false,
            }
        );

        let cli =
            Cli::try_parse_from(["bridgevm", "readiness", "dev", "--clear-live-evidence"]).unwrap();
        let request = request_for(cli.command).unwrap();
        assert_eq!(
            request,
            BridgeVmRequest::ReadinessReport {
                name: "dev".to_string(),
                live_evidence: None,
                record_live_evidence: false,
                clear_live_evidence: true,
            }
        );
    }

    #[test]
    fn application_consistent_snapshot_cli_builds_typed_request() {
        let cli = Cli::try_parse_from([
            "bridgevm",
            "snapshot",
            "create",
            "dev",
            "before-upgrade",
            "--kind",
            "application-consistent",
        ])
        .unwrap();

        let request = request_for(cli.command).unwrap();
        let BridgeVmRequest::CreateSnapshot { vm, name, kind } = request else {
            panic!("expected create snapshot request");
        };

        assert_eq!(vm, "dev");
        assert_eq!(name, "before-upgrade");
        assert_eq!(kind, SnapshotKind::ApplicationConsistent);

        let cli = Cli::try_parse_from([
            "bridgevm",
            "snapshot",
            "execute-application-consistent",
            "dev",
            "before-upgrade",
            "--freeze-timeout-millis",
            "5000",
        ])
        .unwrap();

        let request = request_for(cli.command).unwrap();
        let BridgeVmRequest::ExecuteApplicationConsistentSnapshot {
            vm,
            name,
            freeze_timeout_millis,
        } = request
        else {
            panic!("expected execute application-consistent snapshot request");
        };

        assert_eq!(vm, "dev");
        assert_eq!(name, "before-upgrade");
        assert_eq!(freeze_timeout_millis, Some(5_000));
    }

    #[test]
    fn snapshot_restore_cli_builds_typed_request() {
        let cli = Cli::try_parse_from(["bridgevm", "snapshot", "restore", "dev", "before-upgrade"])
            .unwrap();

        let request = request_for(cli.command).unwrap();
        assert_eq!(
            request,
            BridgeVmRequest::RestoreSnapshot {
                vm: "dev".to_string(),
                name: "before-upgrade".to_string(),
            }
        );
    }

    #[test]
    fn guest_tools_mount_approved_share_cli_builds_named_share_request() {
        let cli = Cli::try_parse_from([
            "bridgevm",
            "guest-tools",
            "mount-approved-share",
            "dev",
            "--share",
            "work",
            "--request-id",
            "mount-1",
        ])
        .unwrap();

        let request = request_for(cli.command).unwrap();
        assert_eq!(
            request,
            BridgeVmRequest::GuestToolsMountApprovedShare {
                name: "dev".to_string(),
                share: "work".to_string(),
                request_id: Some("mount-1".to_string()),
            }
        );
    }

    #[test]
    fn guest_tools_filesystem_cli_builds_host_command_envelopes() {
        let freeze = Cli::try_parse_from([
            "bridgevm",
            "guest-tools",
            "freeze-filesystem",
            "dev",
            "--request-id",
            "freeze-1",
            "--timeout-millis",
            "5000",
        ])
        .unwrap();
        let request = request_for(freeze.command).unwrap();
        let BridgeVmRequest::GuestToolsSendCommand { name, envelope } = request else {
            panic!("expected guest tools send command request");
        };

        assert_eq!(name, "dev");
        assert_eq!(envelope.request_id.as_deref(), Some("freeze-1"));
        assert_eq!(
            envelope.message,
            AgentMessage::FreezeFilesystem {
                timeout_millis: Some(5_000),
            }
        );

        let thaw = Cli::try_parse_from([
            "bridgevm",
            "guest-tools",
            "thaw-filesystem",
            "dev",
            "--request-id",
            "thaw-1",
        ])
        .unwrap();
        let request = request_for(thaw.command).unwrap();
        let BridgeVmRequest::GuestToolsSendCommand { name, envelope } = request else {
            panic!("expected guest tools send command request");
        };

        assert_eq!(name, "dev");
        assert_eq!(envelope.request_id.as_deref(), Some("thaw-1"));
        assert_eq!(envelope.message, AgentMessage::ThawFilesystem);
    }

    #[test]
    fn guest_tools_unmount_share_cli_builds_host_command_envelope() {
        let cli = Cli::try_parse_from([
            "bridgevm",
            "guest-tools",
            "unmount-share",
            "dev",
            "--name",
            "work",
            "--request-id",
            "unmount-1",
        ])
        .unwrap();

        let request = request_for(cli.command).unwrap();
        let BridgeVmRequest::GuestToolsSendCommand { name, envelope } = request else {
            panic!("expected guest tools send command request");
        };

        assert_eq!(name, "dev");
        assert_eq!(envelope.request_id.as_deref(), Some("unmount-1"));
        assert_eq!(
            envelope.message,
            AgentMessage::UnmountShare {
                name: "work".to_string(),
            }
        );
    }

    #[test]
    fn guest_tools_file_drop_cli_builds_host_command_envelopes() {
        let start = Cli::try_parse_from([
            "bridgevm",
            "guest-tools",
            "file-drop-start",
            "dev",
            "--transfer-id",
            "drop-1",
            "--file-name",
            "notes.txt",
            "--size-bytes",
            "12",
            "--request-id",
            "drop-start-1",
        ])
        .unwrap();
        let request = request_for(start.command).unwrap();
        let BridgeVmRequest::GuestToolsSendCommand { name, envelope } = request else {
            panic!("expected guest tools send command request");
        };

        assert_eq!(name, "dev");
        assert_eq!(envelope.request_id.as_deref(), Some("drop-start-1"));
        assert_eq!(
            envelope.message,
            AgentMessage::FileDropStart {
                transfer_id: "drop-1".to_string(),
                file_name: "notes.txt".to_string(),
                size_bytes: 12,
            }
        );

        let chunk = Cli::try_parse_from([
            "bridgevm",
            "guest-tools",
            "file-drop-chunk",
            "dev",
            "--transfer-id",
            "drop-1",
            "--chunk-index",
            "0",
            "--data-base64",
            "aGVsbG8=",
            "--request-id",
            "drop-chunk-1",
        ])
        .unwrap();
        let request = request_for(chunk.command).unwrap();
        let BridgeVmRequest::GuestToolsSendCommand { name, envelope } = request else {
            panic!("expected guest tools send command request");
        };

        assert_eq!(name, "dev");
        assert_eq!(envelope.request_id.as_deref(), Some("drop-chunk-1"));
        assert_eq!(
            envelope.message,
            AgentMessage::FileDropChunk {
                transfer_id: "drop-1".to_string(),
                chunk_index: 0,
                data_base64: "aGVsbG8=".to_string(),
            }
        );

        let complete = Cli::try_parse_from([
            "bridgevm",
            "guest-tools",
            "file-drop-complete",
            "dev",
            "--transfer-id",
            "drop-1",
            "--request-id",
            "drop-complete-1",
        ])
        .unwrap();
        let request = request_for(complete.command).unwrap();
        let BridgeVmRequest::GuestToolsSendCommand { name, envelope } = request else {
            panic!("expected guest tools send command request");
        };

        assert_eq!(name, "dev");
        assert_eq!(envelope.request_id.as_deref(), Some("drop-complete-1"));
        assert_eq!(
            envelope.message,
            AgentMessage::FileDropComplete {
                transfer_id: "drop-1".to_string(),
            }
        );
    }

    #[test]
    fn guest_tools_application_cli_builds_host_command_envelopes() {
        let list = Cli::try_parse_from([
            "bridgevm",
            "guest-tools",
            "list-applications",
            "dev",
            "--request-id",
            "apps-1",
        ])
        .unwrap();
        let request = request_for(list.command).unwrap();
        let BridgeVmRequest::GuestToolsSendCommand { name, envelope } = request else {
            panic!("expected guest tools send command request");
        };

        assert_eq!(name, "dev");
        assert_eq!(envelope.request_id.as_deref(), Some("apps-1"));
        assert_eq!(envelope.message, AgentMessage::ListApplications);

        let launch = Cli::try_parse_from([
            "bridgevm",
            "guest-tools",
            "launch-application",
            "dev",
            "--id",
            "terminal",
            "--request-id",
            "launch-1",
        ])
        .unwrap();
        let request = request_for(launch.command).unwrap();
        let BridgeVmRequest::GuestToolsSendCommand { name, envelope } = request else {
            panic!("expected guest tools send command request");
        };

        assert_eq!(name, "dev");
        assert_eq!(envelope.request_id.as_deref(), Some("launch-1"));
        assert_eq!(
            envelope.message,
            AgentMessage::LaunchApplication {
                id: "terminal".to_string(),
            }
        );
    }

    #[test]
    fn guest_tools_window_cli_builds_host_command_envelopes() {
        let list = Cli::try_parse_from([
            "bridgevm",
            "guest-tools",
            "list-windows",
            "dev",
            "--request-id",
            "windows-1",
        ])
        .unwrap();
        let request = request_for(list.command).unwrap();
        let BridgeVmRequest::GuestToolsSendCommand { name, envelope } = request else {
            panic!("expected guest tools send command request");
        };

        assert_eq!(name, "dev");
        assert_eq!(envelope.request_id.as_deref(), Some("windows-1"));
        assert_eq!(envelope.message, AgentMessage::ListWindows);

        let focus = Cli::try_parse_from([
            "bridgevm",
            "guest-tools",
            "focus-window",
            "dev",
            "--id",
            "window-terminal",
            "--request-id",
            "focus-1",
        ])
        .unwrap();
        let request = request_for(focus.command).unwrap();
        let BridgeVmRequest::GuestToolsSendCommand { name, envelope } = request else {
            panic!("expected guest tools send command request");
        };

        assert_eq!(name, "dev");
        assert_eq!(envelope.request_id.as_deref(), Some("focus-1"));
        assert_eq!(
            envelope.message,
            AgentMessage::FocusWindow {
                id: "window-terminal".to_string(),
            }
        );

        let close = Cli::try_parse_from([
            "bridgevm",
            "guest-tools",
            "close-window",
            "dev",
            "--id",
            "window-terminal",
            "--request-id",
            "close-1",
        ])
        .unwrap();
        let request = request_for(close.command).unwrap();
        let BridgeVmRequest::GuestToolsSendCommand { name, envelope } = request else {
            panic!("expected guest tools send command request");
        };

        assert_eq!(name, "dev");
        assert_eq!(envelope.request_id.as_deref(), Some("close-1"));
        assert_eq!(
            envelope.message,
            AgentMessage::CloseWindow {
                id: "window-terminal".to_string(),
            }
        );
    }

    #[test]
    fn guest_tools_time_sync_cli_builds_host_command_envelope() {
        let cli = Cli::try_parse_from([
            "bridgevm",
            "guest-tools",
            "time-sync",
            "dev",
            "--unix-epoch-millis",
            "1",
            "--request-id",
            "time-sync-1",
        ])
        .unwrap();

        let request = request_for(cli.command).unwrap();
        let BridgeVmRequest::GuestToolsSendCommand { name, envelope } = request else {
            panic!("expected guest tools send command request");
        };

        assert_eq!(name, "dev");
        assert_eq!(envelope.request_id.as_deref(), Some("time-sync-1"));
        assert_eq!(
            envelope.message,
            AgentMessage::TimeSync {
                unix_epoch_millis: 1,
            }
        );
    }

    #[test]
    fn port_add_and_remove_cli_build_typed_requests() {
        let add = Cli::try_parse_from(["bridgevm", "port", "add", "legacy", "3000:3000"]).unwrap();
        let request = request_for(add.command).unwrap();
        assert_eq!(
            request,
            BridgeVmRequest::AddPort {
                name: "legacy".to_string(),
                host: 3000,
                guest: 3000,
            }
        );

        let remove =
            Cli::try_parse_from(["bridgevm", "port", "remove", "legacy", "3000:3000"]).unwrap();
        let request = request_for(remove.command).unwrap();
        assert_eq!(
            request,
            BridgeVmRequest::RemovePort {
                name: "legacy".to_string(),
                host: 3000,
                guest: 3000,
            }
        );
    }

    #[test]
    fn network_plan_cli_builds_typed_request() {
        let cli = Cli::try_parse_from(["bridgevm", "network-plan", "legacy"]).unwrap();
        let request = request_for(cli.command).unwrap();
        assert_eq!(
            request,
            BridgeVmRequest::PlanNetwork {
                name: "legacy".to_string(),
            }
        );
    }

    #[test]
    fn port_mapping_parser_rejects_invalid_shapes() {
        assert!(parse_port_mapping("3000").is_err());
        assert!(parse_port_mapping("0:3000").is_err());
        assert!(parse_port_mapping("3000:0").is_err());
        assert!(parse_port_mapping("abc:3000").is_err());
    }

    #[test]
    fn local_export_error_keeps_storage_reason() {
        let store = unique_store("bridgevm-cli-export-hardening-test");
        let bundle = store.create_vm(&test_manifest("dev")).unwrap();

        let error = export_vm(
            &store,
            ExportArgs {
                name: "dev".to_string(),
                output: bundle.join("nested").join("dev.vmbridge"),
            },
        )
        .unwrap_err();
        let message = format!("{error:#}");

        assert!(
            message.contains("failed to export VM 'dev'"),
            "missing CLI context: {message}"
        );
        assert!(
            message.contains("export output must not be the source bundle or inside it"),
            "missing storage reason: {message}"
        );
    }

    #[test]
    fn local_import_error_keeps_storage_reason() {
        let store = unique_store("bridgevm-cli-import-hardening-test");
        let bundle = store.create_vm(&test_manifest("dev")).unwrap();

        let error = import_vm(
            &store,
            ImportArgs {
                input: bundle,
                name: None,
            },
        )
        .unwrap_err();
        let message = format!("{error:#}");

        assert!(
            message.contains("failed to import VM bundle"),
            "missing CLI context: {message}"
        );
        assert!(
            message.contains("import input conflicts with the destination store"),
            "missing storage reason: {message}"
        );
    }

    #[test]
    fn local_prepare_run_error_preserves_qemu_network_blocker_requirement() {
        let store = unique_store("bridgevm-cli-qemu-network-blocker-test");
        let mut manifest = compatibility_manifest("legacy");
        manifest.network.mode = "bridged".to_string();
        store.create_vm(&manifest).unwrap();

        let error = build_runner_metadata(&store, "legacy", false).unwrap_err();
        let message = format!("{error:#}");

        assert!(
            message.contains("failed to build Compatibility Mode QEMU command"),
            "missing CLI context: {message}"
        );
        assert!(
            message.contains("QEMU launch blocker qemu-bridged-network-unimplemented"),
            "missing QEMU blocker: {message}"
        );
        assert!(
            message.contains("requirement: Compatibility Mode QEMU requires bridge or tap helper selection before launch"),
            "missing QEMU requirement: {message}"
        );
    }

    #[test]
    fn local_fast_spawn_error_updates_runner_metadata_with_blocker() {
        let store = unique_store("bridgevm-cli-fast-spawn-blocker-test");
        store.create_vm(&test_manifest("fast-linux")).unwrap();

        let error = build_runner_metadata(&store, "fast-linux", true).unwrap_err();
        let message = format!("{error:#}");

        assert!(
            message.contains("Fast Mode spawn is not implemented yet"),
            "{message}"
        );
        assert!(message.contains("launch blockers:"), "{message}");
        assert!(message.contains("missing-primary-disk"), "{message}");
        assert!(
            message.contains("fast-mode-spawn-unimplemented"),
            "{message}"
        );
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
            .any(|blocker| blocker.code == "fast-mode-spawn-unimplemented"));
    }

    #[test]
    fn daemon_error_output_preserves_qemu_network_blocker_requirement() {
        let error = print_daemon_response(BridgeVmResponse::Error {
            message: "failed to build Compatibility Mode QEMU command: QEMU launch blocker qemu-bridged-network-unimplemented: bridged networking is not implemented for Compatibility Mode QEMU args yet; requirement: Compatibility Mode QEMU requires bridge or tap helper selection before launch".to_string(),
        })
        .unwrap_err();
        let message = format!("{error:#}");

        assert!(
            message.contains("QEMU launch blocker qemu-bridged-network-unimplemented"),
            "missing QEMU blocker: {message}"
        );
        assert!(
            message.contains("requirement: Compatibility Mode QEMU requires bridge or tap helper selection before launch"),
            "missing QEMU requirement: {message}"
        );
    }

    #[test]
    fn ssh_cli_builds_typed_request() {
        let cli = Cli::try_parse_from(["bridgevm", "ssh", "dev", "--user", "ubuntu"]).unwrap();
        let request = request_for(cli.command).unwrap();
        assert_eq!(
            request,
            BridgeVmRequest::SshPlan {
                name: "dev".to_string(),
                user: Some("ubuntu".to_string()),
            }
        );
    }

    #[test]
    fn restart_cli_builds_typed_request() {
        let cli = Cli::try_parse_from(["bridgevm", "restart", "dev"]).unwrap();
        let request = request_for(cli.command).unwrap();
        assert_eq!(
            request,
            BridgeVmRequest::RestartVm {
                name: "dev".to_string(),
            }
        );
    }

    #[test]
    fn open_cli_builds_typed_request() {
        let cli =
            Cli::try_parse_from(["bridgevm", "open", "dev", "80", "--scheme", "https"]).unwrap();
        let request = request_for(cli.command).unwrap();
        assert_eq!(
            request,
            BridgeVmRequest::OpenPort {
                name: "dev".to_string(),
                guest: 80,
                scheme: Some("https".to_string()),
            }
        );
    }

    #[test]
    fn share_cli_builds_typed_requests() {
        let list = Cli::try_parse_from(["bridgevm", "share", "list", "dev"]).unwrap();
        assert_eq!(
            request_for(list.command).unwrap(),
            BridgeVmRequest::ListShares {
                name: "dev".to_string(),
            }
        );

        let add = Cli::try_parse_from([
            "bridgevm",
            "share",
            "add",
            "dev",
            "workspace",
            "/Users/me/project",
            "--read-only",
            "--host-path-token",
            "share-token-workspace",
        ])
        .unwrap();
        assert_eq!(
            request_for(add.command).unwrap(),
            BridgeVmRequest::AddShare {
                name: "dev".to_string(),
                share: "workspace".to_string(),
                host_path: "/Users/me/project".to_string(),
                read_only: true,
                host_path_token: Some("share-token-workspace".to_string()),
            }
        );

        let remove =
            Cli::try_parse_from(["bridgevm", "share", "remove", "dev", "workspace"]).unwrap();
        assert_eq!(
            request_for(remove.command).unwrap(),
            BridgeVmRequest::RemoveShare {
                name: "dev".to_string(),
                share: "workspace".to_string(),
            }
        );
    }

    #[test]
    fn diagnostics_bundle_cli_builds_typed_request() {
        let cli = Cli::try_parse_from([
            "bridgevm",
            "diagnostics",
            "bundle",
            "legacy",
            "--output",
            "target/diagnostics",
        ])
        .unwrap();

        let request = request_for(cli.command).unwrap();
        assert_eq!(
            request,
            BridgeVmRequest::CreateDiagnosticBundle {
                name: "legacy".to_string(),
                output: PathBuf::from("target/diagnostics"),
            }
        );
    }

    #[test]
    fn logs_cli_builds_typed_request() {
        let cli =
            Cli::try_parse_from(["bridgevm", "logs", "qemu", "legacy", "--bytes", "4096"]).unwrap();

        let request = request_for(cli.command).unwrap();
        assert_eq!(
            request,
            BridgeVmRequest::ViewLogs {
                name: "legacy".to_string(),
                kind: VmLogKind::Qemu,
                max_bytes: Some(4096),
            }
        );
    }

    #[test]
    fn serial_logs_cli_builds_typed_request() {
        let cli = Cli::try_parse_from(["bridgevm", "logs", "serial", "legacy"]).unwrap();

        let request = request_for(cli.command).unwrap();
        assert_eq!(
            request,
            BridgeVmRequest::ViewLogs {
                name: "legacy".to_string(),
                kind: VmLogKind::Serial,
                max_bytes: None,
            }
        );
    }

    #[test]
    fn performance_baseline_cli_builds_typed_request() {
        let cli = Cli::try_parse_from([
            "bridgevm",
            "performance",
            "baseline",
            "dev",
            "--output",
            "target/performance",
        ])
        .unwrap();

        let request = request_for(cli.command).unwrap();
        assert_eq!(
            request,
            BridgeVmRequest::CreatePerformanceBaseline {
                name: "dev".to_string(),
                output: PathBuf::from("target/performance"),
            }
        );
    }

    #[test]
    fn performance_sample_cli_builds_typed_request() {
        let cli = Cli::try_parse_from([
            "bridgevm",
            "performance",
            "sample",
            "dev",
            "--output",
            "target/performance",
            "--artifact-bytes",
            "4096",
            "--iterations",
            "3",
            "--sync",
        ])
        .unwrap();

        let request = request_for(cli.command).unwrap();
        assert_eq!(
            request,
            BridgeVmRequest::CreatePerformanceSample {
                name: "dev".to_string(),
                output: PathBuf::from("target/performance"),
                artifact_bytes: Some(4096),
                iterations: Some(3),
                sync: true,
            }
        );
    }

    #[test]
    fn performance_sample_cli_uses_default_options() {
        let cli = Cli::try_parse_from([
            "bridgevm",
            "performance",
            "sample",
            "dev",
            "--output",
            "target/performance",
        ])
        .unwrap();

        let request = request_for(cli.command).unwrap();
        assert_eq!(
            request,
            BridgeVmRequest::CreatePerformanceSample {
                name: "dev".to_string(),
                output: PathBuf::from("target/performance"),
                artifact_bytes: None,
                iterations: None,
                sync: false,
            }
        );
    }

    #[test]
    fn performance_sample_cli_accepts_bounds_friendly_args() {
        let cli = Cli::try_parse_from([
            "bridgevm",
            "performance",
            "sample",
            "dev",
            "--output",
            "target/performance",
            "--artifact-bytes",
            "18446744073709551615",
            "--iterations",
            "65535",
        ])
        .unwrap();

        let request = request_for(cli.command).unwrap();
        assert_eq!(
            request,
            BridgeVmRequest::CreatePerformanceSample {
                name: "dev".to_string(),
                output: PathBuf::from("target/performance"),
                artifact_bytes: Some(u64::MAX),
                iterations: Some(u16::MAX),
                sync: false,
            }
        );
    }

    #[test]
    fn metadata_repair_cli_builds_typed_request() {
        let cli = Cli::try_parse_from(["bridgevm", "metadata", "repair", "dev"]).unwrap();
        let request = request_for(cli.command).unwrap();
        assert_eq!(
            request,
            BridgeVmRequest::RepairMetadata {
                name: "dev".to_string(),
            }
        );
    }

    #[test]
    fn metadata_migrate_manifest_cli_builds_typed_request() {
        let cli = Cli::try_parse_from([
            "bridgevm",
            "metadata",
            "migrate-manifest",
            "dev",
            "--dry-run",
        ])
        .unwrap();
        let request = request_for(cli.command).unwrap();
        assert_eq!(
            request,
            BridgeVmRequest::MigrateManifest {
                name: "dev".to_string(),
                dry_run: true,
            }
        );
    }

    #[test]
    fn metadata_manifest_schema_is_local_only() {
        let cli = Cli::try_parse_from(["bridgevm", "metadata", "manifest-schema"]).unwrap();
        let error = request_for(cli.command).unwrap_err().to_string();
        assert!(error.contains("local-only"), "{error}");
    }

    #[test]
    fn metadata_validate_manifest_is_local_only() {
        let cli =
            Cli::try_parse_from(["bridgevm", "metadata", "validate-manifest", "manifest.yaml"])
                .unwrap();
        let error = request_for(cli.command).unwrap_err().to_string();
        assert!(error.contains("local-only"), "{error}");
    }

    #[test]
    fn local_metadata_repair_calls_store() {
        let store = unique_store("bridgevm-cli-metadata-repair-test");
        store.create_vm(&test_manifest("dev")).unwrap();

        metadata(
            &store,
            MetadataCommand {
                command: MetadataSubcommand::Repair(VmNameArgs {
                    name: "dev".to_string(),
                }),
            },
        )
        .unwrap();
    }

    #[test]
    fn local_metadata_migrate_manifest_calls_store() {
        let store = unique_store("bridgevm-cli-manifest-migration-test");
        store.create_vm(&test_manifest("dev")).unwrap();

        metadata(
            &store,
            MetadataCommand {
                command: MetadataSubcommand::MigrateManifest(ManifestMigrateArgs {
                    name: "dev".to_string(),
                    dry_run: true,
                }),
            },
        )
        .unwrap();
    }

    #[test]
    fn local_metadata_manifest_schema_prints_v1_contract() {
        metadata(
            &unique_store("bridgevm-cli-manifest-schema-test"),
            MetadataCommand {
                command: MetadataSubcommand::ManifestSchema,
            },
        )
        .unwrap();
    }

    #[test]
    fn local_metadata_validate_manifest_reads_without_store_mutation() {
        let store = unique_store("bridgevm-cli-manifest-validate-test");
        let manifest_path = store.root().join("manifest.yaml");
        fs::create_dir_all(store.root()).unwrap();
        test_manifest("dev").write(&manifest_path).unwrap();

        metadata(
            &store,
            MetadataCommand {
                command: MetadataSubcommand::ValidateManifest(ManifestValidateArgs {
                    path: manifest_path,
                }),
            },
        )
        .unwrap();
    }

    #[test]
    fn clone_cli_builds_typed_request() {
        let cli = Cli::try_parse_from(["bridgevm", "clone", "dev", "dev-copy"]).unwrap();
        let request = request_for(cli.command).unwrap();
        assert_eq!(
            request,
            BridgeVmRequest::CloneVm {
                name: "dev".to_string(),
                new_name: "dev-copy".to_string(),
                linked: false,
            }
        );
    }

    #[test]
    fn linked_clone_cli_builds_typed_request() {
        let cli =
            Cli::try_parse_from(["bridgevm", "clone", "dev", "dev-copy", "--linked"]).unwrap();
        let request = request_for(cli.command).unwrap();
        assert_eq!(
            request,
            BridgeVmRequest::CloneVm {
                name: "dev".to_string(),
                new_name: "dev-copy".to_string(),
                linked: true,
            }
        );
    }

    #[test]
    fn disk_verify_cli_builds_typed_request() {
        let cli = Cli::try_parse_from(["bridgevm", "disk", "verify", "dev"]).unwrap();
        let request = request_for(cli.command).unwrap();
        assert_eq!(
            request,
            BridgeVmRequest::VerifyDisk {
                name: "dev".to_string(),
            }
        );
    }
}
