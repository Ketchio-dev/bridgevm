//! Split out of args.rs by responsibility.

use crate::*;

#[derive(Debug, Parser)]
#[command(name = "bridgevm", about = "BridgeVM developer CLI")]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Command,
    #[arg(long, global = true, value_name = "PATH")]
    pub(crate) store: Option<PathBuf>,
    #[arg(long, global = true, value_name = "SOCKET")]
    pub(crate) socket: Option<PathBuf>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
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
pub(crate) enum HvfCommand {
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
    /// Validate one or more real-title logs against versioned manifests and a GPU trace.
    TitleGateReport(HvfTitleGateReportArgs),
}

#[derive(Debug, Parser)]
pub(crate) struct WindowsHvfPlanArgs {
    #[arg(long, value_name = "PATH")]
    pub(crate) installer: Option<PathBuf>,
}

#[derive(Debug, Parser)]
pub(crate) struct WindowsHvfMachinePlanArgs {
    #[arg(long, value_name = "PATH")]
    pub(crate) installer: Option<PathBuf>,

    #[arg(long, default_value_t = 6)]
    pub(crate) memory_gib: u32,
    #[arg(long, default_value_t = 4)]
    pub(crate) vcpus: u8,
}

#[derive(Debug, Parser)]
pub(crate) struct WindowsHvfBootDiskLayoutProbeArgs {
    #[arg(long, value_name = "PATH")]
    pub(crate) disk: PathBuf,
    #[arg(long, default_value_t = WINDOWS_ARM_BOOT_DISK_DEFAULT_SIZE_GIB)]
    pub(crate) size_gib: u32,
    #[arg(long)]
    pub(crate) create: bool,
}

#[derive(Debug, Parser)]
pub(crate) struct WindowsHvfFirmwareHandoffProbeArgs {
    #[arg(long, value_name = "PATH")]
    pub(crate) firmware: PathBuf,
    #[arg(long, value_name = "PATH")]
    pub(crate) vars_template: Option<PathBuf>,
    #[arg(long, value_name = "PATH")]
    pub(crate) vars: Option<PathBuf>,
    #[arg(long)]
    pub(crate) create_vars: bool,
}

#[derive(Debug, Parser)]
pub(crate) struct WindowsHvfPflashMapProbeArgs {
    #[arg(long, value_name = "PATH")]
    pub(crate) firmware: PathBuf,
    #[arg(long, value_name = "PATH")]
    pub(crate) vars_template: Option<PathBuf>,
    #[arg(long, value_name = "PATH")]
    pub(crate) vars: Option<PathBuf>,
    #[arg(long)]
    pub(crate) create_vars: bool,
}

#[derive(Debug, Parser)]
pub(crate) struct WindowsHvfPflashHvfMapProbeArgs {
    #[arg(long, value_name = "PATH")]
    pub(crate) firmware: PathBuf,
    #[arg(long, value_name = "PATH")]
    pub(crate) vars_template: Option<PathBuf>,
    #[arg(long, value_name = "PATH")]
    pub(crate) vars: Option<PathBuf>,
    #[arg(long)]
    pub(crate) create_vars: bool,
    #[arg(long)]
    pub(crate) allow_map: bool,
}

#[derive(Debug, Parser)]
pub(crate) struct WindowsHvfResetVectorEntryProbeArgs {
    #[arg(long, value_name = "PATH")]
    pub(crate) firmware: PathBuf,
    #[arg(long, value_name = "PATH")]
    pub(crate) vars_template: Option<PathBuf>,
    #[arg(long, value_name = "PATH")]
    pub(crate) vars: Option<PathBuf>,
    #[arg(long)]
    pub(crate) create_vars: bool,
    #[arg(long)]
    pub(crate) allow_entry: bool,
}

#[derive(Debug, Parser)]
pub(crate) struct WindowsHvfFirmwareRunLoopProbeArgs {
    #[arg(long, value_name = "PATH")]
    pub(crate) firmware: PathBuf,
    #[arg(long, value_name = "PATH")]
    pub(crate) vars_template: Option<PathBuf>,
    #[arg(long, value_name = "PATH")]
    pub(crate) vars: Option<PathBuf>,
    #[arg(long)]
    pub(crate) create_vars: bool,
    #[arg(long)]
    pub(crate) allow_loop: bool,
    #[arg(long, default_value_t = 8)]
    pub(crate) max_exits: u32,
    #[arg(long, default_value_t = 64)]
    pub(crate) guest_ram_mib: u32,
    #[arg(long, default_value_t = 100)]
    pub(crate) watchdog_ms: u64,
    #[arg(long)]
    pub(crate) map_low_pflash_alias: bool,
    #[arg(long)]
    pub(crate) seed_diagnostic_vector: bool,
    #[arg(long)]
    pub(crate) seed_guest_ram_diagnostic_vector: bool,
    #[arg(long)]
    pub(crate) seed_executable_diagnostic_vector: bool,
    #[arg(long)]
    pub(crate) try_recommended_vector_base_vbar: bool,
    #[arg(long)]
    pub(crate) continue_after_recommended_vector_base_vbar: bool,
    #[arg(long)]
    pub(crate) repair_low_vector_diagnostic_page: bool,
    #[arg(long)]
    pub(crate) remap_low_vector_to_recommended_vector: bool,
    #[arg(long)]
    pub(crate) continue_after_low_vector_repair: bool,
    #[arg(long)]
    pub(crate) restore_low_vector_slot_before_eret: bool,
    #[arg(long)]
    pub(crate) wire_interrupt_timer: bool,
    #[arg(long, value_name = "PATH")]
    pub(crate) iso: Option<PathBuf>,
    #[arg(long, value_name = "PATH")]
    pub(crate) writable_disk: Option<PathBuf>,
}

#[derive(Debug, Parser)]
pub(crate) struct WindowsHvfPlatformDescriptionProbeArgs {
    #[arg(long, default_value_t = 6)]
    pub(crate) memory_gib: u32,
    #[arg(long, default_value_t = 4)]
    pub(crate) vcpus: u8,
}

#[derive(Debug, Parser)]
pub(crate) struct HvfVmProbeArgs {
    #[arg(long)]
    pub(crate) allow_create: bool,
}

#[derive(Debug, Parser)]
pub(crate) struct HvfVcpuRunProbeArgs {
    #[arg(long)]
    pub(crate) allow_run: bool,
}
