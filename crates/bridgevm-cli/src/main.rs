use anyhow::{bail, Context, Result};
use bridgevm_agent_protocol::{AgentEnvelope, AgentMessage, WindowInputEvent};
use bridgevm_api::{
    accept_guest_tools_hello, add_fast_spawn_runner_required_blocker, add_port, add_share,
    apple_vz_runner_configured, cold_start_fast_backend, compatibility_launch_dependency_blockers,
    compatibility_launch_readiness_metadata, create_diagnostic_bundle, create_performance_baseline,
    create_performance_sample, display_fast_backend_with_size, download_boot_media,
    fast_spawn_runner_required_error, guest_tools_linux_command, guest_tools_token,
    import_boot_media, inspect_boot_media_status, inspect_guest_tools_status,
    launch_readiness_metadata, list_ports, list_shares, open_port_plan, plan_boot_media_download,
    reapply_runtime_resources, remove_port, remove_share, resume_backend, runtime_control_command,
    stop_backend, suspend_backend, verify_boot_media, view_vm_log,
    ApplicationConsistentSnapshotExecutionRecord, BootMediaDownloadPlanMetadata,
    BootMediaDownloadResultMetadata, BootMediaImportMetadata, BootMediaKind, BootMediaStatus,
    BootMediaVerificationMetadata, BridgeVmRequest, BridgeVmResponse, DiagnosticBundleMetadata,
    GuestToolsLinuxCommandRecord, GuestToolsLinuxCommandTransport, GuestToolsSessionRecord,
    GuestToolsStatusRecord, GuestToolsTokenRecord, LifecycleAction, LifecyclePlanRecord,
    NetworkPlanRecord, OpenPortPlanRecord, PerformanceBaselineMetadata, PerformanceSampleMetadata,
    PortForwardListRecord, RuntimeControlCommandRecord, SharedFolderListRecord,
    SnapshotPreflightStatusRecord, SshPlanRecord, VmLogKind, VmLogViewRecord, VmReadinessReport,
    VmRecord,
};
use bridgevm_apple_vz::{
    build_fast_plan, write_launch_spec_artifact, AppleVzBootSpec, AppleVzPathSpec,
};
use bridgevm_config::{manifest_json_schema_v1, Boot, BootMode, Guest, VmManifest, VmMode};
use bridgevm_core::{
    available_boot_templates, available_engine_descriptors, boot_template_by_id,
    current_engine_descriptor_for_mode, recommend_mode, target_engine_descriptor_for_guest,
    BootTemplate, GuestChoice, ModeRecommendation, VmEngineDescriptor,
};
use bridgevm_hvf::{
    plan_windows_11_arm_hvf_machine, plan_windows_11_arm_no_qemu, probe_hvf_guest_entry,
    probe_hvf_guest_exit_loop, probe_hvf_interrupt_timer, probe_hvf_memory_map,
    probe_hvf_mmio_block_device, probe_hvf_mmio_block_queue, probe_hvf_mmio_read_emulation,
    probe_hvf_mmio_read_exit, probe_hvf_mmio_rtc_device, probe_hvf_mmio_serial_device,
    probe_hvf_mmio_write_emulation, probe_hvf_vcpu_create, probe_hvf_vcpu_run, probe_hvf_vm_create,
    probe_hvf_vtimer_exit, probe_virtio_block_file_backing, probe_virtio_block_iso_backing,
    probe_virtio_block_request_model, probe_virtio_block_writable_file_backing,
    probe_virtio_gpu_3d_host_preflight_for, probe_windows_11_arm_boot_disk_layout,
    probe_windows_11_arm_platform_description, probe_windows_11_arm_uefi_firmware_device_discovery,
    probe_windows_11_arm_uefi_firmware_handoff, probe_windows_11_arm_uefi_firmware_run_loop,
    probe_windows_11_arm_uefi_pflash_hvf_map, probe_windows_11_arm_uefi_pflash_map,
    probe_windows_11_arm_uefi_reset_vector_entry, probe_windows_11_arm_xhci_hid_boot_key_report,
    query_hvf_host_capabilities, HvfMachinePlanOptions, VirtioGpu3dHostPreflightProtocol,
    WindowsArmBootDiskLayoutOptions, WindowsArmPlatformDescriptionOptions,
    WindowsArmUefiFirmwareHandoffOptions, WindowsArmUefiFirmwareRunLoopExecutionOptions,
    WindowsArmUefiFirmwareRunLoopOptions, WindowsArmUefiPflashMapOptions,
    WINDOWS_ARM_BOOT_DISK_DEFAULT_SIZE_GIB,
};
use bridgevm_qemu::{
    build_compatibility_command, cont as qmp_cont, is_qmp_status_unavailable, qmp_socket_path,
    query_status, stop as qmp_stop, QemuError,
};
use bridgevm_storage::{
    ApplicationConsistentSnapshotPreflightMetadata, LaunchReadinessMetadata, QmpSupervisorMetadata,
    RuntimeResourcePolicyMetadata, RuntimeResourceVisibility, SnapshotKind,
    VmManifestMigrationMetadata, VmMetadataRepairMetadata, VmRuntimeState, VmStore,
};
use clap::{Parser, Subcommand, ValueEnum};
use std::{
    env,
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Read, Write},
    os::unix::fs::PermissionsExt,
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    process::{Command as ProcessCommand, Stdio},
    time::Duration,
};

const MAX_DAEMON_RESPONSE_BYTES: u64 = 16 * 1024 * 1024;
const DAEMON_IO_TIMEOUT: Duration = Duration::from_secs(30);

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
    Display(DisplayArgs),
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
    #[command(subcommand)]
    Resources(ResourcesCommand),
    #[command(subcommand)]
    RuntimeControl(RuntimeControlCommand),
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
    Hvf(HvfCommand),
    #[command(subcommand)]
    Store(StoreCommand),
    Doctor,
}

#[derive(Debug, Subcommand)]
enum HvfCommand {
    /// Print the Windows 11 Arm non-QEMU BridgeVM HVF VMM plan.
    WindowsPlan(WindowsHvfPlanArgs),
    /// Print the concrete QEMU-free Windows 11 Arm HVF machine boundary.
    MachinePlan(WindowsHvfMachinePlanArgs),
    /// Create or verify a QEMU-free sparse raw GPT boot-disk layout for Windows 11 Arm.
    WindowsBootDiskLayoutProbe(WindowsHvfBootDiskLayoutProbeArgs),
    /// Validate a QEMU-free AArch64 UEFI firmware/vars handoff plan for Windows 11 Arm.
    WindowsFirmwareHandoffProbe(WindowsHvfFirmwareHandoffProbeArgs),
    /// Load verified AArch64 UEFI code/vars into planned pflash memory images.
    WindowsPflashMapProbe(WindowsHvfPflashMapProbeArgs),
    /// Optionally map/unmap verified AArch64 UEFI code/vars pflash slots in HVF.
    WindowsPflashHvfMapProbe(WindowsHvfPflashHvfMapProbeArgs),
    /// Optionally enter the AArch64 UEFI reset vector once under HVF.
    WindowsResetVectorEntryProbe(WindowsHvfResetVectorEntryProbeArgs),
    /// Optionally run a bounded AArch64 UEFI firmware exit-classification loop under HVF.
    WindowsFirmwareRunLoopProbe(WindowsHvfFirmwareRunLoopProbeArgs),
    /// Optionally run the Windows UEFI firmware loop as a named device-discovery gate.
    WindowsFirmwareDeviceDiscoveryProbe(WindowsHvfFirmwareRunLoopProbeArgs),
    /// Build the metadata-only Windows 11 Arm FDT platform description.
    WindowsPlatformDescriptionProbe(WindowsHvfPlatformDescriptionProbeArgs),
    /// Probe metadata-only xHCI HID boot-key report generation for Windows 11 Arm.
    WindowsXhciHidBootKeyProbe,
    /// Query Apple Hypervisor.framework host capability metadata without
    /// creating or launching a VM.
    HostCapabilities,
    /// Optionally create and immediately destroy an empty HVF VM.
    VmProbe(HvfVmProbeArgs),
    /// Optionally create and immediately destroy an empty HVF VM plus one vCPU.
    VcpuProbe(HvfVmProbeArgs),
    /// Optionally pre-cancel and observe one bounded hv_vcpu_run return.
    VcpuRunProbe(HvfVcpuRunProbeArgs),
    /// Optionally verify pending IRQ and virtual timer controls on one empty vCPU.
    InterruptTimerProbe(HvfInterruptTimerProbeArgs),
    /// Optionally run a WFI loop and observe a host-programmed VTimer exit.
    VtimerExitProbe(HvfVtimerExitProbeArgs),
    /// Optionally create an empty HVF VM and map/unmap one guest RAM page.
    MemoryMapProbe(HvfMemoryMapProbeArgs),
    /// Optionally run one mapped HVC instruction under a watchdog.
    GuestEntryProbe(HvfGuestEntryProbeArgs),
    /// Optionally run two HVC exits with an explicit PC advance.
    GuestExitLoopProbe(HvfGuestExitLoopProbeArgs),
    /// Optionally run one unmapped read and observe an MMIO/data-abort exit.
    MmioReadProbe(HvfMmioReadProbeArgs),
    /// Optionally emulate one MMIO read and continue guest execution.
    MmioReadEmulationProbe(HvfMmioReadEmulationProbeArgs),
    /// Optionally emulate one MMIO write and continue guest execution.
    MmioWriteEmulationProbe(HvfMmioWriteEmulationProbeArgs),
    /// Optionally emulate a tiny serial MMIO data/status device loop.
    MmioSerialDeviceProbe(HvfMmioSerialDeviceProbeArgs),
    /// Optionally emulate PL011 plus PL031 RTC through the MMIO device bus.
    MmioRtcDeviceProbe(HvfMmioRtcDeviceProbeArgs),
    /// Optionally emulate VirtIO-MMIO block identity registers through the MMIO device bus.
    MmioBlockDeviceProbe(HvfMmioBlockDeviceProbeArgs),
    /// Optionally emulate VirtIO-MMIO block queue/config/address/notify registers through the MMIO device bus.
    MmioBlockQueueProbe(HvfMmioBlockQueueProbeArgs),
    /// Exercise the in-memory VirtIO block read request descriptor model.
    VirtioBlockRequestModelProbe,
    /// Exercise a host file-backed VirtIO block read request descriptor model.
    VirtioBlockFileBackingProbe(HvfVirtioBlockFileBackingProbeArgs),
    /// Exercise a writable host file-backed VirtIO block write/flush persistence descriptor model.
    VirtioBlockWritableFileBackingProbe(HvfVirtioBlockFileBackingProbeArgs),
    /// Exercise a read-only ISO-backed VirtIO block read request descriptor model.
    VirtioBlockIsoBackingProbe(HvfVirtioBlockIsoBackingProbeArgs),
    /// Exercise the synthetic virtio-gpu 3D host-visible blob map/submit/fence path.
    #[command(
        name = "virtio-gpu-3d-host-preflight",
        alias = "virtio-gpu3d-host-preflight"
    )]
    VirtioGpu3dHostPreflight(HvfVirtioGpu3dHostPreflightArgs),
    /// Summarize a BridgeVM HVF virtio-gpu JSONL trace and optionally enforce the P3 gate.
    VirtioGpuTraceReport(HvfVirtioGpuTraceReportArgs),
}

#[derive(Debug, Parser)]
struct WindowsHvfPlanArgs {
    #[arg(long, value_name = "PATH")]
    installer: Option<PathBuf>,
}

#[derive(Debug, Parser)]
struct WindowsHvfMachinePlanArgs {
    #[arg(long, value_name = "PATH")]
    installer: Option<PathBuf>,
    #[arg(long, default_value_t = 6)]
    memory_gib: u32,
    #[arg(long, default_value_t = 4)]
    vcpus: u8,
}

#[derive(Debug, Parser)]
struct WindowsHvfBootDiskLayoutProbeArgs {
    #[arg(long, value_name = "PATH")]
    disk: PathBuf,
    #[arg(long, default_value_t = WINDOWS_ARM_BOOT_DISK_DEFAULT_SIZE_GIB)]
    size_gib: u32,
    #[arg(long)]
    create: bool,
}

#[derive(Debug, Parser)]
struct WindowsHvfFirmwareHandoffProbeArgs {
    #[arg(long, value_name = "PATH")]
    firmware: PathBuf,
    #[arg(long, value_name = "PATH")]
    vars_template: Option<PathBuf>,
    #[arg(long, value_name = "PATH")]
    vars: Option<PathBuf>,
    #[arg(long)]
    create_vars: bool,
}

#[derive(Debug, Parser)]
struct WindowsHvfPflashMapProbeArgs {
    #[arg(long, value_name = "PATH")]
    firmware: PathBuf,
    #[arg(long, value_name = "PATH")]
    vars_template: Option<PathBuf>,
    #[arg(long, value_name = "PATH")]
    vars: Option<PathBuf>,
    #[arg(long)]
    create_vars: bool,
}

#[derive(Debug, Parser)]
struct WindowsHvfPflashHvfMapProbeArgs {
    #[arg(long, value_name = "PATH")]
    firmware: PathBuf,
    #[arg(long, value_name = "PATH")]
    vars_template: Option<PathBuf>,
    #[arg(long, value_name = "PATH")]
    vars: Option<PathBuf>,
    #[arg(long)]
    create_vars: bool,
    #[arg(long)]
    allow_map: bool,
}

#[derive(Debug, Parser)]
struct WindowsHvfResetVectorEntryProbeArgs {
    #[arg(long, value_name = "PATH")]
    firmware: PathBuf,
    #[arg(long, value_name = "PATH")]
    vars_template: Option<PathBuf>,
    #[arg(long, value_name = "PATH")]
    vars: Option<PathBuf>,
    #[arg(long)]
    create_vars: bool,
    #[arg(long)]
    allow_entry: bool,
}

#[derive(Debug, Parser)]
struct WindowsHvfFirmwareRunLoopProbeArgs {
    #[arg(long, value_name = "PATH")]
    firmware: PathBuf,
    #[arg(long, value_name = "PATH")]
    vars_template: Option<PathBuf>,
    #[arg(long, value_name = "PATH")]
    vars: Option<PathBuf>,
    #[arg(long)]
    create_vars: bool,
    #[arg(long)]
    allow_loop: bool,
    #[arg(long, default_value_t = 8)]
    max_exits: u32,
    #[arg(long, default_value_t = 64)]
    guest_ram_mib: u32,
    #[arg(long, default_value_t = 100)]
    watchdog_ms: u64,
    #[arg(long)]
    map_low_pflash_alias: bool,
    #[arg(long)]
    seed_diagnostic_vector: bool,
    #[arg(long)]
    seed_guest_ram_diagnostic_vector: bool,
    #[arg(long)]
    seed_executable_diagnostic_vector: bool,
    #[arg(long)]
    try_recommended_vector_base_vbar: bool,
    #[arg(long)]
    continue_after_recommended_vector_base_vbar: bool,
    #[arg(long)]
    repair_low_vector_diagnostic_page: bool,
    #[arg(long)]
    remap_low_vector_to_recommended_vector: bool,
    #[arg(long)]
    continue_after_low_vector_repair: bool,
    #[arg(long)]
    restore_low_vector_slot_before_eret: bool,
    #[arg(long)]
    wire_interrupt_timer: bool,
    #[arg(long, value_name = "PATH")]
    iso: Option<PathBuf>,
    #[arg(long, value_name = "PATH")]
    writable_disk: Option<PathBuf>,
}

#[derive(Debug, Parser)]
struct WindowsHvfPlatformDescriptionProbeArgs {
    #[arg(long, default_value_t = 6)]
    memory_gib: u32,
    #[arg(long, default_value_t = 4)]
    vcpus: u8,
}

#[derive(Debug, Parser)]
struct HvfVmProbeArgs {
    #[arg(long)]
    allow_create: bool,
}

#[derive(Debug, Parser)]
struct HvfVcpuRunProbeArgs {
    #[arg(long)]
    allow_run: bool,
}

#[derive(Debug, Parser)]
struct HvfInterruptTimerProbeArgs {
    #[arg(long)]
    allow_interrupt_timer: bool,
}

#[derive(Debug, Parser)]
struct HvfVtimerExitProbeArgs {
    #[arg(long)]
    allow_vtimer_exit: bool,
}

#[derive(Debug, Parser)]
struct HvfMemoryMapProbeArgs {
    #[arg(long)]
    allow_map: bool,
}

#[derive(Debug, Parser)]
struct HvfGuestEntryProbeArgs {
    #[arg(long)]
    allow_entry: bool,
}

#[derive(Debug, Parser)]
struct HvfGuestExitLoopProbeArgs {
    #[arg(long)]
    allow_loop: bool,
}

#[derive(Debug, Parser)]
struct HvfMmioReadProbeArgs {
    #[arg(long)]
    allow_mmio: bool,
}

#[derive(Debug, Parser)]
struct HvfMmioReadEmulationProbeArgs {
    #[arg(long)]
    allow_emulate: bool,
}

#[derive(Debug, Parser)]
struct HvfMmioWriteEmulationProbeArgs {
    #[arg(long)]
    allow_emulate: bool,
}

#[derive(Debug, Parser)]
struct HvfMmioSerialDeviceProbeArgs {
    #[arg(long)]
    allow_device: bool,
}

#[derive(Debug, Parser)]
struct HvfMmioRtcDeviceProbeArgs {
    #[arg(long)]
    allow_device: bool,
}

#[derive(Debug, Parser)]
struct HvfMmioBlockDeviceProbeArgs {
    #[arg(long)]
    allow_device: bool,
}

#[derive(Debug, Parser)]
struct HvfMmioBlockQueueProbeArgs {
    #[arg(long)]
    allow_device: bool,
    #[arg(long, value_name = "PATH")]
    disk: Option<PathBuf>,
    #[arg(long, value_name = "PATH")]
    iso: Option<PathBuf>,
    #[arg(long, value_name = "PATH")]
    writable_disk: Option<PathBuf>,
}

#[derive(Debug, Parser)]
struct HvfVirtioBlockFileBackingProbeArgs {
    #[arg(long, value_name = "PATH")]
    disk: PathBuf,
}

