//! Split out of args.rs by responsibility.

use crate::*;

#[derive(Debug, Parser)]
pub(crate) struct DiagnosticsCommand {
    #[command(subcommand)]
    pub(crate) command: DiagnosticsSubcommand,
}

#[derive(Debug, Subcommand)]
pub(crate) enum DiagnosticsSubcommand {
    Bundle(DiagnosticsBundleArgs),
}

#[derive(Debug, Parser)]
pub(crate) struct DiagnosticsBundleArgs {
    pub(crate) vm: String,
    #[arg(long, value_name = "DIR")]
    pub(crate) output: PathBuf,
}

#[derive(Debug, Parser)]
pub(crate) struct LogsCommand {
    #[command(subcommand)]
    pub(crate) command: LogsSubcommand,
}

#[derive(Debug, Subcommand)]
pub(crate) enum LogsSubcommand {
    Qemu(LogViewArgs),
    Serial(LogViewArgs),
}

#[derive(Debug, Parser)]
pub(crate) struct LogViewArgs {
    pub(crate) vm: String,
    #[arg(long, value_name = "BYTES")]
    pub(crate) bytes: Option<u64>,
}

#[derive(Debug, Parser)]
pub(crate) struct PerformanceCommand {
    #[command(subcommand)]
    pub(crate) command: PerformanceSubcommand,
}

#[derive(Debug, Subcommand)]
pub(crate) enum PerformanceSubcommand {
    Baseline(PerformanceBaselineArgs),
    Sample(PerformanceSampleArgs),
}

#[derive(Debug, Parser)]
pub(crate) struct PerformanceBaselineArgs {
    pub(crate) vm: String,
    #[arg(long, value_name = "DIR")]
    pub(crate) output: PathBuf,
}

#[derive(Debug, Parser)]
pub(crate) struct PerformanceSampleArgs {
    pub(crate) vm: String,
    #[arg(long, value_name = "DIR")]
    pub(crate) output: PathBuf,
    #[arg(long, value_name = "BYTES")]
    pub(crate) artifact_bytes: Option<u64>,
    #[arg(long, value_name = "N")]
    pub(crate) iterations: Option<u16>,
    #[arg(long)]
    pub(crate) sync: bool,
}

#[derive(Debug, Parser)]
pub(crate) struct MetadataCommand {
    #[command(subcommand)]
    pub(crate) command: MetadataSubcommand,
}

#[derive(Debug, Subcommand)]
pub(crate) enum MetadataSubcommand {
    Repair(VmNameArgs),
    MigrateManifest(ManifestMigrateArgs),
    ManifestSchema,
    ValidateManifest(ManifestValidateArgs),
}

#[derive(Debug, Parser)]
pub(crate) struct ManifestMigrateArgs {
    pub(crate) name: String,
    #[arg(long)]
    pub(crate) dry_run: bool,
}

#[derive(Debug, Parser)]
pub(crate) struct ManifestValidateArgs {
    #[arg(value_name = "PATH")]
    pub(crate) path: PathBuf,
}

#[derive(Debug, Parser)]
pub(crate) struct SnapshotCommand {
    #[command(subcommand)]
    pub(crate) command: SnapshotSubcommand,
}

#[derive(Debug, Parser)]
pub(crate) struct DiskCommand {
    #[command(subcommand)]
    pub(crate) command: DiskSubcommand,
}

#[derive(Debug, Parser)]
pub(crate) struct PortCommand {
    #[command(subcommand)]
    pub(crate) command: PortSubcommand,
}

#[derive(Debug, Parser)]
pub(crate) struct MediaCommand {
    #[command(subcommand)]
    pub(crate) command: MediaSubcommand,
}

#[derive(Debug, Parser)]
pub(crate) struct GuestToolsCommand {
    #[command(subcommand)]
    pub(crate) command: GuestToolsSubcommand,
}

#[derive(Debug, Subcommand)]
pub(crate) enum DiskSubcommand {
    Prepare(VmNameArgs),
    Create(VmNameArgs),
    Inspect(VmNameArgs),
    Verify(VmNameArgs),
    Compact(VmNameArgs),
}

#[derive(Debug, Subcommand)]
pub(crate) enum PortSubcommand {
    List(VmNameArgs),
    Add(PortForwardArgs),
    Remove(PortForwardArgs),
}

#[derive(Debug, Parser)]
pub(crate) struct PortForwardArgs {
    pub(crate) vm: String,
    #[arg(value_name = "HOST:GUEST")]
    pub(crate) mapping: String,
}