#[derive(Debug, Parser)]
struct HvfVirtioBlockIsoBackingProbeArgs {
    #[arg(long, value_name = "PATH")]
    iso: PathBuf,
}

#[derive(Debug, Parser)]
struct HvfVirtioGpu3dHostPreflightArgs {
    #[arg(long, value_enum, default_value_t = VirtioGpu3dHostPreflightProtocolChoice::Venus)]
    protocol: VirtioGpu3dHostPreflightProtocolChoice,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum VirtioGpu3dHostPreflightProtocolChoice {
    Venus,
    Virgl,
}

impl From<VirtioGpu3dHostPreflightProtocolChoice> for VirtioGpu3dHostPreflightProtocol {
    fn from(value: VirtioGpu3dHostPreflightProtocolChoice) -> Self {
        match value {
            VirtioGpu3dHostPreflightProtocolChoice::Venus => Self::Venus,
            VirtioGpu3dHostPreflightProtocolChoice::Virgl => Self::Virgl,
        }
    }
}

#[derive(Debug, Parser)]
struct HvfVirtioGpuTraceReportArgs {
    #[arg(long, value_name = "PATH")]
    trace: PathBuf,
    #[arg(long, value_enum, default_value_t = VirtioGpuTraceProtocolChoice::Auto)]
    protocol: VirtioGpuTraceProtocolChoice,
    #[arg(long)]
    require_p3_gate: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum VirtioGpuTraceProtocolChoice {
    Auto,
    Venus,
    Virgl,
}

impl VirtioGpuTraceProtocolChoice {
    fn label(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Venus => "venus",
            Self::Virgl => "virgl",
        }
    }
}

#[derive(Debug, Subcommand)]
enum StoreCommand {
    Doctor,
}

#[derive(Debug, Subcommand)]
enum ResourcesCommand {
    /// Re-evaluate a running Fast Mode VM's resource policy for the current
    /// power state and foreground/background visibility.
    Reapply(RuntimeResourcesArgs),
}

#[derive(Debug, Subcommand)]
enum RuntimeControlCommand {
    /// Query the live Apple VZ display process over its recorded control socket.
    Status(VmNameArgs),
    /// Ask the live Apple VZ display process to stop gracefully.
    Stop(VmNameArgs),
    /// Fetch the latest runtime resource policy visible to the display process.
    Policy(VmNameArgs),
    /// Summarize the display pacing view derived from the live runtime policy.
    Pacing(VmNameArgs),
    /// Re-evaluate runtime resources and ask any live display helper to read
    /// the refreshed policy.
    Reapply(RuntimeResourcesArgs),
}

#[derive(Debug, Parser)]
struct RuntimeResourcesArgs {
    name: String,
    #[arg(long, value_enum, default_value_t = RuntimeResourceVisibilityChoice::Foreground)]
    visibility: RuntimeResourceVisibilityChoice,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum RuntimeResourceVisibilityChoice {
    Foreground,
    Background,
}

impl From<RuntimeResourceVisibilityChoice> for RuntimeResourceVisibility {
    fn from(value: RuntimeResourceVisibilityChoice) -> Self {
        match value {
            RuntimeResourceVisibilityChoice::Foreground => RuntimeResourceVisibility::Foreground,
            RuntimeResourceVisibilityChoice::Background => RuntimeResourceVisibility::Background,
        }
    }
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
    #[arg(long)]
    disk: Option<String>,
    #[arg(long, value_enum)]
    disk_format: Option<DiskFormatChoice>,
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
struct DisplayArgs {
    name: String,
    #[arg(long, value_name = "PX")]
    width: Option<u32>,
    #[arg(long, value_name = "PX")]
    height: Option<u32>,
}

impl DisplayArgs {
    fn display_size(&self) -> Result<Option<(u32, u32)>> {
        match (self.width, self.height) {
            (Some(width), Some(height)) if width > 0 && height > 0 => Ok(Some((width, height))),
            (Some(_), Some(_)) => bail!("--width and --height must be positive integers"),
            (None, None) => Ok(None),
            _ => bail!("--width and --height must be provided together"),
        }
    }
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
    SetWindowBounds(GuestToolsWindowBoundsArgs),
    WindowPointer(GuestToolsWindowPointerArgs),
    WindowKey(GuestToolsWindowKeyArgs),
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
struct GuestToolsWindowBoundsArgs {
    vm: String,
    #[arg(long)]
    id: String,
    #[arg(long)]
    x: i64,
    #[arg(long)]
    y: i64,
    #[arg(long)]
    width: u64,
    #[arg(long)]
    height: u64,
    #[arg(long, value_name = "ID")]
    request_id: Option<String>,
}

#[derive(Debug, Parser)]
struct GuestToolsWindowPointerArgs {
    vm: String,
    #[arg(long)]
    id: String,
    #[arg(long)]
    x: i64,
    #[arg(long)]
    y: i64,
    #[arg(long, value_enum)]
    action: GuestToolsWindowPointerAction,
    #[arg(long, value_enum)]
    button: Option<GuestToolsWindowPointerButton>,
    #[arg(long, value_name = "ID")]
    request_id: Option<String>,
}

#[derive(Debug, Parser)]
struct GuestToolsWindowKeyArgs {
    vm: String,
    #[arg(long)]
    id: String,
    #[arg(long)]
    key: String,
    #[arg(long, value_enum)]
    action: GuestToolsWindowKeyAction,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum GuestToolsWindowPointerAction {
    Move,
    Press,
    Release,
    Click,
}

impl GuestToolsWindowPointerAction {
    fn as_protocol(self) -> &'static str {
        match self {
            Self::Move => "move",
            Self::Press => "press",
            Self::Release => "release",
            Self::Click => "click",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum GuestToolsWindowPointerButton {
    Left,
    Middle,
    Right,
}

impl GuestToolsWindowPointerButton {
    fn as_protocol(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Middle => "middle",
            Self::Right => "right",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum GuestToolsWindowKeyAction {
    Press,
    Release,
    Tap,
}

impl GuestToolsWindowKeyAction {
    fn as_protocol(self) -> &'static str {
        match self {
            Self::Press => "press",
            Self::Release => "release",
            Self::Tap => "tap",
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
enum DiskFormatChoice {
    Qcow2,
    Raw,
}

const DEFAULT_PRIMARY_DISK_SIZE: &str = "80GiB";

impl DiskFormatChoice {
    fn manifest_format(self) -> &'static str {
        match self {
            Self::Qcow2 => "qcow2",
            Self::Raw => "raw",
        }
    }

    fn default_primary_path(self) -> &'static str {
        match self {
            Self::Qcow2 => "disks/root.qcow2",
            Self::Raw => "disks/root.raw",
        }
    }
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
        Command::Resources(args) => resources(&store, args),
        Command::RuntimeControl(args) => runtime_control(&store, args),
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
        Command::Hvf(args) => hvf(args),
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
        Command::Create(args) => request_for_create(args),
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
            GuestToolsSubcommand::SetWindowBounds(args) => {
                Ok(BridgeVmRequest::GuestToolsSendCommand {
                    name: args.vm,
                    envelope: agent_command_envelope(
                        AgentMessage::SetWindowBounds {
                            id: args.id,
                            x: args.x,
                            y: args.y,
                            width: args.width,
                            height: args.height,
                        },
                        args.request_id,
                    ),
                })
            }
            GuestToolsSubcommand::WindowPointer(args) => {
                Ok(BridgeVmRequest::GuestToolsSendCommand {
                    name: args.vm,
                    envelope: agent_command_envelope(
                        AgentMessage::WindowInput {
                            id: args.id,
                            event: WindowInputEvent::Pointer {
                                x: args.x,
                                y: args.y,
                                action: args.action.as_protocol().to_string(),
                                button: args
                                    .button
                                    .map(|button| button.as_protocol().to_string()),
                            },
                        },
                        args.request_id,
                    ),
                })
            }
            GuestToolsSubcommand::WindowKey(args) => Ok(BridgeVmRequest::GuestToolsSendCommand {
                name: args.vm,
                envelope: agent_command_envelope(
                    AgentMessage::WindowInput {
                        id: args.id,
                        event: WindowInputEvent::Key {
                            key: args.key,
                            action: args.action.as_protocol().to_string(),
                        },
                    },
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
        Command::Resources(args) => match args {
            ResourcesCommand::Reapply(args) => Ok(BridgeVmRequest::ReapplyRuntimeResources {
                name: args.name,
                visibility: args.visibility.into(),
            }),
        },
        Command::RuntimeControl(args) => match args {
            RuntimeControlCommand::Status(args) => Ok(BridgeVmRequest::RuntimeControl {
                name: args.name,
                command: "status".to_string(),
            }),
            RuntimeControlCommand::Stop(args) => Ok(BridgeVmRequest::RuntimeControl {
                name: args.name,
                command: "stop".to_string(),
            }),
            RuntimeControlCommand::Policy(args) => Ok(BridgeVmRequest::RuntimeControl {
                name: args.name,
                command: "policy".to_string(),
            }),
            RuntimeControlCommand::Pacing(args) => Ok(BridgeVmRequest::RuntimeControl {
                name: args.name,
                command: "pacing".to_string(),
            }),
            RuntimeControlCommand::Reapply(args) => Ok(BridgeVmRequest::ReapplyRuntimeResources {
                name: args.name,
                visibility: args.visibility.into(),
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
        Command::Hvf(_) => {
            bail!("hvf commands are local metadata-only commands; omit --socket")
        }
        Command::Store(StoreCommand::Doctor) => Ok(BridgeVmRequest::Doctor),
        Command::Doctor => Ok(BridgeVmRequest::Doctor),
    }
}

fn request_for_create(args: CreateArgs) -> Result<BridgeVmRequest> {
    if create_args_are_plain_template_request(&args) {
        return Ok(BridgeVmRequest::CreateVmFromTemplate {
            name: args.name,
            template_id: args.template.expect("plain template request has template"),
        });
    }

    let manifest = manifest_for_create(args)?;
    Ok(BridgeVmRequest::create_vm(manifest))
}

fn create_args_are_plain_template_request(args: &CreateArgs) -> bool {
    args.template.is_some()
        && args.os.is_none()
        && args.version.is_none()
        && args.arch.is_none()
        && args.mode == ModeChoice::Auto
        && args.disk.is_none()
        && args.disk_format.is_none()
        && args.boot_mode.is_none()
        && args.installer_image.is_none()
        && args.kernel_path.is_none()
        && args.initrd_path.is_none()
        && args.kernel_command_line.is_none()
        && args.macos_restore_image.is_none()
}

fn send_request(socket: &Path, request: BridgeVmRequest) -> Result<BridgeVmResponse> {
    let mut stream = UnixStream::connect(socket)
        .with_context(|| format!("failed to connect to daemon socket {}", socket.display()))?;
    stream
        .set_read_timeout(Some(DAEMON_IO_TIMEOUT))
        .context("failed to configure daemon response timeout")?;
    stream
        .set_write_timeout(Some(DAEMON_IO_TIMEOUT))
        .context("failed to configure daemon request timeout")?;
    serde_json::to_writer(&mut stream, &request).context("failed to write daemon request")?;
    stream.write_all(b"\n")?;

    let mut response_frame = Vec::new();
    BufReader::new(stream)
        .take(MAX_DAEMON_RESPONSE_BYTES + 1)
        .read_until(b'\n', &mut response_frame)
        .context("failed to read daemon response")?;
    if response_frame.is_empty() {
        bail!("daemon returned an empty response")
    }
    if response_frame.len() as u64 > MAX_DAEMON_RESPONSE_BYTES {
        bail!(
            "daemon response exceeded {} bytes",
            MAX_DAEMON_RESPONSE_BYTES
        )
    }
    if response_frame.last() != Some(&b'\n') {
        bail!("daemon returned an incomplete response frame")
    }
    let response = serde_json::from_slice::<BridgeVmResponse>(&response_frame)
        .context("invalid daemon response JSON")?;
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
            print_engine_catalog(available_engine_descriptors());
            print_parallels_class_progress(&parallels_class_progress());
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
        } => print_runner_status(metadata, qmp_supervisor.as_ref(), None),
        BridgeVmResponse::RuntimeControl { control } => print_runtime_control_command(&control)?,
        BridgeVmResponse::ReadinessReport { report } => print_readiness_report(&report),
        BridgeVmResponse::LifecyclePlan { plan } => print_lifecycle_plan(&plan),
        BridgeVmResponse::RuntimeResourcePolicy { policy } => {
            print_runtime_resource_policy(&policy)
        }
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
            print_mode_recommendation(&recommendation, None);
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

    let disk_size = args
        .disk
        .clone()
        .or_else(|| {
            template
                .as_ref()
                .and_then(|template| template.primary_disk_size().map(str::to_string))
        })
        .unwrap_or_else(|| DEFAULT_PRIMARY_DISK_SIZE.to_string());
    let boot = boot_for_create(&args, mode, &rec, template.as_ref());
    let mut manifest = VmManifest::new(args.name, mode, Guest { os, version, arch }, disk_size);
    if let Some(template) = &template {
        template.apply_storage_defaults(&mut manifest.storage.primary);
        if let Some(disk) = &args.disk {
            manifest.storage.primary.size = disk.clone();
        }
    }
    if let Some(disk_format) = args.disk_format {
        manifest.storage.primary.format = disk_format.manifest_format().to_string();
        manifest.storage.primary.path = disk_format.default_primary_path().to_string();
    }
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
        | GuestToolsSubcommand::SetWindowBounds(_)
        | GuestToolsSubcommand::WindowPointer(_)
        | GuestToolsSubcommand::WindowKey(_)
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

fn resources(store: &VmStore, args: ResourcesCommand) -> Result<()> {
    match args {
        ResourcesCommand::Reapply(args) => {
            let policy = reapply_runtime_resources(store, &args.name, args.visibility.into())
                .map_err(anyhow::Error::msg)
                .with_context(|| {
                    format!("failed to reapply runtime resources for '{}'", args.name)
                })?;
            print_runtime_resource_policy(&policy);
        }
    }
    Ok(())
}

fn runtime_control(store: &VmStore, args: RuntimeControlCommand) -> Result<()> {
    match args {
        RuntimeControlCommand::Status(args) => {
            run_runtime_control_command(store, &args.name, "status")
        }
        RuntimeControlCommand::Stop(args) => run_runtime_control_command(store, &args.name, "stop"),
        RuntimeControlCommand::Policy(args) => {
            run_runtime_control_command(store, &args.name, "policy")
        }
        RuntimeControlCommand::Pacing(args) => {
            run_runtime_control_command(store, &args.name, "pacing")
        }
        RuntimeControlCommand::Reapply(args) => {
            let policy = reapply_runtime_resources(store, &args.name, args.visibility.into())
                .map_err(anyhow::Error::msg)
                .with_context(|| {
                    format!(
                        "failed to reapply runtime control policy for '{}'",
                        args.name
                    )
                })?;
            print_runtime_resource_policy(&policy);
            Ok(())
        }
    }
}

fn run_runtime_control_command(store: &VmStore, name: &str, command: &str) -> Result<()> {
    let control = runtime_control_command(store, name, command).map_err(anyhow::Error::msg)?;
    print_runtime_control_command(&control)
}

fn print_runtime_control_command(control: &RuntimeControlCommandRecord) -> Result<()> {
    println!("Runtime control {} for {}", control.command, control.vm);
    println!("Kind: {}", control.kind);
    println!("Socket: {}", control.socket_path.display());
    println!(
        "{}",
        serde_json::to_string_pretty(&control.response)
            .context("failed to format runtime response")?
    );
    Ok(())
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
    print_runner_status(Some(metadata), qmp_supervisor.as_ref(), None);
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
    print_runner_status(Some(metadata), qmp_supervisor.as_ref(), None);
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
    print_runner_status(Some(metadata), qmp_supervisor.as_ref(), None);
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
    print_runner_status(Some(metadata), qmp_supervisor.as_ref(), None);
    Ok(())
}

fn display_backend_local(store: &VmStore, args: DisplayArgs) -> Result<()> {
    if !apple_vz_runner_configured() {
        anyhow::bail!(
            "embedded display requires BRIDGEVM_APPLE_VZ_RUNNER to point at a signed AppleVzRunner"
        );
    }
    let display_size = args.display_size()?;
    let metadata = display_fast_backend_with_size(store, &args.name, display_size)
        .map_err(anyhow::Error::msg)
        .with_context(|| format!("failed to launch embedded display for VM '{}'", args.name))?;
    println!(
        "Launched embedded display window for {} (close the window to stop the VM)",
        args.name
    );
    let runtime_policy = store
        .runtime_resource_policy_metadata(&args.name)
        .context("failed to read runtime resource policy metadata")?;
    print_runner_status(Some(metadata), None, runtime_policy.as_ref());
    Ok(())
}

fn build_runner_metadata(
    store: &VmStore,
    name: &str,
    spawn: bool,
) -> Result<bridgevm_storage::RunnerMetadata> {
    let (bundle, mut manifest, _) = store
        .get_vm_with_active_disk(name)
        .context("failed to read VM")?;

    let (disk, active_disk) = store
        .prepare_active_disk(name)
        .context("failed to prepare active disk")?;
    manifest.storage.primary.path = active_disk.path.display().to_string();
    manifest.storage.primary.format = active_disk.format.clone();
    if manifest.mode == VmMode::Fast {
        // Gated REAL cold-start launch: when `BRIDGEVM_APPLE_VZ_RUNNER` is set
        // and the caller asked to spawn, boot a real Apple VZ VM. When unset,
        // preserve the legacy dry-run + runner-required fallback.
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
            add_fast_spawn_runner_required_blocker(&mut readiness);
        }
        let spawn_error = spawn.then(|| fast_spawn_runner_required_error(&readiness));
        let metadata = bridgevm_storage::RunnerMetadata {
            engine: "lightvm".to_string(),
            pid: None,
            command: plan.render_runner_words_for_launch_spec(&launch_spec_path),
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
            runtime_control: None,
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
    let readiness = compatibility_launch_readiness_metadata(
        &disk,
        compatibility_launch_dependency_blockers(&manifest, &bundle),
    );
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
            runtime_control: None,
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
        runtime_control: None,
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
    let runtime_policy = store
        .runtime_resource_policy_metadata(&args.name)
        .context("failed to read runtime resource policy metadata")?;
    print_runner_status(metadata, qmp_supervisor.as_ref(), runtime_policy.as_ref());
    Ok(())
}

fn print_runner_status(
    metadata: Option<bridgevm_storage::RunnerMetadata>,
    qmp_supervisor: Option<&QmpSupervisorMetadata>,
    runtime_policy: Option<&RuntimeResourcePolicyMetadata>,
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
            if let Some(runtime_control) = &metadata.runtime_control {
                print_runtime_control(runtime_control);
            }
            if let Some(policy) = runtime_policy {
                print_runner_runtime_policy(policy);
            }
            println!("Command: {}", metadata.command.join(" "));
        }
        None => println!("No runner metadata"),
    }
    if let Some(supervisor) = qmp_supervisor {
        print_qmp_supervisor(supervisor);
    }
}

fn print_runtime_control(control: &bridgevm_storage::RuntimeControlMetadata) {
    println!("Runtime control kind: {}", control.kind);
    println!("Runtime control socket: {}", control.socket_path.display());
    println!("Runtime control commands: {}", control.commands.join(", "));
}

fn print_runner_runtime_policy(policy: &RuntimeResourcePolicyMetadata) {
    println!("Runtime policy visibility: {}", policy.visibility);
    println!("Runtime policy display FPS cap: {}", policy.display_fps_cap);
    println!("Runtime policy live applied: {}", policy.live_applied);
    println!(
        "Runtime policy control acknowledged: {}",
        policy.runtime_control_acknowledged
    );
    if !policy.live_apply_blockers.is_empty() {
        let blockers = policy
            .live_apply_blockers
            .iter()
            .map(|blocker| blocker.code.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        println!("Runtime policy blockers: {blockers}");
    }
}

fn print_runtime_resource_policy(policy: &RuntimeResourcePolicyMetadata) {
    println!("Runtime resources for {}", policy.vm);
    println!("Mode: {}", policy.mode);
    println!("Profile: {}", policy.profile);
    println!("Visibility: {}", policy.visibility);
    println!("State: {}", policy.state);
    println!("On battery: {}", policy.on_battery);
    println!("Memory: {}", policy.memory);
    println!("CPU: {}", policy.cpu);
    println!("Display FPS cap: {}", policy.display_fps_cap);
    println!("Rationale: {}", policy.rationale);
    println!("Live applied: {}", policy.live_applied);
    println!(
        "Runtime control acknowledged: {}",
        policy.runtime_control_acknowledged
    );
    if policy.live_apply_blockers.is_empty() {
        println!("Live apply blockers: none");
    } else {
        for blocker in &policy.live_apply_blockers {
            println!("Live apply blocker: {} - {}", blocker.code, blocker.message);
        }
    }
    println!("Metadata recorded: {}", policy.updated_at_unix);
}

fn compatibility_qemu_command_error(error: QemuError) -> String {
    format!("failed to build Compatibility Mode QEMU command: {error}")
}

fn compatibility_launch_readiness_summary(readiness: &LaunchReadinessMetadata) -> String {
    let summary = readiness
        .blockers
        .iter()
        .map(|blocker| format!("{}: {}", blocker.code, blocker.message))
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
            "Live evidence graphical boot progress: proven={}",
            evidence.graphical_boot_progress_proven
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
    let choice = GuestChoice {
        os: args.os,
        version: args.version,
        arch: args.arch,
    };
    let rec = recommend_mode(&choice);
    print_mode_recommendation(&rec, Some(&choice));
    Ok(())
}

fn hvf(command: HvfCommand) -> Result<()> {
    match command {
        HvfCommand::WindowsPlan(args) => {
            let plan = plan_windows_11_arm_no_qemu(args.installer);
            print!("{}", plan.render_text());
            Ok(())
        }
        HvfCommand::MachinePlan(args) => {
            if args.memory_gib == 0 {
                bail!("--memory-gib must be greater than zero");
            }
            if args.vcpus == 0 {
                bail!("--vcpus must be greater than zero");
            }
            let plan = plan_windows_11_arm_hvf_machine(HvfMachinePlanOptions {
                installer: args.installer,
                memory_gib: args.memory_gib,
                vcpu_count: args.vcpus,
            });
            print!("{}", plan.render_text());
            Ok(())
        }
        HvfCommand::WindowsBootDiskLayoutProbe(args) => {
            if args.size_gib == 0 {
                bail!("--size-gib must be greater than zero");
            }
            let probe = probe_windows_11_arm_boot_disk_layout(WindowsArmBootDiskLayoutOptions {
                disk_path: args.disk,
                size_gib: args.size_gib,
                create: args.create,
            });
            print!("{}", probe.render_text());
            Ok(())
        }
        HvfCommand::WindowsFirmwareHandoffProbe(args) => {
            let probe =
                probe_windows_11_arm_uefi_firmware_handoff(WindowsArmUefiFirmwareHandoffOptions {
                    firmware_path: args.firmware,
                    vars_template_path: args.vars_template,
                    vars_path: args.vars,
                    create_vars: args.create_vars,
                });
            print!("{}", probe.render_text());
            Ok(())
        }
        HvfCommand::WindowsPflashMapProbe(args) => {
            let probe = probe_windows_11_arm_uefi_pflash_map(WindowsArmUefiPflashMapOptions {
                firmware_path: args.firmware,
                vars_template_path: args.vars_template,
                vars_path: args.vars,
                create_vars: args.create_vars,
            });
            print!("{}", probe.render_text());
            Ok(())
        }
        HvfCommand::WindowsPflashHvfMapProbe(args) => {
            let allow_map = args.allow_map
                || env::var("BRIDGEVM_HVF_ALLOW_UEFI_PFLASH_MAP").as_deref() == Ok("1");
            let probe = probe_windows_11_arm_uefi_pflash_hvf_map(
                WindowsArmUefiPflashMapOptions {
                    firmware_path: args.firmware,
                    vars_template_path: args.vars_template,
                    vars_path: args.vars,
                    create_vars: args.create_vars,
                },
                allow_map,
            );
            print!("{}", probe.render_text());
            Ok(())
        }
        HvfCommand::WindowsResetVectorEntryProbe(args) => {
            let allow_entry = args.allow_entry
                || env::var("BRIDGEVM_HVF_ALLOW_UEFI_RESET_VECTOR_ENTRY").as_deref() == Ok("1");
            let probe = probe_windows_11_arm_uefi_reset_vector_entry(
                WindowsArmUefiPflashMapOptions {
                    firmware_path: args.firmware,
                    vars_template_path: args.vars_template,
                    vars_path: args.vars,
                    create_vars: args.create_vars,
                },
                allow_entry,
            );
            print!("{}", probe.render_text());
            Ok(())
        }
        HvfCommand::WindowsFirmwareRunLoopProbe(args) => {
            if args.max_exits == 0 {
                bail!("--max-exits must be greater than zero");
            }
            if args.guest_ram_mib == 0 {
                bail!("--guest-ram-mib must be greater than zero");
            }
            if args.watchdog_ms == 0 {
                bail!("--watchdog-ms must be greater than zero");
            }
            let allow_loop = args.allow_loop
                || env::var("BRIDGEVM_HVF_ALLOW_UEFI_FIRMWARE_RUN_LOOP").as_deref() == Ok("1");
            let probe =
                probe_windows_11_arm_uefi_firmware_run_loop(WindowsArmUefiFirmwareRunLoopOptions {
                    pflash: WindowsArmUefiPflashMapOptions {
                        firmware_path: args.firmware,
                        vars_template_path: args.vars_template,
                        vars_path: args.vars,
                        create_vars: args.create_vars,
                    },
                    execution: WindowsArmUefiFirmwareRunLoopExecutionOptions {
                        allow_loop,
                        requested_exits: args.max_exits,
                        guest_ram_mib: args.guest_ram_mib,
                        watchdog_timeout_ms: args.watchdog_ms,
                        map_low_pflash_alias: args.map_low_pflash_alias,
                        seed_diagnostic_vector: args.seed_diagnostic_vector,
                        seed_guest_ram_diagnostic_vector: args.seed_guest_ram_diagnostic_vector,
                        seed_executable_diagnostic_vector: args.seed_executable_diagnostic_vector,
                        try_recommended_vector_base_vbar: args.try_recommended_vector_base_vbar,
                        continue_after_recommended_vector_base_vbar: args
                            .continue_after_recommended_vector_base_vbar,
                        repair_low_vector_diagnostic_page: args.repair_low_vector_diagnostic_page,
                        remap_low_vector_to_recommended_vector: args
                            .remap_low_vector_to_recommended_vector,
                        continue_after_low_vector_repair: args.continue_after_low_vector_repair,
                        restore_low_vector_slot_before_eret: args
                            .restore_low_vector_slot_before_eret,
                        wire_interrupt_timer: args.wire_interrupt_timer,
                        stop_at_first_post_repair_device_boundary: false,
                        installer_iso_path: args.iso,
                        writable_target_disk_path: args.writable_disk,
                    },
                });
            print!("{}", probe.render_text());
            Ok(())
        }
        HvfCommand::WindowsFirmwareDeviceDiscoveryProbe(args) => {
            if args.max_exits == 0 {
                bail!("--max-exits must be greater than zero");
            }
            if args.guest_ram_mib == 0 {
                bail!("--guest-ram-mib must be greater than zero");
            }
            if args.watchdog_ms == 0 {
                bail!("--watchdog-ms must be greater than zero");
            }
            let allow_loop = args.allow_loop
                || env::var("BRIDGEVM_HVF_ALLOW_UEFI_FIRMWARE_RUN_LOOP").as_deref() == Ok("1");
            let probe = probe_windows_11_arm_uefi_firmware_device_discovery(
                WindowsArmUefiFirmwareRunLoopOptions {
                    pflash: WindowsArmUefiPflashMapOptions {
                        firmware_path: args.firmware,
                        vars_template_path: args.vars_template,
                        vars_path: args.vars,
                        create_vars: args.create_vars,
                    },
                    execution: WindowsArmUefiFirmwareRunLoopExecutionOptions {
                        allow_loop,
                        requested_exits: args.max_exits,
                        guest_ram_mib: args.guest_ram_mib,
                        watchdog_timeout_ms: args.watchdog_ms,
                        map_low_pflash_alias: args.map_low_pflash_alias,
                        seed_diagnostic_vector: args.seed_diagnostic_vector,
                        seed_guest_ram_diagnostic_vector: args.seed_guest_ram_diagnostic_vector,
                        seed_executable_diagnostic_vector: args.seed_executable_diagnostic_vector,
                        try_recommended_vector_base_vbar: args.try_recommended_vector_base_vbar,
                        continue_after_recommended_vector_base_vbar: args
                            .continue_after_recommended_vector_base_vbar,
                        repair_low_vector_diagnostic_page: args.repair_low_vector_diagnostic_page,
                        remap_low_vector_to_recommended_vector: args
                            .remap_low_vector_to_recommended_vector,
                        continue_after_low_vector_repair: args.continue_after_low_vector_repair,
                        restore_low_vector_slot_before_eret: args
                            .restore_low_vector_slot_before_eret,
                        wire_interrupt_timer: args.wire_interrupt_timer,
                        stop_at_first_post_repair_device_boundary: false,
                        installer_iso_path: args.iso,
                        writable_target_disk_path: args.writable_disk,
                    },
                },
            );
            print!("{}", probe.render_text());
            Ok(())
        }
        HvfCommand::WindowsPlatformDescriptionProbe(args) => {
            if args.memory_gib == 0 {
                bail!("--memory-gib must be greater than zero");
            }
            if args.vcpus == 0 {
                bail!("--vcpus must be greater than zero");
            }
            let probe =
                probe_windows_11_arm_platform_description(WindowsArmPlatformDescriptionOptions {
                    guest_ram_bytes: u64::from(args.memory_gib) * 1024 * 1024 * 1024,
                    vcpu_count: args.vcpus,
                });
            print!("{}", probe.render_text());
            Ok(())
        }
        HvfCommand::WindowsXhciHidBootKeyProbe => {
            let probe = probe_windows_11_arm_xhci_hid_boot_key_report();
            print!("{}", probe.render_text());
            Ok(())
        }
        HvfCommand::HostCapabilities => {
            let capabilities = query_hvf_host_capabilities();
            print!("{}", capabilities.render_text());
            Ok(())
        }
        HvfCommand::VmProbe(args) => {
            let allow_create =
                args.allow_create || env::var("BRIDGEVM_HVF_ALLOW_VM_CREATE").as_deref() == Ok("1");
            let probe = probe_hvf_vm_create(allow_create);
            print!("{}", probe.render_text());
            Ok(())
        }
        HvfCommand::VcpuProbe(args) => {
            let allow_create =
                args.allow_create || env::var("BRIDGEVM_HVF_ALLOW_VM_CREATE").as_deref() == Ok("1");
            let probe = probe_hvf_vcpu_create(allow_create);
            print!("{}", probe.render_text());
            Ok(())
        }
        HvfCommand::VcpuRunProbe(args) => {
            let allow_run =
                args.allow_run || env::var("BRIDGEVM_HVF_ALLOW_VCPU_RUN").as_deref() == Ok("1");
            let probe = probe_hvf_vcpu_run(allow_run);
            print!("{}", probe.render_text());
            Ok(())
        }
        HvfCommand::InterruptTimerProbe(args) => {
            let allow_probe = args.allow_interrupt_timer
                || env::var("BRIDGEVM_HVF_ALLOW_INTERRUPT_TIMER").as_deref() == Ok("1");
            let probe = probe_hvf_interrupt_timer(allow_probe);
            print!("{}", probe.render_text());
            Ok(())
        }
        HvfCommand::VtimerExitProbe(args) => {
            let allow_probe =
                args.allow_vtimer_exit || env_truthy("BRIDGEVM_HVF_ALLOW_VTIMER_EXIT");
            let probe = probe_hvf_vtimer_exit(allow_probe);
            print!("{}", probe.render_text());
            Ok(())
        }
        HvfCommand::MemoryMapProbe(args) => {
            let allow_map =
                args.allow_map || env::var("BRIDGEVM_HVF_ALLOW_MEMORY_MAP").as_deref() == Ok("1");
            let probe = probe_hvf_memory_map(allow_map);
            print!("{}", probe.render_text());
            Ok(())
        }
        HvfCommand::GuestEntryProbe(args) => {
            let allow_entry = args.allow_entry
                || env::var("BRIDGEVM_HVF_ALLOW_GUEST_ENTRY").as_deref() == Ok("1");
            let probe = probe_hvf_guest_entry(allow_entry);
            print!("{}", probe.render_text());
            Ok(())
        }
        HvfCommand::GuestExitLoopProbe(args) => {
            let allow_loop =
                args.allow_loop || env::var("BRIDGEVM_HVF_ALLOW_EXIT_LOOP").as_deref() == Ok("1");
            let probe = probe_hvf_guest_exit_loop(allow_loop);
            print!("{}", probe.render_text());
            Ok(())
        }
        HvfCommand::MmioReadProbe(args) => {
            let allow_mmio =
                args.allow_mmio || env::var("BRIDGEVM_HVF_ALLOW_MMIO_READ").as_deref() == Ok("1");
            let probe = probe_hvf_mmio_read_exit(allow_mmio);
            print!("{}", probe.render_text());
            Ok(())
        }
        HvfCommand::MmioReadEmulationProbe(args) => {
            let allow_emulate = args.allow_emulate
                || env::var("BRIDGEVM_HVF_ALLOW_MMIO_EMULATION").as_deref() == Ok("1");
            let probe = probe_hvf_mmio_read_emulation(allow_emulate);
            print!("{}", probe.render_text());
            Ok(())
        }
        HvfCommand::MmioWriteEmulationProbe(args) => {
            let allow_emulate = args.allow_emulate
                || env::var("BRIDGEVM_HVF_ALLOW_MMIO_WRITE_EMULATION").as_deref() == Ok("1");
            let probe = probe_hvf_mmio_write_emulation(allow_emulate);
            print!("{}", probe.render_text());
            Ok(())
        }
        HvfCommand::MmioSerialDeviceProbe(args) => {
            let allow_device = args.allow_device
                || env::var("BRIDGEVM_HVF_ALLOW_MMIO_SERIAL_DEVICE").as_deref() == Ok("1");
            let probe = probe_hvf_mmio_serial_device(allow_device);
            print!("{}", probe.render_text());
            Ok(())
        }
        HvfCommand::MmioRtcDeviceProbe(args) => {
            let allow_device = args.allow_device
                || env::var("BRIDGEVM_HVF_ALLOW_MMIO_RTC_DEVICE").as_deref() == Ok("1");
            let probe = probe_hvf_mmio_rtc_device(allow_device);
            print!("{}", probe.render_text());
            Ok(())
        }
        HvfCommand::MmioBlockDeviceProbe(args) => {
            let allow_device = args.allow_device
                || env::var("BRIDGEVM_HVF_ALLOW_MMIO_BLOCK_DEVICE").as_deref() == Ok("1");
            let probe = probe_hvf_mmio_block_device(allow_device);
            print!("{}", probe.render_text());
            Ok(())
        }
        HvfCommand::MmioBlockQueueProbe(args) => {
            let backing_selectors = usize::from(args.disk.is_some())
                + usize::from(args.iso.is_some())
                + usize::from(args.writable_disk.is_some());
            if backing_selectors > 1 {
                bail!("--disk, --iso, and --writable-disk are mutually exclusive for hvf mmio-block-queue-probe");
            }
            let allow_device = args.allow_device
                || env::var("BRIDGEVM_HVF_ALLOW_MMIO_BLOCK_QUEUE").as_deref() == Ok("1");
            let probe =
                probe_hvf_mmio_block_queue(allow_device, args.disk, args.iso, args.writable_disk);
            print!("{}", probe.render_text());
            Ok(())
        }
        HvfCommand::VirtioBlockRequestModelProbe => {
            let probe = probe_virtio_block_request_model();
            print!("{}", probe.render_text());
            Ok(())
        }
        HvfCommand::VirtioBlockFileBackingProbe(args) => {
            let probe = probe_virtio_block_file_backing(args.disk);
            print!("{}", probe.render_text());
            Ok(())
        }
        HvfCommand::VirtioBlockWritableFileBackingProbe(args) => {
            let probe = probe_virtio_block_writable_file_backing(args.disk);
            print!("{}", probe.render_text());
            Ok(())
        }
        HvfCommand::VirtioBlockIsoBackingProbe(args) => {
            let probe = probe_virtio_block_iso_backing(args.iso);
            print!("{}", probe.render_text());
            Ok(())
        }
        HvfCommand::VirtioGpu3dHostPreflight(args) => {
            let probe = probe_virtio_gpu_3d_host_preflight_for(args.protocol.into());
            print!("{}", probe.render_text());
            Ok(())
        }
        HvfCommand::VirtioGpuTraceReport(args) => {
            let report = analyze_virtio_gpu_trace(&args.trace)?;
            let blockers = report.p3_blockers(args.protocol);
            print_virtio_gpu_trace_report(&args.trace, args.protocol, &report, &blockers);
            if args.require_p3_gate && !blockers.is_empty() {
                bail!("virtio-gpu P3 trace gate failed: {}", blockers.join("; "));
            }
            Ok(())
        }
    }
}

const VIRTIO_GPU_TRACE_FEATURE_VIRGL: u64 = 1 << 0;
const VIRTIO_GPU_TRACE_FEATURE_RESOURCE_BLOB: u64 = 1 << 3;
const VIRTIO_GPU_TRACE_FEATURE_CONTEXT_INIT: u64 = 1 << 4;
const VIRTIO_TRACE_FEATURE_VERSION_1: u64 = 1 << 0;
const VIRTIO_GPU_TRACE_CAPSET_VIRGL: u64 = 1;
const VIRTIO_GPU_TRACE_CAPSET_VIRGL2: u64 = 2;
const VIRTIO_GPU_TRACE_CAPSET_VENUS: u64 = 4;

#[derive(Debug, Default)]
struct VirtioGpuTraceReport {
    lines: usize,
    events: usize,
    invalid_lines: Vec<usize>,
    device_init: bool,
    backend_3d: bool,
    backend_attached: bool,
    queue_notify: bool,
    device_features_word0: Option<u64>,
    device_features_word1: Option<u64>,
    driver_features_word0: Option<u64>,
    driver_features_word1: Option<u64>,
    capset_info_ok: bool,
    virgl_capset_info_ok: bool,
    venus_capset_info_ok: bool,
    capset_ok: bool,
    virgl_capset_ok: bool,
    venus_capset_ok: bool,
    resource_create_3d_ok: bool,
    resource_attach_backing_ok: bool,
    blob_create_ok: bool,
    ctx_create_ok: bool,
    virgl_ctx_create_ok: bool,
    venus_ctx_create_ok: bool,
    submit_3d_ok: bool,
    submit_3d_nonzero_ok: bool,
    fenced_command: bool,
    fence_create: bool,
    backend_fence_parked: bool,
    fence_complete: bool,
    fence_deliver: bool,
    scanout_readbacks: u64,
    scanout_readback_throttled: u64,
    scanout_readback_bytes: u64,
    scanout_readback_nanoseconds: u64,
    scanout_readback_max_nanoseconds: u64,
    scanout_readback_transfer_nanoseconds: u64,
    scanout_readback_composite_nanoseconds: u64,
    scanout_readbacks_deferred: u64,
    scanout_blits: u64,
    scanout_blit_nanoseconds: u64,
    iosurface_verify_matched: u64,
    iosurface_verify_mismatched: u64,
    error_responses: Vec<String>,
}

impl VirtioGpuTraceReport {
    fn observe(&mut self, value: &serde_json::Value, line_number: usize) {
        match json_str(value, "event") {
            Some("device_init") => {
                self.device_init = true;
                self.backend_3d |= json_bool(value, "backend_3d").unwrap_or(false);
            }
            Some("backend_attached") => {
                self.backend_attached = true;
            }
            Some("common_read") => {
                if json_str(value, "field") == Some("device_features") {
                    match json_u64(value, "device_features_sel") {
                        Some(0) => self.device_features_word0 = json_u64(value, "value"),
                        Some(1) => self.device_features_word1 = json_u64(value, "value"),
                        _ => {}
                    }
                }
            }
            Some("driver_features") => match json_u64(value, "select") {
                Some(0) => self.driver_features_word0 = json_u64(value, "accepted"),
                Some(1) => self.driver_features_word1 = json_u64(value, "accepted"),
                _ => {}
            },
            Some("queue_notify") => {
                self.queue_notify |= json_bool(value, "valid").unwrap_or(true);
            }
            Some("command") => self.observe_command(value, line_number),
            Some("fence_create") => {
                self.fence_create = true;
                self.backend_fence_parked |= json_bool(value, "backend_accepted").unwrap_or(false)
                    && json_str(value, "outcome") == Some("parked");
            }
            Some("fence_complete") => self.fence_complete = true,
            Some("fence_deliver") => self.fence_deliver = true,
            Some("scanout_readback") => {
                self.scanout_readbacks = self.scanout_readbacks.saturating_add(1);
                self.scanout_readback_bytes = self
                    .scanout_readback_bytes
                    .saturating_add(json_u64(value, "bytes").unwrap_or(0));
                let duration_ns = json_u64(value, "duration_ns").unwrap_or(0);
                self.scanout_readback_nanoseconds = self
                    .scanout_readback_nanoseconds
                    .saturating_add(duration_ns);
                self.scanout_readback_max_nanoseconds =
                    self.scanout_readback_max_nanoseconds.max(duration_ns);
                self.scanout_readback_transfer_nanoseconds = self
                    .scanout_readback_transfer_nanoseconds
                    .saturating_add(json_u64(value, "transfer_ns").unwrap_or(0));
                self.scanout_readback_composite_nanoseconds = self
                    .scanout_readback_composite_nanoseconds
                    .saturating_add(json_u64(value, "composite_ns").unwrap_or(0));
                if json_u64(value, "deferred").unwrap_or(0) == 1 {
                    self.scanout_readbacks_deferred =
                        self.scanout_readbacks_deferred.saturating_add(1);
                }
            }
            Some("scanout_readback_throttled") => {
                self.scanout_readback_throttled = self.scanout_readback_throttled.saturating_add(1);
            }
            Some("scanout_blit") => {
                self.scanout_blits = self.scanout_blits.saturating_add(1);
                self.scanout_blit_nanoseconds = self
                    .scanout_blit_nanoseconds
                    .saturating_add(json_u64(value, "duration_ns").unwrap_or(0));
            }
            Some("scanout_iosurface_verify") => {
                if json_bool(value, "matched").unwrap_or(false) {
                    self.iosurface_verify_matched =
                        self.iosurface_verify_matched.saturating_add(1);
                } else {
                    self.iosurface_verify_mismatched =
                        self.iosurface_verify_mismatched.saturating_add(1);
                }
            }
            _ => {}
        }
    }

    fn observe_command(&mut self, value: &serde_json::Value, line_number: usize) {
        let name = json_str(value, "name").unwrap_or("UNKNOWN");
        let response = json_str(value, "response_name").unwrap_or("UNKNOWN");
        if json_bool(value, "fenced").unwrap_or(false) {
            self.fenced_command = true;
        }
        match (name, response) {
            ("GET_CAPSET_INFO", "OK_CAPSET_INFO") => {
                self.capset_info_ok = true;
                if let Some(capset_id) = json_u64(value, "response_capset_id") {
                    self.virgl_capset_info_ok |= is_virgl_capset(capset_id);
                    self.venus_capset_info_ok |= capset_id == VIRTIO_GPU_TRACE_CAPSET_VENUS;
                }
            }
            ("GET_CAPSET", "OK_CAPSET") => {
                self.capset_ok = true;
                if let Some(capset_id) = json_u64(value, "capset_id") {
                    self.virgl_capset_ok |= is_virgl_capset(capset_id);
                    self.venus_capset_ok |= capset_id == VIRTIO_GPU_TRACE_CAPSET_VENUS;
                }
            }
            ("RESOURCE_CREATE_BLOB", "OK_NODATA") => self.blob_create_ok = true,
            ("RESOURCE_CREATE_3D", "OK_NODATA") => self.resource_create_3d_ok = true,
            ("RESOURCE_ATTACH_BACKING", "OK_NODATA") => self.resource_attach_backing_ok = true,
            ("CTX_CREATE", "OK_NODATA") => {
                self.ctx_create_ok = true;
                if let Some(context_init) = json_u64(value, "context_init") {
                    let capset_id = context_init & 0xff;
                    self.virgl_ctx_create_ok |= is_virgl_capset(capset_id);
                    self.venus_ctx_create_ok |= capset_id == VIRTIO_GPU_TRACE_CAPSET_VENUS;
                }
            }
            ("SUBMIT_3D", "OK_NODATA") => {
                self.submit_3d_ok = true;
                self.submit_3d_nonzero_ok |=
                    json_u64(value, "submit_size").is_some_and(|size| size > 0);
            }
            _ => {}
        }
        if response.starts_with("ERR_") {
            let seq = json_u64(value, "seq")
                .map(|seq| seq.to_string())
                .unwrap_or_else(|| "?".to_string());
            self.error_responses.push(format!(
                "line {line_number}, seq {seq}: {name} -> {response}"
            ));
        }
    }

    fn has_3d_backend(&self) -> bool {
        self.backend_3d || self.backend_attached
    }

    fn accepted_venus_features(&self) -> bool {
        let required = VIRTIO_GPU_TRACE_FEATURE_VIRGL
            | VIRTIO_GPU_TRACE_FEATURE_RESOURCE_BLOB
            | VIRTIO_GPU_TRACE_FEATURE_CONTEXT_INIT;
        self.driver_features_word0
            .is_some_and(|features| features & required == required)
    }

    fn accepted_version_1(&self) -> bool {
        self.driver_features_word1
            .is_some_and(|features| features & VIRTIO_TRACE_FEATURE_VERSION_1 != 0)
    }

    fn fence_lifecycle_observed(&self) -> bool {
        self.fenced_command && self.fence_create && (self.fence_complete || self.fence_deliver)
    }

    fn scanout_readback_average_us(&self) -> f64 {
        if self.scanout_readbacks == 0 {
            return 0.0;
        }
        self.scanout_readback_nanoseconds as f64 / self.scanout_readbacks as f64 / 1_000.0
    }

    fn scanout_readback_phase_average_us(&self, phase_nanoseconds: u64) -> f64 {
        if self.scanout_readbacks == 0 {
            return 0.0;
        }
        phase_nanoseconds as f64 / self.scanout_readbacks as f64 / 1_000.0
    }

    fn scanout_readback_effective_gbps(&self) -> f64 {
        if self.scanout_readback_nanoseconds == 0 {
            return 0.0;
        }
        self.scanout_readback_bytes as f64 / self.scanout_readback_nanoseconds as f64
    }

    fn scanout_throttle_percent(&self) -> f64 {
        let observed = self
            .scanout_readbacks
            .saturating_add(self.scanout_readback_throttled);
        if observed == 0 {
            return 0.0;
        }
        self.scanout_readback_throttled as f64 / observed as f64 * 100.0
    }

    fn p3_blockers(&self, protocol: VirtioGpuTraceProtocolChoice) -> Vec<String> {
        let mut blockers = Vec::new();
        if !self.invalid_lines.is_empty() {
            blockers.push(format!(
                "invalid JSONL trace lines present: {}",
                self.invalid_lines
                    .iter()
                    .map(|line| line.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        if self.events == 0 {
            blockers.push("trace contains no parsed events".to_string());
        }
        if !self.device_init {
            blockers.push("missing device_init event".to_string());
        }
        if !self.has_3d_backend() {
            blockers.push("3D backend not attached in trace".to_string());
        }
        if !self.accepted_version_1() {
            blockers.push("driver did not accept VIRTIO_F_VERSION_1".to_string());
        }
        if !self.queue_notify {
            blockers.push("missing valid virtio-gpu queue_notify".to_string());
        }
        if !self.capset_info_ok {
            blockers.push("missing successful GET_CAPSET_INFO".to_string());
        }
        if !self.capset_ok {
            blockers.push("missing successful GET_CAPSET".to_string());
        }
        if !self.ctx_create_ok {
            blockers.push("missing successful CTX_CREATE".to_string());
        }
        if !self.submit_3d_ok {
            blockers.push("missing successful SUBMIT_3D".to_string());
        }
        if !self.submit_3d_nonzero_ok {
            blockers.push("missing successful non-empty SUBMIT_3D".to_string());
        }
        if !self.backend_fence_parked {
            blockers.push("missing backend-parked renderer fence".to_string());
        }
        if !self.fence_lifecycle_observed() {
            blockers
                .push("missing fenced command plus fence create/completion/delivery".to_string());
        }
        blockers.extend(self.protocol_blockers(protocol));
        blockers
    }

    fn protocol_blockers(&self, protocol: VirtioGpuTraceProtocolChoice) -> Vec<String> {
        match protocol {
            VirtioGpuTraceProtocolChoice::Venus => self.venus_protocol_blockers(),
            VirtioGpuTraceProtocolChoice::Virgl => self.virgl_protocol_blockers(),
            VirtioGpuTraceProtocolChoice::Auto => {
                let venus = self.venus_protocol_blockers();
                let virgl = self.virgl_protocol_blockers();
                if venus.is_empty() || virgl.is_empty() {
                    Vec::new()
                } else {
                    vec![format!(
                        "trace did not satisfy VENUS or VIRGL protocol identity (VENUS: {}; VIRGL: {})",
                        venus.join(", "),
                        virgl.join(", ")
                    )]
                }
            }
        }
    }

    fn venus_protocol_blockers(&self) -> Vec<String> {
        let mut blockers = Vec::new();
        if !self.accepted_venus_features() {
            blockers
                .push("driver did not accept VIRGL, RESOURCE_BLOB, and CONTEXT_INIT".to_string());
        }
        if !self.blob_create_ok {
            blockers.push("missing successful RESOURCE_CREATE_BLOB".to_string());
        }
        if !self.venus_capset_info_ok {
            blockers.push("GET_CAPSET_INFO did not report VENUS capset id 4".to_string());
        }
        if !self.venus_capset_ok {
            blockers.push("missing successful GET_CAPSET for VENUS capset id 4".to_string());
        }
        if !self.venus_ctx_create_ok {
            blockers.push("missing CTX_CREATE with VENUS context_init low byte 4".to_string());
        }
        blockers
    }

    fn virgl_protocol_blockers(&self) -> Vec<String> {
        let mut blockers = Vec::new();
        if !self.resource_create_3d_ok {
            blockers.push("missing successful RESOURCE_CREATE_3D".to_string());
        }
        if !self.resource_attach_backing_ok {
            blockers.push("missing successful RESOURCE_ATTACH_BACKING".to_string());
        }
        if !self.virgl_capset_info_ok {
            blockers
                .push("GET_CAPSET_INFO did not report VIRGL/VIRGL2 capset id 1 or 2".to_string());
        }
        if !self.virgl_capset_ok {
            blockers.push(
                "missing successful GET_CAPSET for VIRGL/VIRGL2 capset id 1 or 2".to_string(),
            );
        }
        if !self.virgl_ctx_create_ok {
            blockers.push(
                "missing CTX_CREATE with VIRGL/VIRGL2 context_init low byte 1 or 2".to_string(),
            );
        }
        blockers
    }

    fn selected_protocol(&self, protocol: VirtioGpuTraceProtocolChoice) -> &'static str {
        let venus_ok = self.venus_protocol_blockers().is_empty();
        let virgl_ok = self.virgl_protocol_blockers().is_empty();
        match protocol {
            VirtioGpuTraceProtocolChoice::Venus if venus_ok => "venus",
            VirtioGpuTraceProtocolChoice::Venus => "venus-missing",
            VirtioGpuTraceProtocolChoice::Virgl if virgl_ok => "virgl",
            VirtioGpuTraceProtocolChoice::Virgl => "virgl-missing",
            VirtioGpuTraceProtocolChoice::Auto if venus_ok && virgl_ok => "venus+virgl",
            VirtioGpuTraceProtocolChoice::Auto if venus_ok => "venus",
            VirtioGpuTraceProtocolChoice::Auto if virgl_ok => "virgl",
            VirtioGpuTraceProtocolChoice::Auto => "unknown",
        }
    }
}

fn is_virgl_capset(capset_id: u64) -> bool {
    capset_id == VIRTIO_GPU_TRACE_CAPSET_VIRGL || capset_id == VIRTIO_GPU_TRACE_CAPSET_VIRGL2
}

fn analyze_virtio_gpu_trace(path: &Path) -> Result<VirtioGpuTraceReport> {
    let file = fs::File::open(path)
        .with_context(|| format!("failed to open virtio-gpu trace {}", path.display()))?;
    let mut report = VirtioGpuTraceReport::default();
    for (index, line) in BufReader::new(file).lines().enumerate() {
        let line_number = index + 1;
        let line = line.with_context(|| format!("failed to read trace line {line_number}"))?;
        if line.trim().is_empty() {
            continue;
        }
        report.lines += 1;
        match serde_json::from_str::<serde_json::Value>(&line) {
            Ok(value) => {
                report.events += 1;
                report.observe(&value, line_number);
            }
            Err(_) => report.invalid_lines.push(line_number),
        }
    }
    Ok(report)
}

fn print_virtio_gpu_trace_report(
    path: &Path,
    protocol: VirtioGpuTraceProtocolChoice,
    report: &VirtioGpuTraceReport,
    blockers: &[String],
) {
    println!("BridgeVM HVF virtio-gpu trace report");
    println!("Trace: {}", path.display());
    println!("Requested protocol: {}", protocol.label());
    println!("Selected protocol: {}", report.selected_protocol(protocol));
    println!("Non-empty lines: {}", report.lines);
    println!("Parsed events: {}", report.events);
    println!("Invalid lines: {}", report.invalid_lines.len());
    println!("Device initialized: {}", report.device_init);
    println!("3D backend attached: {}", report.has_3d_backend());
    println!(
        "Device feature word0: {}",
        hex_option(report.device_features_word0)
    );
    println!(
        "Device feature word1: {}",
        hex_option(report.device_features_word1)
    );
    println!(
        "Driver feature word0: {}",
        hex_option(report.driver_features_word0)
    );
    println!(
        "Driver feature word1: {}",
        hex_option(report.driver_features_word1)
    );
    println!(
        "VENUS feature set accepted: {}",
        report.accepted_venus_features()
    );
    println!(
        "VIRTIO_F_VERSION_1 accepted: {}",
        report.accepted_version_1()
    );
    println!("Queue notify observed: {}", report.queue_notify);
    println!("GET_CAPSET_INFO OK: {}", report.capset_info_ok);
    println!(
        "GET_CAPSET_INFO VIRGL/VIRGL2 id 1/2: {}",
        report.virgl_capset_info_ok
    );
    println!(
        "GET_CAPSET_INFO VENUS id 4: {}",
        report.venus_capset_info_ok
    );
    println!("GET_CAPSET OK: {}", report.capset_ok);
    println!("GET_CAPSET VIRGL/VIRGL2 id 1/2: {}", report.virgl_capset_ok);
    println!("GET_CAPSET VENUS id 4: {}", report.venus_capset_ok);
    println!("RESOURCE_CREATE_3D OK: {}", report.resource_create_3d_ok);
    println!(
        "RESOURCE_ATTACH_BACKING OK: {}",
        report.resource_attach_backing_ok
    );
    println!("RESOURCE_CREATE_BLOB OK: {}", report.blob_create_ok);
    println!("CTX_CREATE OK: {}", report.ctx_create_ok);
    println!(
        "CTX_CREATE VIRGL/VIRGL2 context_init: {}",
        report.virgl_ctx_create_ok
    );
    println!(
        "CTX_CREATE VENUS context_init: {}",
        report.venus_ctx_create_ok
    );
    println!("SUBMIT_3D OK: {}", report.submit_3d_ok);
    println!("SUBMIT_3D non-empty: {}", report.submit_3d_nonzero_ok);
    println!("Fenced command observed: {}", report.fenced_command);
    println!("Fence create observed: {}", report.fence_create);
    println!(
        "Backend-parked fence observed: {}",
        report.backend_fence_parked
    );
    println!("Fence complete observed: {}", report.fence_complete);
    println!("Fence deliver observed: {}", report.fence_deliver);
    println!("Scanout readbacks: {}", report.scanout_readbacks);
    println!(
        "Scanout throttled flushes: {}",
        report.scanout_readback_throttled
    );
    println!("Scanout readback bytes: {}", report.scanout_readback_bytes);
    println!(
        "Scanout readback duration ns: {}",
        report.scanout_readback_nanoseconds
    );
    println!(
        "Scanout readback average us: {:.3}",
        report.scanout_readback_average_us()
    );
    println!(
        "Scanout readback max us: {:.3}",
        report.scanout_readback_max_nanoseconds as f64 / 1_000.0
    );
    println!(
        "Scanout readback transfer avg us: {:.3}",
        report.scanout_readback_phase_average_us(report.scanout_readback_transfer_nanoseconds)
    );
    println!(
        "Scanout readback composite avg us: {:.3}",
        report.scanout_readback_phase_average_us(report.scanout_readback_composite_nanoseconds)
    );
    println!(
        "Scanout readbacks deferred-serviced: {}",
        report.scanout_readbacks_deferred
    );
    println!("Scanout IOSurface blits: {}", report.scanout_blits);
    println!(
        "Scanout IOSurface blit avg us: {:.3}",
        if report.scanout_blits == 0 {
            0.0
        } else {
            report.scanout_blit_nanoseconds as f64 / report.scanout_blits as f64 / 1_000.0
        }
    );
    println!(
        "Scanout IOSurface verify: {} matched / {} mismatched",
        report.iosurface_verify_matched, report.iosurface_verify_mismatched
    );
    println!(
        "Scanout readback effective GB/s: {:.3}",
        report.scanout_readback_effective_gbps()
    );
    println!(
        "Scanout throttle ratio: {:.2}%",
        report.scanout_throttle_percent()
    );
    if report.error_responses.is_empty() {
        println!("Error responses: none");
    } else {
        println!("Error responses: {}", report.error_responses.len());
        for response in report.error_responses.iter().take(5) {
            println!("- {response}");
        }
    }
    println!(
        "P3 Windows 3D trace gate: {}",
        if blockers.is_empty() { "PASS" } else { "FAIL" }
    );
    if blockers.is_empty() {
        println!("Blockers: none");
    } else {
        println!("Blockers:");
        for blocker in blockers {
            println!("- {blocker}");
        }
    }
}

fn json_str<'a>(value: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    value.get(key)?.as_str()
}

fn json_bool(value: &serde_json::Value, key: &str) -> Option<bool> {
    match value.get(key)? {
        serde_json::Value::Bool(value) => Some(*value),
        serde_json::Value::String(value) => match value.trim().to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" | "on" => Some(true),
            "false" | "0" | "no" | "off" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

fn json_u64(value: &serde_json::Value, key: &str) -> Option<u64> {
    match value.get(key)? {
        serde_json::Value::Number(value) => value
            .as_u64()
            .or_else(|| value.as_i64().and_then(|signed| signed.try_into().ok())),
        serde_json::Value::String(value) => {
            let value = value.trim();
            value
                .strip_prefix("0x")
                .or_else(|| value.strip_prefix("0X"))
                .map(|hex| u64::from_str_radix(hex, 16).ok())
                .unwrap_or_else(|| value.parse().ok())
        }
        _ => None,
    }
}

fn hex_option(value: Option<u64>) -> String {
    value
        .map(|value| format!("{value:#x}"))
        .unwrap_or_else(|| "missing".to_string())
}

fn env_truthy(name: &str) -> bool {
    match env::var(name) {
        Ok(value) => matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ),
        Err(_) => false,
    }
}

fn print_mode_recommendation(rec: &ModeRecommendation, choice: Option<&GuestChoice>) {
    println!("Recommended mode: {}", rec.mode);
    print_recommendation_engine_context(rec, choice);
    println!("Expected performance: {}", rec.performance);
    println!("Battery impact: {}", rec.battery_impact);
    println!("Integration: {}", rec.integration);
    println!("{}", rec.message);
    if let Some(template) = &rec.boot_template {
        print_boot_template(template);
    }
}

fn print_recommendation_engine_context(rec: &ModeRecommendation, choice: Option<&GuestChoice>) {
    let current = current_engine_descriptor_for_mode(rec.mode);
    println!(
        "Current execution engine: {} ({})",
        current.label,
        current.lane.id()
    );
    println!("Current engine substrate: {}", current.substrate);
    println!("Current engine QEMU usage: {}", current.qemu_usage);
    if let Some(target) = choice.and_then(target_engine_descriptor_for_guest) {
        println!(
            "Target product engine: {} ({})",
            target.label,
            target.lane.id()
        );
        println!("Target engine substrate: {}", target.substrate);
        println!("Target engine QEMU usage: {}", target.qemu_usage);
        println!("Target engine state: {}", target.product_state_detail);
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
    if let Some(storage) = &template.storage {
        println!("Primary disk path: {}", storage.primary.path);
        println!("Primary disk format: {}", storage.primary.format);
        println!("Primary disk size: {}", storage.primary.size);
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
    print_engine_catalog(available_engine_descriptors());
    print_parallels_class_progress(&parallels_class_progress());
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProductTrackStatus {
    Proven,
    Partial,
    Planned,
}

impl ProductTrackStatus {
    fn as_str(self) -> &'static str {
        match self {
            ProductTrackStatus::Proven => "PROVEN",
            ProductTrackStatus::Partial => "PARTIAL",
            ProductTrackStatus::Planned => "PLANNED",
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
struct ProductTrackProgress {
    status: ProductTrackStatus,
    name: &'static str,
    implemented: &'static str,
    next: &'static str,
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

fn print_engine_catalog(descriptors: &[VmEngineDescriptor]) {
    println!("Engine lanes:");
    for descriptor in descriptors {
        println!(
            "[{}] {} ({}): {}",
            descriptor.product_state.as_str(),
            descriptor.label,
            descriptor.lane.id(),
            descriptor.product_state_detail
        );
        println!("    Substrate: {}", descriptor.substrate);
        println!("    Guest scope: {}", descriptor.guest_scope);
        println!(
            "    Windows 11 Arm role: {}",
            descriptor.windows_11_arm_role
        );
        println!("    QEMU: {}", descriptor.qemu_usage);
    }
}

fn parallels_class_progress() -> Vec<ProductTrackProgress> {
    vec![
        ProductTrackProgress {
            status: ProductTrackStatus::Partial,
            name: "macOS-native integration / Coherence",
            implemented:
                "clipboard/display resize foundations plus preserved Linux .desktop/gio/gtk-launch/wmctrl live GUI proof and crop/proxy plumbing",
            next: "drive real guest-window crops from real framebuffer/proxy sessions, then move toward compositor-grade host-window integration",
        },
        ProductTrackProgress {
            status: ProductTrackStatus::Proven,
            name: "Apple Silicon Fast Mode",
            implemented:
                "Apple Virtualization.framework path with live Linux Arm64 boot/suspend/resume and VZVirtualMachineView display",
            next: "broaden boot shapes and keep app/daemon/helper IPC tight",
        },
        ProductTrackProgress {
            status: ProductTrackStatus::Partial,
            name: "intelligent resources / battery",
            implemented:
                "power-aware launch policy, display pacing consumption, and runtime policy IPC",
            next: "live Apple VZ CPU/RAM control must apply the policy to a running VM",
        },
        ProductTrackProgress {
            status: ProductTrackStatus::Planned,
            name: "graphics acceleration / Metal",
            implemented: "native VZ GUI pixels are proven in an AppKit display window",
            next: "Metal compositor/frame pacing first; Direct3D-to-Metal or WDDM remains long-term R&D",
        },
    ]
}

fn print_parallels_class_progress(progress: &[ProductTrackProgress]) {
    println!("Parallels-class progress:");
    for track in progress {
        println!(
            "[{}] {}: {}",
            track.status.as_str(),
            track.name,
            track.implemented
        );
        println!("    Next: {}", track.next);
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

    fn unique_socket_path(prefix: &str) -> PathBuf {
        let mut path = PathBuf::from("/tmp");
        path.push(format!(
            "{prefix}-{}-{}.sock",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        path
    }

    fn unique_trace_path(prefix: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "{prefix}-{}-{}.jsonl",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        path
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

    fn create_args_for_windows_11_arm(name: &str) -> CreateArgs {
        CreateArgs {
            name: name.to_string(),
            template: None,
            os: Some("windows".to_string()),
            version: Some("11".to_string()),
            arch: Some("arm64".to_string()),
            mode: ModeChoice::Auto,
            disk: Some("128GiB".to_string()),
            disk_format: Some(DiskFormatChoice::Qcow2),
            boot_mode: None,
            installer_image: None,
            kernel_path: None,
            initrd_path: None,
            kernel_command_line: None,
            macos_restore_image: None,
        }
    }

    #[test]
    fn create_auto_uses_compatibility_for_windows_11_arm() {
        let manifest =
            manifest_for_create(create_args_for_windows_11_arm("win11")).expect("manifest");

        assert_eq!(manifest.mode, VmMode::Compatibility);
        assert_eq!(manifest.guest.os, "windows");
        assert_eq!(manifest.guest.version.as_deref(), Some("11"));
        assert_eq!(manifest.guest.arch, "arm64");
    }

    #[test]
    fn create_rejects_explicit_fast_mode_for_windows_11_arm() {
        let mut args = create_args_for_windows_11_arm("win11");
        args.mode = ModeChoice::Fast;

        let error = manifest_for_create(args).expect_err("Windows should not be Fast Mode");
        assert!(error
            .to_string()
            .contains("Apple VZ Fast Mode is Linux/macOS Arm only"));
    }

    #[test]
    fn create_accepts_raw_disk_format_for_fast_linux_kernel_live_path() {
        let cli = Cli::try_parse_from([
            "bridgevm",
            "create",
            "vz-linux",
            "--os",
            "ubuntu",
            "--arch",
            "arm64",
            "--mode",
            "fast",
            "--boot-mode",
            "linux-kernel",
            "--kernel-path",
            "boot/vmlinuz",
            "--initrd-path",
            "boot/initrd",
            "--kernel-command-line",
            "console=hvc0 root=/dev/vda",
            "--disk",
            "64MiB",
            "--disk-format",
            "raw",
        ])
        .unwrap();
        let Command::Create(args) = cli.command else {
            panic!("expected create command");
        };

        let manifest = manifest_for_create(args).expect("manifest");
        assert_eq!(manifest.mode, VmMode::Fast);
        assert_eq!(manifest.storage.primary.path, "disks/root.raw");
        assert_eq!(manifest.storage.primary.format, "raw");
        assert_eq!(manifest.storage.primary.size, "64MiB");
        let boot = manifest.boot.expect("boot");
        assert_eq!(boot.mode, BootMode::LinuxKernel);
        assert_eq!(boot.kernel_path.as_deref(), Some("boot/vmlinuz"));
        assert_eq!(boot.initrd_path.as_deref(), Some("boot/initrd"));
        assert_eq!(
            boot.kernel_command_line.as_deref(),
            Some("console=hvc0 root=/dev/vda")
        );
    }

    #[test]
    fn create_keeps_qcow2_defaults_without_template_storage() {
        let cli = Cli::try_parse_from([
            "bridgevm",
            "create",
            "plain-linux",
            "--os",
            "ubuntu",
            "--arch",
            "arm64",
        ])
        .unwrap();
        let Command::Create(args) = cli.command else {
            panic!("expected create command");
        };

        let manifest = manifest_for_create(args).expect("manifest");
        assert_eq!(manifest.mode, VmMode::Fast);
        assert_eq!(manifest.storage.primary.path, "disks/root.qcow2");
        assert_eq!(manifest.storage.primary.format, "qcow2");
        assert_eq!(manifest.storage.primary.size, DEFAULT_PRIMARY_DISK_SIZE);
    }

    #[test]
    fn create_uses_debian_apple_vz_linux_kernel_raw_template_storage() {
        let cli = Cli::try_parse_from([
            "bridgevm",
            "create",
            "try-vz-linux",
            "--template",
            "debian-arm64-apple-vz-linux-kernel-raw",
        ])
        .unwrap();
        let Command::Create(args) = cli.command else {
            panic!("expected create command");
        };

        let manifest = manifest_for_create(args).expect("manifest");
        assert_eq!(manifest.mode, VmMode::Fast);
        assert_eq!(manifest.guest.os, "debian");
        assert_eq!(manifest.guest.arch, "arm64");
        assert_eq!(manifest.storage.primary.path, "disks/root.raw");
        assert_eq!(manifest.storage.primary.format, "raw");
        assert_eq!(manifest.storage.primary.size, "64MiB");
        let boot = manifest.boot.expect("boot");
        assert_eq!(boot.mode, BootMode::LinuxKernel);
        assert_eq!(boot.kernel_path.as_deref(), Some("boot/vmlinuz"));
        assert_eq!(boot.initrd_path.as_deref(), Some("boot/initrd"));
        assert_eq!(
            boot.kernel_command_line.as_deref(),
            Some("console=hvc0 priority=low")
        );
    }

    #[test]
    fn create_uses_ubuntu_apple_vz_linux_kernel_raw_template_storage() {
        let cli = Cli::try_parse_from([
            "bridgevm",
            "create",
            "ubuntu-desktop-vz",
            "--template",
            "ubuntu-arm64-apple-vz-linux-kernel-raw",
        ])
        .unwrap();
        let Command::Create(args) = cli.command else {
            panic!("expected create command");
        };

        let manifest = manifest_for_create(args).expect("manifest");
        assert_eq!(manifest.mode, VmMode::Fast);
        assert_eq!(manifest.guest.os, "ubuntu");
        assert_eq!(manifest.guest.arch, "arm64");
        assert_eq!(manifest.storage.primary.path, "disks/root.raw");
        assert_eq!(manifest.storage.primary.format, "raw");
        assert_eq!(manifest.storage.primary.size, "32GiB");
        let boot = manifest.boot.expect("boot");
        assert_eq!(boot.mode, BootMode::LinuxKernel);
        assert_eq!(boot.kernel_path.as_deref(), Some("boot/vmlinuz"));
        assert_eq!(boot.initrd_path.as_deref(), Some("boot/initrd"));
        assert_eq!(
            boot.kernel_command_line.as_deref(),
            Some("console=hvc0 root=/dev/vda2 rw systemd.unit=graphical.target")
        );
    }

    #[test]
    fn socket_request_for_plain_template_create_uses_daemon_template_api() {
        let cli = Cli::try_parse_from([
            "bridgevm",
            "create",
            "try-vz-linux",
            "--template",
            "debian-arm64-apple-vz-linux-kernel-raw",
        ])
        .unwrap();
        let Command::Create(args) = cli.command else {
            panic!("expected create command");
        };

        let request = request_for(Command::Create(args)).expect("request");
        let BridgeVmRequest::CreateVmFromTemplate { name, template_id } = request else {
            panic!("expected create-from-template request");
        };
        assert_eq!(name, "try-vz-linux");
        assert_eq!(template_id, "debian-arm64-apple-vz-linux-kernel-raw");
    }

    #[test]
    fn hvf_windows_plan_cli_accepts_installer_path() {
        let cli = Cli::try_parse_from([
            "bridgevm",
            "hvf",
            "windows-plan",
            "--installer",
            "ISO/Win11_25H2_English_Arm64_v2.iso",
        ])
        .unwrap();

        let Command::Hvf(HvfCommand::WindowsPlan(args)) = cli.command else {
            panic!("expected hvf windows-plan command");
        };

        assert_eq!(
            args.installer.as_deref(),
            Some(Path::new("ISO/Win11_25H2_English_Arm64_v2.iso"))
        );
    }

    #[test]
    fn windows_hvf_plan_render_is_blocked_and_qemu_free() {
        let plan =
            plan_windows_11_arm_no_qemu(Some(PathBuf::from("ISO/Win11_25H2_English_Arm64_v2.iso")));
        let output = plan.render_text();

        assert!(output.contains("Windows 11 Arm no-QEMU HVF plan"));
        assert!(output.contains("Engine: BridgeVM HVF"));
        assert!(output.contains("Substrate: Apple Hypervisor.framework"));
        assert!(output.contains("Installer: ISO/Win11_25H2_English_Arm64_v2.iso"));
        assert!(output.contains("QEMU: not used"));
        assert!(output.contains("Overall: blocked"));
        assert!(!output.contains("qemu-system"));
        assert!(!output.contains('%'));
    }

    #[test]
    fn hvf_host_capabilities_cli_parses() {
        let cli = Cli::try_parse_from(["bridgevm", "hvf", "host-capabilities"]).unwrap();

        let Command::Hvf(HvfCommand::HostCapabilities) = cli.command else {
            panic!("expected hvf host-capabilities command");
        };
    }

    #[test]
    fn hvf_vm_probe_cli_defaults_to_no_create() {
        let cli = Cli::try_parse_from(["bridgevm", "hvf", "vm-probe"]).unwrap();

        let Command::Hvf(HvfCommand::VmProbe(args)) = cli.command else {
            panic!("expected hvf vm-probe command");
        };

        assert!(!args.allow_create);
    }

    #[test]
    fn hvf_vm_probe_cli_accepts_explicit_create_opt_in() {
        let cli = Cli::try_parse_from(["bridgevm", "hvf", "vm-probe", "--allow-create"]).unwrap();

        let Command::Hvf(HvfCommand::VmProbe(args)) = cli.command else {
            panic!("expected hvf vm-probe command");
        };

        assert!(args.allow_create);
    }

    #[test]
    fn hvf_vcpu_probe_cli_accepts_explicit_create_opt_in() {
        let cli = Cli::try_parse_from(["bridgevm", "hvf", "vcpu-probe", "--allow-create"]).unwrap();

        let Command::Hvf(HvfCommand::VcpuProbe(args)) = cli.command else {
            panic!("expected hvf vcpu-probe command");
        };

        assert!(args.allow_create);
    }

    #[test]
    fn hvf_vcpu_run_probe_cli_defaults_to_no_run() {
        let cli = Cli::try_parse_from(["bridgevm", "hvf", "vcpu-run-probe"]).unwrap();

        let Command::Hvf(HvfCommand::VcpuRunProbe(args)) = cli.command else {
            panic!("expected hvf vcpu-run-probe command");
        };

        assert!(!args.allow_run);
    }

    #[test]
    fn hvf_vcpu_run_probe_cli_accepts_explicit_run_opt_in() {
        let cli =
            Cli::try_parse_from(["bridgevm", "hvf", "vcpu-run-probe", "--allow-run"]).unwrap();

        let Command::Hvf(HvfCommand::VcpuRunProbe(args)) = cli.command else {
            panic!("expected hvf vcpu-run-probe command");
        };

        assert!(args.allow_run);
    }

    #[test]
    fn hvf_interrupt_timer_probe_cli_defaults_to_no_probe() {
        let cli = Cli::try_parse_from(["bridgevm", "hvf", "interrupt-timer-probe"]).unwrap();

        let Command::Hvf(HvfCommand::InterruptTimerProbe(args)) = cli.command else {
            panic!("expected hvf interrupt-timer-probe command");
        };

        assert!(!args.allow_interrupt_timer);
    }

    #[test]
    fn hvf_interrupt_timer_probe_cli_accepts_explicit_opt_in() {
        let cli = Cli::try_parse_from([
            "bridgevm",
            "hvf",
            "interrupt-timer-probe",
            "--allow-interrupt-timer",
        ])
        .unwrap();

        let Command::Hvf(HvfCommand::InterruptTimerProbe(args)) = cli.command else {
            panic!("expected hvf interrupt-timer-probe command");
        };

        assert!(args.allow_interrupt_timer);
    }

    #[test]
    fn hvf_vtimer_exit_probe_cli_defaults_to_no_probe() {
        let cli = Cli::try_parse_from(["bridgevm", "hvf", "vtimer-exit-probe"]).unwrap();

        let Command::Hvf(HvfCommand::VtimerExitProbe(args)) = cli.command else {
            panic!("expected hvf vtimer-exit-probe command");
        };

        assert!(!args.allow_vtimer_exit);
    }

    #[test]
    fn hvf_vtimer_exit_probe_cli_accepts_explicit_opt_in() {
        let cli = Cli::try_parse_from([
            "bridgevm",
            "hvf",
            "vtimer-exit-probe",
            "--allow-vtimer-exit",
        ])
        .unwrap();

        let Command::Hvf(HvfCommand::VtimerExitProbe(args)) = cli.command else {
            panic!("expected hvf vtimer-exit-probe command");
        };

        assert!(args.allow_vtimer_exit);
    }

    #[test]
    fn hvf_memory_map_probe_cli_defaults_to_no_map() {
        let cli = Cli::try_parse_from(["bridgevm", "hvf", "memory-map-probe"]).unwrap();

        let Command::Hvf(HvfCommand::MemoryMapProbe(args)) = cli.command else {
            panic!("expected hvf memory-map-probe command");
        };

        assert!(!args.allow_map);
    }

    #[test]
    fn hvf_memory_map_probe_cli_accepts_explicit_map_opt_in() {
        let cli =
            Cli::try_parse_from(["bridgevm", "hvf", "memory-map-probe", "--allow-map"]).unwrap();

        let Command::Hvf(HvfCommand::MemoryMapProbe(args)) = cli.command else {
            panic!("expected hvf memory-map-probe command");
        };

        assert!(args.allow_map);
    }

    #[test]
    fn hvf_guest_entry_probe_cli_defaults_to_no_entry() {
        let cli = Cli::try_parse_from(["bridgevm", "hvf", "guest-entry-probe"]).unwrap();

        let Command::Hvf(HvfCommand::GuestEntryProbe(args)) = cli.command else {
            panic!("expected hvf guest-entry-probe command");
        };

        assert!(!args.allow_entry);
    }

    #[test]
    fn hvf_guest_entry_probe_cli_accepts_explicit_entry_opt_in() {
        let cli =
            Cli::try_parse_from(["bridgevm", "hvf", "guest-entry-probe", "--allow-entry"]).unwrap();

        let Command::Hvf(HvfCommand::GuestEntryProbe(args)) = cli.command else {
            panic!("expected hvf guest-entry-probe command");
        };

        assert!(args.allow_entry);
    }

    #[test]
    fn hvf_guest_exit_loop_probe_cli_defaults_to_no_loop() {
        let cli = Cli::try_parse_from(["bridgevm", "hvf", "guest-exit-loop-probe"]).unwrap();

        let Command::Hvf(HvfCommand::GuestExitLoopProbe(args)) = cli.command else {
            panic!("expected hvf guest-exit-loop-probe command");
        };

        assert!(!args.allow_loop);
    }

    #[test]
    fn hvf_guest_exit_loop_probe_cli_accepts_explicit_loop_opt_in() {
        let cli = Cli::try_parse_from(["bridgevm", "hvf", "guest-exit-loop-probe", "--allow-loop"])
            .unwrap();

        let Command::Hvf(HvfCommand::GuestExitLoopProbe(args)) = cli.command else {
            panic!("expected hvf guest-exit-loop-probe command");
        };

        assert!(args.allow_loop);
    }

    #[test]
    fn hvf_mmio_read_probe_cli_defaults_to_no_mmio() {
        let cli = Cli::try_parse_from(["bridgevm", "hvf", "mmio-read-probe"]).unwrap();

        let Command::Hvf(HvfCommand::MmioReadProbe(args)) = cli.command else {
            panic!("expected hvf mmio-read-probe command");
        };

        assert!(!args.allow_mmio);
    }

    #[test]
    fn hvf_mmio_read_probe_cli_accepts_explicit_mmio_opt_in() {
        let cli =
            Cli::try_parse_from(["bridgevm", "hvf", "mmio-read-probe", "--allow-mmio"]).unwrap();

        let Command::Hvf(HvfCommand::MmioReadProbe(args)) = cli.command else {
            panic!("expected hvf mmio-read-probe command");
        };

        assert!(args.allow_mmio);
    }

    #[test]
    fn hvf_mmio_read_emulation_probe_cli_defaults_to_no_emulation() {
        let cli = Cli::try_parse_from(["bridgevm", "hvf", "mmio-read-emulation-probe"]).unwrap();

        let Command::Hvf(HvfCommand::MmioReadEmulationProbe(args)) = cli.command else {
            panic!("expected hvf mmio-read-emulation-probe command");
        };

        assert!(!args.allow_emulate);
    }

    #[test]
    fn hvf_mmio_read_emulation_probe_cli_accepts_explicit_emulation_opt_in() {
        let cli = Cli::try_parse_from([
            "bridgevm",
            "hvf",
            "mmio-read-emulation-probe",
            "--allow-emulate",
        ])
        .unwrap();

        let Command::Hvf(HvfCommand::MmioReadEmulationProbe(args)) = cli.command else {
            panic!("expected hvf mmio-read-emulation-probe command");
        };

        assert!(args.allow_emulate);
    }

    #[test]
    fn hvf_mmio_write_emulation_probe_cli_defaults_to_no_emulation() {
        let cli = Cli::try_parse_from(["bridgevm", "hvf", "mmio-write-emulation-probe"]).unwrap();

        let Command::Hvf(HvfCommand::MmioWriteEmulationProbe(args)) = cli.command else {
            panic!("expected hvf mmio-write-emulation-probe command");
        };

        assert!(!args.allow_emulate);
    }

    #[test]
    fn hvf_mmio_write_emulation_probe_cli_accepts_explicit_emulation_opt_in() {
        let cli = Cli::try_parse_from([
            "bridgevm",
            "hvf",
            "mmio-write-emulation-probe",
            "--allow-emulate",
        ])
        .unwrap();

        let Command::Hvf(HvfCommand::MmioWriteEmulationProbe(args)) = cli.command else {
            panic!("expected hvf mmio-write-emulation-probe command");
        };

        assert!(args.allow_emulate);
    }

    #[test]
    fn hvf_mmio_serial_device_probe_cli_defaults_to_no_device() {
        let cli = Cli::try_parse_from(["bridgevm", "hvf", "mmio-serial-device-probe"]).unwrap();

        let Command::Hvf(HvfCommand::MmioSerialDeviceProbe(args)) = cli.command else {
            panic!("expected hvf mmio-serial-device-probe command");
        };

        assert!(!args.allow_device);
    }

    #[test]
    fn hvf_mmio_serial_device_probe_cli_accepts_explicit_device_opt_in() {
        let cli = Cli::try_parse_from([
            "bridgevm",
            "hvf",
            "mmio-serial-device-probe",
            "--allow-device",
        ])
        .unwrap();

        let Command::Hvf(HvfCommand::MmioSerialDeviceProbe(args)) = cli.command else {
            panic!("expected hvf mmio-serial-device-probe command");
        };

        assert!(args.allow_device);
    }

    #[test]
    fn hvf_mmio_rtc_device_probe_cli_defaults_to_no_device() {
        let cli = Cli::try_parse_from(["bridgevm", "hvf", "mmio-rtc-device-probe"]).unwrap();

        let Command::Hvf(HvfCommand::MmioRtcDeviceProbe(args)) = cli.command else {
            panic!("expected hvf mmio-rtc-device-probe command");
        };

        assert!(!args.allow_device);
    }

    #[test]
    fn hvf_mmio_rtc_device_probe_cli_accepts_explicit_device_opt_in() {
        let cli =
            Cli::try_parse_from(["bridgevm", "hvf", "mmio-rtc-device-probe", "--allow-device"])
                .unwrap();

        let Command::Hvf(HvfCommand::MmioRtcDeviceProbe(args)) = cli.command else {
            panic!("expected hvf mmio-rtc-device-probe command");
        };

        assert!(args.allow_device);
    }

    #[test]
    fn hvf_mmio_block_device_probe_cli_defaults_to_no_device() {
        let cli = Cli::try_parse_from(["bridgevm", "hvf", "mmio-block-device-probe"]).unwrap();

        let Command::Hvf(HvfCommand::MmioBlockDeviceProbe(args)) = cli.command else {
            panic!("expected hvf mmio-block-device-probe command");
        };

        assert!(!args.allow_device);
    }

    #[test]
    fn hvf_mmio_block_device_probe_cli_accepts_explicit_device_opt_in() {
        let cli = Cli::try_parse_from([
            "bridgevm",
            "hvf",
            "mmio-block-device-probe",
            "--allow-device",
        ])
        .unwrap();

        let Command::Hvf(HvfCommand::MmioBlockDeviceProbe(args)) = cli.command else {
            panic!("expected hvf mmio-block-device-probe command");
        };

        assert!(args.allow_device);
    }

    #[test]
    fn hvf_mmio_block_queue_probe_cli_defaults_to_no_device() {
        let cli = Cli::try_parse_from(["bridgevm", "hvf", "mmio-block-queue-probe"]).unwrap();

        let Command::Hvf(HvfCommand::MmioBlockQueueProbe(args)) = cli.command else {
            panic!("expected hvf mmio-block-queue-probe command");
        };

        assert!(!args.allow_device);
        assert_eq!(args.disk, None);
        assert_eq!(args.iso, None);
        assert_eq!(args.writable_disk, None);
    }

    #[test]
    fn hvf_mmio_block_queue_probe_cli_accepts_explicit_device_opt_in() {
        let cli = Cli::try_parse_from([
            "bridgevm",
            "hvf",
            "mmio-block-queue-probe",
            "--allow-device",
        ])
        .unwrap();

        let Command::Hvf(HvfCommand::MmioBlockQueueProbe(args)) = cli.command else {
            panic!("expected hvf mmio-block-queue-probe command");
        };

        assert!(args.allow_device);
        assert_eq!(args.disk, None);
        assert_eq!(args.iso, None);
        assert_eq!(args.writable_disk, None);
    }

    #[test]
    fn hvf_mmio_block_queue_probe_cli_accepts_file_backing_disk() {
        let cli = Cli::try_parse_from([
            "bridgevm",
            "hvf",
            "mmio-block-queue-probe",
            "--allow-device",
            "--disk",
            "/tmp/bridgevm-live-block.img",
        ])
        .unwrap();

        let Command::Hvf(HvfCommand::MmioBlockQueueProbe(args)) = cli.command else {
            panic!("expected hvf mmio-block-queue-probe command");
        };

        assert!(args.allow_device);
        assert_eq!(
            args.disk,
            Some(PathBuf::from("/tmp/bridgevm-live-block.img"))
        );
        assert_eq!(args.iso, None);
        assert_eq!(args.writable_disk, None);
    }

    #[test]
    fn hvf_mmio_block_queue_probe_cli_accepts_read_only_iso_backing() {
        let cli = Cli::try_parse_from([
            "bridgevm",
            "hvf",
            "mmio-block-queue-probe",
            "--allow-device",
            "--iso",
            "/tmp/Win11_Arm64.iso",
        ])
        .unwrap();

        let Command::Hvf(HvfCommand::MmioBlockQueueProbe(args)) = cli.command else {
            panic!("expected hvf mmio-block-queue-probe command");
        };

        assert!(args.allow_device);
        assert_eq!(args.disk, None);
        assert_eq!(args.iso, Some(PathBuf::from("/tmp/Win11_Arm64.iso")));
        assert_eq!(args.writable_disk, None);
    }

    #[test]
    fn hvf_mmio_block_queue_probe_cli_accepts_writable_file_backing_disk() {
        let cli = Cli::try_parse_from([
            "bridgevm",
            "hvf",
            "mmio-block-queue-probe",
            "--allow-device",
            "--writable-disk",
            "/tmp/bridgevm-writable-live-block.img",
        ])
        .unwrap();

        let Command::Hvf(HvfCommand::MmioBlockQueueProbe(args)) = cli.command else {
            panic!("expected hvf mmio-block-queue-probe command");
        };

        assert!(args.allow_device);
        assert_eq!(args.disk, None);
        assert_eq!(args.iso, None);
        assert_eq!(
            args.writable_disk,
            Some(PathBuf::from("/tmp/bridgevm-writable-live-block.img"))
        );
    }

    #[test]
    fn hvf_virtio_block_request_model_probe_cli_parses() {
        let cli =
            Cli::try_parse_from(["bridgevm", "hvf", "virtio-block-request-model-probe"]).unwrap();

        let Command::Hvf(HvfCommand::VirtioBlockRequestModelProbe) = cli.command else {
            panic!("expected hvf virtio-block-request-model-probe command");
        };
    }

    #[test]
    fn hvf_virtio_block_file_backing_probe_cli_requires_disk() {
        let error = Cli::try_parse_from(["bridgevm", "hvf", "virtio-block-file-backing-probe"])
            .unwrap_err();

        assert!(error.to_string().contains("--disk"));
    }

    #[test]
    fn hvf_virtio_block_file_backing_probe_cli_accepts_disk() {
        let cli = Cli::try_parse_from([
            "bridgevm",
            "hvf",
            "virtio-block-file-backing-probe",
            "--disk",
            "/tmp/bridgevm-test.img",
        ])
        .unwrap();

        let Command::Hvf(HvfCommand::VirtioBlockFileBackingProbe(args)) = cli.command else {
            panic!("expected hvf virtio-block-file-backing-probe command");
        };

        assert_eq!(args.disk, PathBuf::from("/tmp/bridgevm-test.img"));
    }

    #[test]
    fn hvf_virtio_block_writable_file_backing_probe_cli_requires_disk() {
        let error = Cli::try_parse_from([
            "bridgevm",
            "hvf",
            "virtio-block-writable-file-backing-probe",
        ])
        .unwrap_err();

        assert!(error.to_string().contains("--disk"));
    }

    #[test]
    fn hvf_virtio_block_writable_file_backing_probe_cli_accepts_disk() {
        let cli = Cli::try_parse_from([
            "bridgevm",
            "hvf",
            "virtio-block-writable-file-backing-probe",
            "--disk",
            "/tmp/bridgevm-writable-test.img",
        ])
        .unwrap();

        let Command::Hvf(HvfCommand::VirtioBlockWritableFileBackingProbe(args)) = cli.command
        else {
            panic!("expected hvf virtio-block-writable-file-backing-probe command");
        };

        assert_eq!(args.disk, PathBuf::from("/tmp/bridgevm-writable-test.img"));
    }

    #[test]
    fn hvf_virtio_block_iso_backing_probe_cli_requires_iso() {
        let error =
            Cli::try_parse_from(["bridgevm", "hvf", "virtio-block-iso-backing-probe"]).unwrap_err();

        assert!(error.to_string().contains("--iso"));
    }

    #[test]
    fn hvf_virtio_block_iso_backing_probe_cli_accepts_iso() {
        let cli = Cli::try_parse_from([
            "bridgevm",
            "hvf",
            "virtio-block-iso-backing-probe",
            "--iso",
            "/tmp/Win11_Arm64.iso",
        ])
        .unwrap();

        let Command::Hvf(HvfCommand::VirtioBlockIsoBackingProbe(args)) = cli.command else {
            panic!("expected hvf virtio-block-iso-backing-probe command");
        };

        assert_eq!(args.iso, PathBuf::from("/tmp/Win11_Arm64.iso"));
    }

    #[test]
    fn hvf_virtio_gpu_trace_report_cli_accepts_trace_and_gate_flag() {
        let cli = Cli::try_parse_from([
            "bridgevm",
            "hvf",
            "virtio-gpu-trace-report",
            "--trace",
            "/tmp/bridgevm-virtio-gpu.jsonl",
            "--protocol",
            "virgl",
            "--require-p3-gate",
        ])
        .unwrap();

        let Command::Hvf(HvfCommand::VirtioGpuTraceReport(args)) = cli.command else {
            panic!("expected hvf virtio-gpu-trace-report command");
        };

        assert_eq!(args.trace, PathBuf::from("/tmp/bridgevm-virtio-gpu.jsonl"));
        assert_eq!(args.protocol, VirtioGpuTraceProtocolChoice::Virgl);
        assert!(args.require_p3_gate);
    }

    #[test]
    fn hvf_virtio_gpu_3d_host_preflight_cli_accepts_command() {
        let cli = Cli::try_parse_from(["bridgevm", "hvf", "virtio-gpu-3d-host-preflight"]).unwrap();

        let Command::Hvf(HvfCommand::VirtioGpu3dHostPreflight(args)) = cli.command else {
            panic!("expected hvf virtio-gpu-3d-host-preflight command");
        };

        assert_eq!(args.protocol, VirtioGpu3dHostPreflightProtocolChoice::Venus);
    }

    #[test]
    fn hvf_virtio_gpu_3d_host_preflight_cli_accepts_virgl_protocol() {
        let cli = Cli::try_parse_from([
            "bridgevm",
            "hvf",
            "virtio-gpu-3d-host-preflight",
            "--protocol",
            "virgl",
        ])
        .unwrap();

        let Command::Hvf(HvfCommand::VirtioGpu3dHostPreflight(args)) = cli.command else {
            panic!("expected hvf virtio-gpu-3d-host-preflight command");
        };

        assert_eq!(args.protocol, VirtioGpu3dHostPreflightProtocolChoice::Virgl);
    }

    #[test]
    fn virtio_gpu_trace_report_passes_p3_gate_on_complete_trace() {
        let path = unique_trace_path("bridgevm-cli-virtio-gpu-pass");
        fs::write(&path, complete_virtio_gpu_trace_sample()).unwrap();

        let report = analyze_virtio_gpu_trace(&path).unwrap();
        let _ = fs::remove_file(path);

        assert_eq!(report.events, 13);
        assert!(report.device_init);
        assert!(report.has_3d_backend());
        assert!(report.accepted_venus_features());
        assert!(report.accepted_version_1());
        assert!(report.capset_info_ok);
        assert!(report.venus_capset_info_ok);
        assert!(report.capset_ok);
        assert!(report.venus_capset_ok);
        assert!(report.blob_create_ok);
        assert!(report.ctx_create_ok);
        assert!(report.venus_ctx_create_ok);
        assert!(report.submit_3d_ok);
        assert!(report.submit_3d_nonzero_ok);
        assert!(report.backend_fence_parked);
        assert!(report.fence_lifecycle_observed());
        assert!(report
            .p3_blockers(VirtioGpuTraceProtocolChoice::Auto)
            .is_empty());
        assert!(report
            .p3_blockers(VirtioGpuTraceProtocolChoice::Venus)
            .is_empty());
    }

    #[test]
    fn virtio_gpu_trace_report_flags_missing_submit_and_fence() {
        let path = unique_trace_path("bridgevm-cli-virtio-gpu-missing");
        fs::write(
            &path,
            r#"{"seq":1,"event":"device_init","backend_3d":true}
{"seq":2,"event":"driver_features","select":0,"accepted":8}
{"seq":3,"event":"driver_features","select":1,"accepted":1}
{"seq":4,"event":"queue_notify","valid":true}
{"seq":5,"event":"command","name":"GET_CAPSET_INFO","response_name":"OK_CAPSET_INFO","response_capset_id":4,"response_capset_max_version":1,"response_capset_max_size":64}
{"seq":6,"event":"command","name":"GET_CAPSET","response_name":"OK_CAPSET","capset_id":4,"capset_version":1}
{"seq":7,"event":"command","name":"RESOURCE_CREATE_BLOB","response_name":"OK_NODATA"}
{"seq":8,"event":"command","name":"CTX_CREATE","response_name":"OK_NODATA","context_init":4}
"#,
        )
        .unwrap();

        let report = analyze_virtio_gpu_trace(&path).unwrap();
        let _ = fs::remove_file(path);
        let blockers = report.p3_blockers(VirtioGpuTraceProtocolChoice::Auto);

        assert!(blockers
            .iter()
            .any(|blocker| blocker == "missing successful SUBMIT_3D"));
        assert!(blockers.iter().any(|blocker| {
            blocker == "missing fenced command plus fence create/completion/delivery"
        }));
    }

    #[test]
    fn virtio_gpu_trace_report_aggregates_scanout_readbacks() {
        let path = unique_trace_path("bridgevm-cli-virtio-gpu-scanout");
        fs::write(
            &path,
            r#"{"event":"scanout_readback","bytes":4096000,"duration_ns":800000}
{"event":"scanout_readback_throttled"}
{"event":"scanout_readback","bytes":4096000,"duration_ns":1200000}
"#,
        )
        .unwrap();

        let report = analyze_virtio_gpu_trace(&path).unwrap();
        let _ = fs::remove_file(path);

        assert_eq!(report.scanout_readbacks, 2);
        assert_eq!(report.scanout_readback_throttled, 1);
        assert_eq!(report.scanout_readback_bytes, 8_192_000);
        assert_eq!(report.scanout_readback_nanoseconds, 2_000_000);
        assert_eq!(report.scanout_readback_max_nanoseconds, 1_200_000);
        assert!((report.scanout_readback_average_us() - 1_000.0).abs() < f64::EPSILON);
        assert!((report.scanout_readback_effective_gbps() - 4.096).abs() < f64::EPSILON);
        assert!((report.scanout_throttle_percent() - 100.0 / 3.0).abs() < 1e-12);
    }

    #[test]
    fn virtio_gpu_trace_report_protocol_gate_distinguishes_venus_and_virgl() {
        let path = unique_trace_path("bridgevm-cli-virtio-gpu-non-venus");
        fs::write(
            &path,
            r#"{"seq":1,"event":"device_init","backend_3d":true}
{"seq":2,"event":"driver_features","select":0,"accepted":25}
{"seq":3,"event":"driver_features","select":1,"accepted":1}
{"seq":4,"event":"queue_notify","valid":true}
{"seq":5,"event":"command","name":"GET_CAPSET_INFO","response_name":"OK_CAPSET_INFO","response_capset_id":1,"response_capset_max_version":1,"response_capset_max_size":64}
{"seq":6,"event":"command","name":"GET_CAPSET","response_name":"OK_CAPSET","capset_id":1,"capset_version":1}
{"seq":7,"event":"command","name":"RESOURCE_CREATE_3D","response_name":"OK_NODATA"}
{"seq":8,"event":"command","name":"RESOURCE_ATTACH_BACKING","response_name":"OK_NODATA"}
{"seq":9,"event":"command","name":"CTX_CREATE","response_name":"OK_NODATA","context_init":1}
{"seq":10,"event":"command","name":"SUBMIT_3D","response_name":"OK_NODATA","fenced":true,"submit_size":16}
{"seq":11,"event":"fence_create","ctx_id":1,"ring_idx":0,"fence_id":9,"backend_accepted":true,"outcome":"parked"}
{"seq":12,"event":"fence_deliver","ctx_id":1,"ring_idx":0,"fence_id":9,"used_len":24}
"#,
        )
        .unwrap();

        let report = analyze_virtio_gpu_trace(&path).unwrap();
        let _ = fs::remove_file(path);

        assert!(report.capset_info_ok);
        assert!(report.capset_ok);
        assert!(report.ctx_create_ok);
        assert!(!report.venus_capset_info_ok);
        assert!(!report.venus_capset_ok);
        assert!(!report.venus_ctx_create_ok);
        assert!(report.virgl_capset_info_ok);
        assert!(report.virgl_capset_ok);
        assert!(report.virgl_ctx_create_ok);
        assert!(report.resource_create_3d_ok);
        assert!(report.resource_attach_backing_ok);
        assert!(!report.blob_create_ok);
        assert!(report
            .p3_blockers(VirtioGpuTraceProtocolChoice::Auto)
            .is_empty());
        assert!(report
            .p3_blockers(VirtioGpuTraceProtocolChoice::Virgl)
            .is_empty());

        let venus_blockers = report.p3_blockers(VirtioGpuTraceProtocolChoice::Venus);
        assert!(venus_blockers
            .iter()
            .any(|blocker| blocker == "GET_CAPSET_INFO did not report VENUS capset id 4"));
        assert!(venus_blockers
            .iter()
            .any(|blocker| blocker == "missing successful GET_CAPSET for VENUS capset id 4"));
        assert!(venus_blockers
            .iter()
            .any(|blocker| { blocker == "missing CTX_CREATE with VENUS context_init low byte 4" }));
    }

    fn complete_virtio_gpu_trace_sample() -> &'static str {
        r#"{"seq":1,"event":"device_init","width":1280,"height":720,"backend_3d":true}
{"seq":2,"event":"common_read","field":"device_features","device_features_sel":0,"value":27}
{"seq":3,"event":"common_read","field":"device_features","device_features_sel":1,"value":1}
{"seq":4,"event":"driver_features","select":0,"accepted":25}
{"seq":5,"event":"driver_features","select":1,"accepted":1}
{"seq":6,"event":"queue_notify","queue":0,"valid":true}
{"seq":7,"event":"command","name":"GET_CAPSET_INFO","response_name":"OK_CAPSET_INFO","response_capset_id":4,"response_capset_max_version":1,"response_capset_max_size":64}
{"seq":8,"event":"command","name":"GET_CAPSET","response_name":"OK_CAPSET","capset_id":4,"capset_version":1}
{"seq":9,"event":"command","name":"RESOURCE_CREATE_BLOB","response_name":"OK_NODATA"}
{"seq":10,"event":"command","name":"CTX_CREATE","response_name":"OK_NODATA","context_init":4}
{"seq":11,"event":"command","name":"SUBMIT_3D","response_name":"OK_NODATA","fenced":true,"submit_size":16}
{"seq":12,"event":"fence_create","ctx_id":1,"ring_idx":0,"fence_id":9,"backend_accepted":true,"outcome":"parked"}
{"seq":13,"event":"fence_deliver","ctx_id":1,"ring_idx":0,"fence_id":9,"used_len":24}
"#
    }

    #[test]
    fn hvf_machine_plan_cli_accepts_installer_resources() {
        let cli = Cli::try_parse_from([
            "bridgevm",
            "hvf",
            "machine-plan",
            "--installer",
            "ISO/Win11_25H2_English_Arm64_v2.iso",
            "--memory-gib",
            "8",
            "--vcpus",
            "6",
        ])
        .unwrap();

        let Command::Hvf(HvfCommand::MachinePlan(args)) = cli.command else {
            panic!("expected hvf machine-plan command");
        };

        assert_eq!(
            args.installer.as_deref(),
            Some(Path::new("ISO/Win11_25H2_English_Arm64_v2.iso"))
        );
        assert_eq!(args.memory_gib, 8);
        assert_eq!(args.vcpus, 6);
    }

    #[test]
    fn hvf_windows_boot_disk_layout_probe_cli_accepts_disk_size_and_create() {
        let cli = Cli::try_parse_from([
            "bridgevm",
            "hvf",
            "windows-boot-disk-layout-probe",
            "--disk",
            "/tmp/win11-arm-hvf.raw",
            "--size-gib",
            "8",
            "--create",
        ])
        .unwrap();

        let Command::Hvf(HvfCommand::WindowsBootDiskLayoutProbe(args)) = cli.command else {
            panic!("expected hvf windows-boot-disk-layout-probe command");
        };

        assert_eq!(args.disk, PathBuf::from("/tmp/win11-arm-hvf.raw"));
        assert_eq!(args.size_gib, 8);
        assert!(args.create);
    }

    #[test]
    fn hvf_windows_firmware_handoff_probe_cli_accepts_firmware_vars_and_create_vars() {
        let cli = Cli::try_parse_from([
            "bridgevm",
            "hvf",
            "windows-firmware-handoff-probe",
            "--firmware",
            "/tmp/AAVMF_CODE.fd",
            "--vars-template",
            "/tmp/AAVMF_VARS.fd",
            "--vars",
            "/tmp/win11-arm-vars.fd",
            "--create-vars",
        ])
        .unwrap();

        let Command::Hvf(HvfCommand::WindowsFirmwareHandoffProbe(args)) = cli.command else {
            panic!("expected hvf windows-firmware-handoff-probe command");
        };

        assert_eq!(args.firmware, PathBuf::from("/tmp/AAVMF_CODE.fd"));
        assert_eq!(
            args.vars_template,
            Some(PathBuf::from("/tmp/AAVMF_VARS.fd"))
        );
        assert_eq!(args.vars, Some(PathBuf::from("/tmp/win11-arm-vars.fd")));
        assert!(args.create_vars);
    }

    #[test]
    fn hvf_windows_pflash_map_probe_cli_accepts_firmware_vars_and_create_vars() {
        let cli = Cli::try_parse_from([
            "bridgevm",
            "hvf",
            "windows-pflash-map-probe",
            "--firmware",
            "/tmp/AAVMF_CODE.fd",
            "--vars-template",
            "/tmp/AAVMF_VARS.fd",
            "--vars",
            "/tmp/win11-arm-vars.fd",
            "--create-vars",
        ])
        .unwrap();

        let Command::Hvf(HvfCommand::WindowsPflashMapProbe(args)) = cli.command else {
            panic!("expected hvf windows-pflash-map-probe command");
        };

        assert_eq!(args.firmware, PathBuf::from("/tmp/AAVMF_CODE.fd"));
        assert_eq!(
            args.vars_template,
            Some(PathBuf::from("/tmp/AAVMF_VARS.fd"))
        );
        assert_eq!(args.vars, Some(PathBuf::from("/tmp/win11-arm-vars.fd")));
        assert!(args.create_vars);
    }

    #[test]
    fn hvf_windows_pflash_hvf_map_probe_cli_accepts_firmware_vars_create_vars_and_allow_map() {
        let cli = Cli::try_parse_from([
            "bridgevm",
            "hvf",
            "windows-pflash-hvf-map-probe",
            "--firmware",
            "/tmp/AAVMF_CODE.fd",
            "--vars-template",
            "/tmp/AAVMF_VARS.fd",
            "--vars",
            "/tmp/win11-arm-vars.fd",
            "--create-vars",
            "--allow-map",
        ])
        .unwrap();

        let Command::Hvf(HvfCommand::WindowsPflashHvfMapProbe(args)) = cli.command else {
            panic!("expected hvf windows-pflash-hvf-map-probe command");
        };

        assert_eq!(args.firmware, PathBuf::from("/tmp/AAVMF_CODE.fd"));
        assert_eq!(
            args.vars_template,
            Some(PathBuf::from("/tmp/AAVMF_VARS.fd"))
        );
        assert_eq!(args.vars, Some(PathBuf::from("/tmp/win11-arm-vars.fd")));
        assert!(args.create_vars);
        assert!(args.allow_map);
    }

    #[test]
    fn hvf_windows_reset_vector_entry_probe_cli_accepts_firmware_vars_create_vars_and_allow_entry()
    {
        let cli = Cli::try_parse_from([
            "bridgevm",
            "hvf",
            "windows-reset-vector-entry-probe",
            "--firmware",
            "/tmp/AAVMF_CODE.fd",
            "--vars-template",
            "/tmp/AAVMF_VARS.fd",
            "--vars",
            "/tmp/win11-arm-vars.fd",
            "--create-vars",
            "--allow-entry",
        ])
        .unwrap();

        let Command::Hvf(HvfCommand::WindowsResetVectorEntryProbe(args)) = cli.command else {
            panic!("expected hvf windows-reset-vector-entry-probe command");
        };

        assert_eq!(args.firmware, PathBuf::from("/tmp/AAVMF_CODE.fd"));
        assert_eq!(
            args.vars_template,
            Some(PathBuf::from("/tmp/AAVMF_VARS.fd"))
        );
        assert_eq!(args.vars, Some(PathBuf::from("/tmp/win11-arm-vars.fd")));
        assert!(args.create_vars);
        assert!(args.allow_entry);
    }

    #[test]
    fn hvf_windows_firmware_run_loop_probe_cli_accepts_firmware_vars_create_vars_and_loop_bounds() {
        let cli = Cli::try_parse_from([
            "bridgevm",
            "hvf",
            "windows-firmware-run-loop-probe",
            "--firmware",
            "/tmp/AAVMF_CODE.fd",
            "--vars-template",
            "/tmp/AAVMF_VARS.fd",
            "--vars",
            "/tmp/win11-arm-vars.fd",
            "--create-vars",
            "--allow-loop",
            "--max-exits",
            "12",
            "--guest-ram-mib",
            "128",
            "--watchdog-ms",
            "250",
            "--map-low-pflash-alias",
            "--seed-diagnostic-vector",
            "--seed-guest-ram-diagnostic-vector",
            "--seed-executable-diagnostic-vector",
            "--try-recommended-vector-base-vbar",
            "--continue-after-recommended-vector-base-vbar",
            "--repair-low-vector-diagnostic-page",
            "--remap-low-vector-to-recommended-vector",
            "--continue-after-low-vector-repair",
            "--restore-low-vector-slot-before-eret",
            "--wire-interrupt-timer",
            "--iso",
            "/tmp/Win11_Arm64.iso",
            "--writable-disk",
            "/tmp/windows-arm.raw",
        ])
        .unwrap();

        let Command::Hvf(HvfCommand::WindowsFirmwareRunLoopProbe(args)) = cli.command else {
            panic!("expected hvf windows-firmware-run-loop-probe command");
        };

        assert_eq!(args.firmware, PathBuf::from("/tmp/AAVMF_CODE.fd"));
        assert_eq!(
            args.vars_template,
            Some(PathBuf::from("/tmp/AAVMF_VARS.fd"))
        );
        assert_eq!(args.vars, Some(PathBuf::from("/tmp/win11-arm-vars.fd")));
        assert!(args.create_vars);
        assert!(args.allow_loop);
        assert_eq!(args.max_exits, 12);
        assert_eq!(args.guest_ram_mib, 128);
        assert_eq!(args.watchdog_ms, 250);
        assert!(args.map_low_pflash_alias);
        assert!(args.seed_diagnostic_vector);
        assert!(args.seed_guest_ram_diagnostic_vector);
        assert!(args.seed_executable_diagnostic_vector);
        assert!(args.try_recommended_vector_base_vbar);
        assert!(args.continue_after_recommended_vector_base_vbar);
        assert!(args.repair_low_vector_diagnostic_page);
        assert!(args.remap_low_vector_to_recommended_vector);
        assert!(args.continue_after_low_vector_repair);
        assert!(args.restore_low_vector_slot_before_eret);
        assert!(args.wire_interrupt_timer);
        assert_eq!(args.iso, Some(PathBuf::from("/tmp/Win11_Arm64.iso")));
        assert_eq!(
            args.writable_disk,
            Some(PathBuf::from("/tmp/windows-arm.raw"))
        );
    }

    #[test]
    fn hvf_windows_firmware_device_discovery_probe_cli_accepts_firmware_media_and_loop_flags() {
        let cli = Cli::try_parse_from([
            "bridgevm",
            "hvf",
            "windows-firmware-device-discovery-probe",
            "--firmware",
            "/tmp/AAVMF_CODE.fd",
            "--vars-template",
            "/tmp/AAVMF_VARS.fd",
            "--vars",
            "/tmp/win11-arm-vars.fd",
            "--create-vars",
            "--allow-loop",
            "--max-exits",
            "16",
            "--guest-ram-mib",
            "128",
            "--watchdog-ms",
            "250",
            "--map-low-pflash-alias",
            "--repair-low-vector-diagnostic-page",
            "--continue-after-low-vector-repair",
            "--wire-interrupt-timer",
            "--iso",
            "/tmp/Win11_Arm64.iso",
            "--writable-disk",
            "/tmp/windows-arm.raw",
        ])
        .unwrap();

        let Command::Hvf(HvfCommand::WindowsFirmwareDeviceDiscoveryProbe(args)) = cli.command
        else {
            panic!("expected hvf windows-firmware-device-discovery-probe command");
        };

        assert_eq!(args.firmware, PathBuf::from("/tmp/AAVMF_CODE.fd"));
        assert_eq!(
            args.vars_template,
            Some(PathBuf::from("/tmp/AAVMF_VARS.fd"))
        );
        assert_eq!(args.vars, Some(PathBuf::from("/tmp/win11-arm-vars.fd")));
        assert!(args.create_vars);
        assert!(args.allow_loop);
        assert_eq!(args.max_exits, 16);
        assert_eq!(args.guest_ram_mib, 128);
        assert_eq!(args.watchdog_ms, 250);
        assert!(args.map_low_pflash_alias);
        assert!(args.repair_low_vector_diagnostic_page);
        assert!(args.continue_after_low_vector_repair);
        assert!(args.wire_interrupt_timer);
        assert_eq!(args.iso, Some(PathBuf::from("/tmp/Win11_Arm64.iso")));
        assert_eq!(
            args.writable_disk,
            Some(PathBuf::from("/tmp/windows-arm.raw"))
        );
    }

    #[test]
    fn hvf_windows_platform_description_probe_cli_accepts_memory_and_vcpus() {
        let cli = Cli::try_parse_from([
            "bridgevm",
            "hvf",
            "windows-platform-description-probe",
            "--memory-gib",
            "8",
            "--vcpus",
            "6",
        ])
        .unwrap();

        let Command::Hvf(HvfCommand::WindowsPlatformDescriptionProbe(args)) = cli.command else {
            panic!("expected hvf windows-platform-description-probe command");
        };

        assert_eq!(args.memory_gib, 8);
        assert_eq!(args.vcpus, 6);
    }

    #[test]
    fn hvf_windows_xhci_hid_boot_key_probe_cli_parses() {
        let cli =
            Cli::try_parse_from(["bridgevm", "hvf", "windows-xhci-hid-boot-key-probe"]).unwrap();

        let Command::Hvf(HvfCommand::WindowsXhciHidBootKeyProbe) = cli.command else {
            panic!("expected hvf windows-xhci-hid-boot-key-probe command");
        };
    }

    #[test]
    fn windows_hvf_machine_plan_render_is_blocked_and_qemu_free() {
        let plan = plan_windows_11_arm_hvf_machine(HvfMachinePlanOptions {
            installer: Some(PathBuf::from("ISO/Win11_25H2_English_Arm64_v2.iso")),
            memory_gib: 6,
            vcpu_count: 4,
        });
        let output = plan.render_text();

        assert!(output.contains("Windows 11 Arm HVF machine plan"));
        assert!(output.contains("QEMU: not used"));
        assert!(output.contains("Memory: 6 GiB"));
        assert!(output.contains("vCPU lifecycle:"));
        assert!(output.contains("Devices:"));
        assert!(output.contains("firmware UART and RTC skeletons"));
        assert!(output.contains("read-only installer media"));
        assert!(output.contains("system boot disk"));
        assert!(output.contains("Overall: blocked"));
        assert!(!output.contains("qemu-system"));
        assert!(!output.contains('%'));
    }

    #[test]
    fn display_cli_accepts_optional_display_size() {
        let cli = Cli::try_parse_from([
            "bridgevm", "display", "dev", "--width", "1440", "--height", "900",
        ])
        .unwrap();
        let Command::Display(args) = cli.command else {
            panic!("expected display command");
        };

        assert_eq!(args.name, "dev");
        assert_eq!(args.display_size().unwrap(), Some((1440, 900)));
    }

    #[test]
    fn display_cli_requires_width_and_height_together() {
        let cli = Cli::try_parse_from(["bridgevm", "display", "dev", "--width", "1440"]).unwrap();
        let Command::Display(args) = cli.command else {
            panic!("expected display command");
        };

        let error = args.display_size().expect_err("missing height must fail");
        assert!(error
            .to_string()
            .contains("--width and --height must be provided together"));
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
    fn parallels_class_progress_tracks_honest_product_scope() {
        let tracks = parallels_class_progress();

        assert!(tracks.contains(&ProductTrackProgress {
            status: ProductTrackStatus::Partial,
            name: "macOS-native integration / Coherence",
            implemented:
                "clipboard/display resize foundations plus preserved Linux .desktop/gio/gtk-launch/wmctrl live GUI proof and crop/proxy plumbing",
            next: "drive real guest-window crops from real framebuffer/proxy sessions, then move toward compositor-grade host-window integration",
        }));
        assert!(tracks.contains(&ProductTrackProgress {
            status: ProductTrackStatus::Proven,
            name: "Apple Silicon Fast Mode",
            implemented:
                "Apple Virtualization.framework path with live Linux Arm64 boot/suspend/resume and VZVirtualMachineView display",
            next: "broaden boot shapes and keep app/daemon/helper IPC tight",
        }));
        assert!(tracks.contains(&ProductTrackProgress {
            status: ProductTrackStatus::Partial,
            name: "intelligent resources / battery",
            implemented:
                "power-aware launch policy, display pacing consumption, and runtime policy IPC",
            next: "live Apple VZ CPU/RAM control must apply the policy to a running VM",
        }));
        assert!(tracks.contains(&ProductTrackProgress {
            status: ProductTrackStatus::Planned,
            name: "graphics acceleration / Metal",
            implemented: "native VZ GUI pixels are proven in an AppKit display window",
            next: "Metal compositor/frame pacing first; Direct3D-to-Metal or WDDM remains long-term R&D",
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
    fn resources_reapply_cli_builds_typed_request() {
        let cli = Cli::try_parse_from([
            "bridgevm",
            "resources",
            "reapply",
            "dev",
            "--visibility",
            "background",
        ])
        .unwrap();
        let request = request_for(cli.command).unwrap();
        assert_eq!(
            request,
            BridgeVmRequest::ReapplyRuntimeResources {
                name: "dev".to_string(),
                visibility: RuntimeResourceVisibility::Background,
            }
        );
    }

    #[test]
    fn runtime_control_cli_builds_typed_requests() {
        let cli = Cli::try_parse_from(["bridgevm", "runtime-control", "status", "dev"]).unwrap();
        let request = request_for(cli.command).unwrap();
        assert_eq!(
            request,
            BridgeVmRequest::RuntimeControl {
                name: "dev".to_string(),
                command: "status".to_string(),
            }
        );

        let cli = Cli::try_parse_from(["bridgevm", "runtime-control", "stop", "dev"]).unwrap();
        let request = request_for(cli.command).unwrap();
        assert_eq!(
            request,
            BridgeVmRequest::RuntimeControl {
                name: "dev".to_string(),
                command: "stop".to_string(),
            }
        );

        let cli = Cli::try_parse_from(["bridgevm", "runtime-control", "policy", "dev"]).unwrap();
        let request = request_for(cli.command).unwrap();
        assert_eq!(
            request,
            BridgeVmRequest::RuntimeControl {
                name: "dev".to_string(),
                command: "policy".to_string(),
            }
        );

        let cli = Cli::try_parse_from(["bridgevm", "runtime-control", "pacing", "dev"]).unwrap();
        let request = request_for(cli.command).unwrap();
        assert_eq!(
            request,
            BridgeVmRequest::RuntimeControl {
                name: "dev".to_string(),
                command: "pacing".to_string(),
            }
        );

        let cli = Cli::try_parse_from([
            "bridgevm",
            "runtime-control",
            "reapply",
            "dev",
            "--visibility",
            "background",
        ])
        .unwrap();
        let request = request_for(cli.command).unwrap();
        assert_eq!(
            request,
            BridgeVmRequest::ReapplyRuntimeResources {
                name: "dev".to_string(),
                visibility: RuntimeResourceVisibility::Background,
            }
        );
    }

    #[test]
    fn runtime_control_status_uses_recorded_socket_metadata() {
        let store = unique_store("bridgevm-cli-runtime-control-test");
        store.create_vm(&test_manifest("dev")).unwrap();
        let socket_path = unique_socket_path("bridgevm-cli-rc");
        let listener = std::os::unix::net::UnixListener::bind(&socket_path).unwrap();
        let server = std::thread::spawn(move || {
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
                    br#"{"display":{"height":768,"width":1024},"ok":true,"state":"running","stopping":false,"supported_commands":["status","stop","policy","pacing"],"vm":"dev"}"#,
                )
                .unwrap();
            stream.write_all(b"\n").unwrap();
        });

        let metadata = bridgevm_storage::RunnerMetadata {
            engine: "lightvm".to_string(),
            pid: Some(42),
            command: vec!["lightvm-runner".to_string()],
            log_path: PathBuf::from("lightvm.log"),
            started_at_unix: 1,
            dry_run: false,
            launch_spec_path: None,
            guest_tools: None,
            disk: None,
            active_disk: None,
            launch_readiness: None,
            runtime_control: Some(bridgevm_storage::RuntimeControlMetadata {
                kind: "apple-vz-display".to_string(),
                socket_path: socket_path.clone(),
                commands: vec![
                    "status".to_string(),
                    "stop".to_string(),
                    "policy".to_string(),
                    "pacing".to_string(),
                ],
            }),
        };
        store.write_runner_metadata("dev", &metadata).unwrap();

        run_runtime_control_command(&store, "dev", "status").unwrap();
        server.join().unwrap();
        let _ = fs::remove_file(socket_path);
    }

    #[test]
    fn daemon_client_rejects_oversized_response() {
        let socket_path = unique_socket_path("bridgevm-cli-oversized-response");
        let listener = std::os::unix::net::UnixListener::bind(&socket_path).unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = String::new();
            BufReader::new(stream.try_clone().unwrap())
                .read_line(&mut request)
                .unwrap();
            let oversized = vec![b'x'; MAX_DAEMON_RESPONSE_BYTES as usize + 1];
            let _ = stream.write_all(&oversized);
        });

        let error = send_request(&socket_path, BridgeVmRequest::Doctor).unwrap_err();
        assert!(error.to_string().contains("exceeded 16777216 bytes"));
        server.join().unwrap();
        let _ = fs::remove_file(socket_path);
    }

    #[test]
    fn daemon_client_rejects_incomplete_response_frame() {
        let socket_path = unique_socket_path("bridgevm-cli-incomplete-response");
        let listener = std::os::unix::net::UnixListener::bind(&socket_path).unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = String::new();
            BufReader::new(stream.try_clone().unwrap())
                .read_line(&mut request)
                .unwrap();
            stream.write_all(b"{}").unwrap();
        });

        let error = send_request(&socket_path, BridgeVmRequest::Doctor).unwrap_err();
        assert!(error.to_string().contains("incomplete response frame"));
        server.join().unwrap();
        let _ = fs::remove_file(socket_path);
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

        let bounds = Cli::try_parse_from([
            "bridgevm",
            "guest-tools",
            "set-window-bounds",
            "dev",
            "--id",
            "window-terminal",
            "--x",
            "30",
            "--y",
            "40",
            "--width",
            "800",
            "--height",
            "600",
            "--request-id",
            "bounds-1",
        ])
        .unwrap();
        let request = request_for(bounds.command).unwrap();
        let BridgeVmRequest::GuestToolsSendCommand { name, envelope } = request else {
            panic!("expected guest tools send command request");
        };

        assert_eq!(name, "dev");
        assert_eq!(envelope.request_id.as_deref(), Some("bounds-1"));
        assert_eq!(
            envelope.message,
            AgentMessage::SetWindowBounds {
                id: "window-terminal".to_string(),
                x: 30,
                y: 40,
                width: 800,
                height: 600,
            }
        );

        let pointer = Cli::try_parse_from([
            "bridgevm",
            "guest-tools",
            "window-pointer",
            "dev",
            "--id",
            "window-terminal",
            "--x",
            "120",
            "--y",
            "240",
            "--action",
            "click",
            "--button",
            "left",
            "--request-id",
            "pointer-1",
        ])
        .unwrap();
        let request = request_for(pointer.command).unwrap();
        let BridgeVmRequest::GuestToolsSendCommand { name, envelope } = request else {
            panic!("expected guest tools send command request");
        };

        assert_eq!(name, "dev");
        assert_eq!(envelope.request_id.as_deref(), Some("pointer-1"));
        assert_eq!(
            envelope.message,
            AgentMessage::WindowInput {
                id: "window-terminal".to_string(),
                event: WindowInputEvent::Pointer {
                    x: 120,
                    y: 240,
                    action: "click".to_string(),
                    button: Some("left".to_string()),
                },
            }
        );

        let key = Cli::try_parse_from([
            "bridgevm",
            "guest-tools",
            "window-key",
            "dev",
            "--id",
            "window-terminal",
            "--key",
            "Return",
            "--action",
            "tap",
            "--request-id",
            "key-1",
        ])
        .unwrap();
        let request = request_for(key.command).unwrap();
        let BridgeVmRequest::GuestToolsSendCommand { name, envelope } = request else {
            panic!("expected guest tools send command request");
        };

        assert_eq!(name, "dev");
        assert_eq!(envelope.request_id.as_deref(), Some("key-1"));
        assert_eq!(
            envelope.message,
            AgentMessage::WindowInput {
                id: "window-terminal".to_string(),
                event: WindowInputEvent::Key {
                    key: "Return".to_string(),
                    action: "tap".to_string(),
                },
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
        manifest.network.mode = "advanced".to_string();
        store.create_vm(&manifest).unwrap();

        let error = build_runner_metadata(&store, "legacy", false).unwrap_err();
        let message = format!("{error:#}");

        assert!(
            message.contains("failed to build Compatibility Mode QEMU command"),
            "missing CLI context: {message}"
        );
        assert!(
            message.contains("QEMU launch blocker qemu-advanced-network-requires-schema"),
            "missing QEMU blocker: {message}"
        );
        assert!(
            message.contains("requirement: Compatibility Mode QEMU requires an advanced network schema and launcher wiring before launch"),
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
            message.contains("Fast Mode spawn requires BRIDGEVM_APPLE_VZ_RUNNER"),
            "{message}"
        );
        assert!(message.contains("launch blockers:"), "{message}");
        assert!(message.contains("missing-primary-disk"), "{message}");
        assert!(message.contains("apple-vz-runner-unavailable"), "{message}");
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
    fn daemon_error_output_preserves_qemu_network_blocker_requirement() {
        let error = print_daemon_response(BridgeVmResponse::Error {
            message: "failed to build Compatibility Mode QEMU command: QEMU launch blocker qemu-advanced-network-requires-schema: advanced networking requires an advanced Compatibility Mode QEMU schema before args can be generated; requirement: Compatibility Mode QEMU requires an advanced network schema and launcher wiring before launch".to_string(),
        })
        .unwrap_err();
        let message = format!("{error:#}");

        assert!(
            message.contains("QEMU launch blocker qemu-advanced-network-requires-schema"),
            "missing QEMU blocker: {message}"
        );
        assert!(
            message.contains("requirement: Compatibility Mode QEMU requires an advanced network schema and launcher wiring before launch"),
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