#[derive(Debug, Parser)]
pub(crate) struct ShareCommand {
    #[command(subcommand)]
    pub(crate) command: ShareSubcommand,
}

#[derive(Debug, Subcommand)]
pub(crate) enum ShareSubcommand {
    List(VmNameArgs),
    Add(ShareAddArgs),
    Remove(ShareRemoveArgs),
}

#[derive(Debug, Parser)]
pub(crate) struct ShareAddArgs {
    pub(crate) vm: String,
    pub(crate) name: String,
    #[arg(value_name = "HOST_PATH")]
    pub(crate) host_path: String,
    #[arg(long)]
    pub(crate) read_only: bool,
    #[arg(long, value_name = "TOKEN")]
    pub(crate) host_path_token: Option<String>,
}

#[derive(Debug, Parser)]
pub(crate) struct ShareRemoveArgs {
    pub(crate) vm: String,
    pub(crate) name: String,
}

#[derive(Debug, Parser)]
pub(crate) struct SshArgs {
    pub(crate) vm: String,
    #[arg(long, default_value = "user")]
    pub(crate) user: String,
}

#[derive(Debug, Parser)]
pub(crate) struct OpenArgs {
    pub(crate) vm: String,
    #[arg(value_name = "GUEST_PORT")]
    pub(crate) guest: u16,
    #[arg(long, default_value = "http")]
    pub(crate) scheme: String,
}

#[derive(Debug, Subcommand)]
pub(crate) enum MediaSubcommand {
    Download(MediaDownloadArgs),
    DownloadPlan(MediaDownloadPlanArgs),
    Import(MediaImportArgs),
    Status(VmNameArgs),
    Verify(MediaVerifyArgs),
}

#[derive(Debug, Subcommand)]
pub(crate) enum GuestToolsSubcommand {
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
    SetWindowBounds(GuestToolsWindowBoundsArgs),
    WindowPointer(GuestToolsWindowPointerArgs),
    WindowKey(GuestToolsWindowKeyArgs),
    TimeSync(GuestToolsTimeSyncArgs),
}

#[derive(Debug, Parser)]
pub(crate) struct GuestToolsLinuxCommandArgs {
    pub(crate) vm: String,
    #[arg(long, value_enum, default_value_t = GuestToolsLinuxCommandTransportChoice::Device)]
    pub(crate) transport: GuestToolsLinuxCommandTransportChoice,

    #[arg(long, value_name = "PATH")]
    pub(crate) token_file: Option<PathBuf>,
    #[arg(long, value_name = "PATH")]
    pub(crate) device: Option<PathBuf>,
}

#[derive(Debug, Parser)]
pub(crate) struct GuestToolsAcceptHelloArgs {
    pub(crate) vm: String,
    #[arg(long, value_name = "JSON")]
    pub(crate) hello_json: String,
}

#[derive(Debug, Parser)]
pub(crate) struct GuestToolsSendCommandArgs {
    pub(crate) vm: String,
    #[arg(long, value_name = "JSON")]
    pub(crate) envelope_json: String,
}

#[derive(Debug, Parser)]
pub(crate) struct GuestToolsFreezeFilesystemArgs {
    pub(crate) vm: String,
    #[arg(long, value_name = "ID")]
    pub(crate) request_id: Option<String>,
    #[arg(long)]
    pub(crate) timeout_millis: Option<u64>,
}

#[derive(Debug, Parser)]
pub(crate) struct GuestToolsSetClipboardArgs {
    pub(crate) vm: String,
    #[arg(long)]
    pub(crate) text: String,
    #[arg(long, value_name = "ID")]
    pub(crate) request_id: Option<String>,
}

#[derive(Debug, Parser)]
pub(crate) struct GuestToolsResizeDisplayArgs {
    pub(crate) vm: String,
    #[arg(long)]
    pub(crate) width: u32,
    #[arg(long)]
    pub(crate) height: u32,
    #[arg(long)]
    pub(crate) scale: u16,
    #[arg(long, value_name = "ID")]
    pub(crate) request_id: Option<String>,
}

#[derive(Debug, Parser)]
pub(crate) struct GuestToolsMountShareArgs {
    pub(crate) vm: String,
    #[arg(long)]
    pub(crate) name: String,
    #[arg(long, value_name = "TOKEN")]
    pub(crate) host_path_token: String,
    #[arg(long, value_name = "ID")]
    pub(crate) request_id: Option<String>,
}

#[derive(Debug, Parser)]
pub(crate) struct GuestToolsMountApprovedShareArgs {
    pub(crate) vm: String,
    #[arg(long)]
    pub(crate) share: String,
    #[arg(long, value_name = "ID")]
    pub(crate) request_id: Option<String>,
}

#[derive(Debug, Parser)]
pub(crate) struct GuestToolsUnmountShareArgs {
    pub(crate) vm: String,
    #[arg(long)]
    pub(crate) name: String,
    #[arg(long, value_name = "ID")]
    pub(crate) request_id: Option<String>,
}

#[derive(Debug, Parser)]
pub(crate) struct GuestToolsFileDropStartArgs {
    pub(crate) vm: String,
    #[arg(long, value_name = "ID")]
    pub(crate) transfer_id: String,
    #[arg(long)]
    pub(crate) file_name: String,
    #[arg(long)]
    pub(crate) size_bytes: u64,
    #[arg(long, value_name = "ID")]
    pub(crate) request_id: Option<String>,
}

#[derive(Debug, Parser)]
pub(crate) struct GuestToolsFileDropChunkArgs {
    pub(crate) vm: String,
    #[arg(long, value_name = "ID")]
    pub(crate) transfer_id: String,
    #[arg(long)]
    pub(crate) chunk_index: u32,
    #[arg(long)]
    pub(crate) data_base64: String,
    #[arg(long, value_name = "ID")]
    pub(crate) request_id: Option<String>,
}

#[derive(Debug, Parser)]
pub(crate) struct GuestToolsFileDropCompleteArgs {
    pub(crate) vm: String,
    #[arg(long, value_name = "ID")]
    pub(crate) transfer_id: String,
    #[arg(long, value_name = "ID")]
    pub(crate) request_id: Option<String>,
}

#[derive(Debug, Parser)]
pub(crate) struct GuestToolsRequestIdArgs {
    pub(crate) vm: String,
    #[arg(long, value_name = "ID")]
    pub(crate) request_id: Option<String>,
}

#[derive(Debug, Parser)]
pub(crate) struct GuestToolsIdCommandArgs {
    pub(crate) vm: String,
    #[arg(long)]
    pub(crate) id: String,
    #[arg(long, value_name = "ID")]
    pub(crate) request_id: Option<String>,
}

#[derive(Debug, Parser)]
pub(crate) struct GuestToolsWindowBoundsArgs {
    pub(crate) vm: String,
    #[arg(long)]
    pub(crate) id: String,
    #[arg(long)]
    pub(crate) x: i64,
    #[arg(long)]
    pub(crate) y: i64,
    #[arg(long)]
    pub(crate) width: u64,
    #[arg(long)]
    pub(crate) height: u64,
    #[arg(long, value_name = "ID")]
    pub(crate) request_id: Option<String>,
}

#[derive(Debug, Parser)]
pub(crate) struct GuestToolsWindowPointerArgs {
    pub(crate) vm: String,
    #[arg(long)]
    pub(crate) id: String,
    #[arg(long)]
    pub(crate) x: i64,
    #[arg(long)]
    pub(crate) y: i64,
    #[arg(long, value_enum)]
    pub(crate) action: GuestToolsWindowPointerAction,
    #[arg(long, value_enum)]
    pub(crate) button: Option<GuestToolsWindowPointerButton>,
    #[arg(long, value_name = "ID")]
    pub(crate) request_id: Option<String>,
}

#[derive(Debug, Parser)]
pub(crate) struct GuestToolsWindowKeyArgs {
    pub(crate) vm: String,
    #[arg(long)]
    pub(crate) id: String,
    #[arg(long)]
    pub(crate) key: String,
    #[arg(long, value_enum)]
    pub(crate) action: GuestToolsWindowKeyAction,
    #[arg(long, value_name = "ID")]
    pub(crate) request_id: Option<String>,
}

#[derive(Debug, Parser)]
pub(crate) struct GuestToolsTimeSyncArgs {
    pub(crate) vm: String,
    #[arg(long, value_name = "MILLIS")]
    pub(crate) unix_epoch_millis: Option<u64>,
    #[arg(long, value_name = "ID")]
    pub(crate) request_id: Option<String>,
}

#[derive(Debug, Parser)]
pub(crate) struct MediaDownloadArgs {
    pub(crate) vm: String,
    #[arg(long, value_enum)]
    pub(crate) kind: Option<BootMediaKindChoice>,
}

#[derive(Debug, Parser)]
pub(crate) struct MediaDownloadPlanArgs {
    pub(crate) vm: String,
    #[arg(long, value_name = "URL")]
    pub(crate) url: String,
    #[arg(long, value_name = "SHA256")]
    pub(crate) sha256: Option<String>,
    #[arg(long, value_enum)]
    pub(crate) kind: Option<BootMediaKindChoice>,
}

#[derive(Debug, Parser)]
pub(crate) struct MediaImportArgs {
    pub(crate) vm: String,
    #[arg(long, value_name = "PATH")]
    pub(crate) source: PathBuf,
    #[arg(long, value_enum)]
    pub(crate) kind: Option<BootMediaKindChoice>,
}

#[derive(Debug, Parser)]
pub(crate) struct MediaVerifyArgs {
    pub(crate) vm: String,
    #[arg(long, value_name = "SHA256")]
    pub(crate) sha256: String,
    #[arg(long, value_enum)]
    pub(crate) kind: Option<BootMediaKindChoice>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum SnapshotSubcommand {
    Create(SnapshotCreateArgs),
    ExecuteApplicationConsistent(SnapshotApplicationConsistentExecuteArgs),
    DiskCreate(SnapshotDiskCreateArgs),
    Chain(VmNameArgs),
    List(VmNameArgs),
    Restore(SnapshotRestoreArgs),
}

#[derive(Debug, Parser)]
pub(crate) struct SnapshotCreateArgs {
    pub(crate) vm: String,
    pub(crate) name: String,
    #[arg(long, value_enum, default_value_t = SnapshotKindChoice::Disk)]
    pub(crate) kind: SnapshotKindChoice,
}

#[derive(Debug, Parser)]
pub(crate) struct SnapshotApplicationConsistentExecuteArgs {
    pub(crate) vm: String,
    pub(crate) name: String,
    #[arg(long)]
    pub(crate) freeze_timeout_millis: Option<u64>,
}

#[derive(Debug, Parser)]
pub(crate) struct SnapshotDiskCreateArgs {
    pub(crate) vm: String,
    pub(crate) name: String,
}

#[derive(Debug, Parser)]
pub(crate) struct SnapshotRestoreArgs {
    pub(crate) vm: String,
    pub(crate) name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum SnapshotKindChoice {
    Disk,
    Suspend,
    ApplicationConsistent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum GuestToolsLinuxCommandTransportChoice {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum GuestToolsWindowPointerAction {
    Move,
    Press,
    Release,
    Click,
}

impl GuestToolsWindowPointerAction {
    pub(crate) fn as_protocol(self) -> &'static str {
        match self {
            Self::Move => "move",
            Self::Press => "press",
            Self::Release => "release",
            Self::Click => "click",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum GuestToolsWindowPointerButton {
    Left,
    Middle,
    Right,
}

impl GuestToolsWindowPointerButton {
    pub(crate) fn as_protocol(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Middle => "middle",
            Self::Right => "right",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum GuestToolsWindowKeyAction {
    Press,
    Release,
    Tap,
}

impl GuestToolsWindowKeyAction {
    pub(crate) fn as_protocol(self) -> &'static str {
        match self {
            Self::Press => "press",
            Self::Release => "release",
            Self::Tap => "tap",
        }
    }
}

#[derive(Debug, Parser)]
pub(crate) struct GuestArgs {
    #[arg(long)]
    pub(crate) os: String,
    #[arg(long)]
    pub(crate) version: Option<String>,
    #[arg(long, default_value = "arm64")]
    pub(crate) arch: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum ModeChoice {
    Auto,
    Fast,
    Compatibility,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum BootModeChoice {
    ExistingDisk,
    LinuxKernel,
    LinuxInstaller,
    WindowsInstaller,
    MacosRestore,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum DiskFormatChoice {
    Qcow2,
    Raw,
}

pub(crate) const DEFAULT_PRIMARY_DISK_SIZE: &str = "80GiB";

impl DiskFormatChoice {
    pub(crate) fn manifest_format(self) -> &'static str {
        match self {
            Self::Qcow2 => "qcow2",
            Self::Raw => "raw",
        }
    }

    pub(crate) fn default_primary_path(self) -> &'static str {
        match self {
            Self::Qcow2 => "disks/root.qcow2",
            Self::Raw => "disks/root.raw",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum BootMediaKindChoice {
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

#[cfg(test)]
#[path = "../args_tests/mod.rs"]
mod tests;
