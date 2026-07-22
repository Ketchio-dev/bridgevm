// allow: SIZE_OK - Task 5q virt platform wiring is a legacy monolithic surface carried to preserve validated HVF/PCIe evidence; full modular split is separate work.
//! `VirtPlatform` — the assembled Path A "QEMU virt" platform.
//!
//! This ties the three Path A bricks together into the object a live HVF run
//! loop drives:
//!
//! - [`crate::machine`] — the device memory map + IRQ map (single source of truth).
//! - [`crate::fwcfg`] — the `fw_cfg` keystone, populated with the guest device
//!   tree, ACPI tables, SMBIOS and boot order.
//! - [`crate::dtb`] — the QEMU-`virt`-shaped device tree handed to firmware.
//! - [`crate::nvme`] — the first PCIe storage endpoint behind BAR0.
//!
//! The live wiring is small and lives at the data-abort (MMIO) exit of
//! `hv_vcpu_run`: on a guest MMIO fault the run loop calls
//! [`VirtPlatform::on_mmio_with_post_drain`] with the fault address, access, and a
//! [`GuestMemoryMut`] view of guest RAM, then applies the [`MmioOutcome`].
//! Everything in this module is host-only and unit-testable; only the `hv_vcpu_run`
//! call itself needs an entitled,
//! code-signed Apple Silicon host (the step-6 Linux ACPI-only bring-up in
//! `docs/hvf-windows-engine-strategy.md`).

mod bootorder;

use std::{
    io,
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

use crate::acpi::{
    build_acpi_with_devices, AcpiDeviceConfig, ACPI_LOADER_FILE, ACPI_RSDP_FILE, ACPI_TABLE_FILE,
    ACPI_TPM_LOG_FILE,
};
use crate::dtb::{build_virt_fdt_with_devices, VirtFdtConfig, VirtFdtDeviceConfig};
use crate::fwcfg::{
    FwCfg, GuestMemoryMut, KEY_CMDLINE_DATA, KEY_CMDLINE_SIZE, KEY_INITRD_DATA, KEY_INITRD_SIZE,
    KEY_KERNEL_DATA, KEY_KERNEL_SIZE,
};
use crate::hda::{HdaController, HdaPcmSink};
use crate::machine::{self, Region};
use crate::msix::MsixMessage;
use crate::net_nat::{HostSocketOutboundIpv4Handler, NatBackend, NatStats};
use crate::nvme::{
    NvmeCommandTrace, NvmeCompletionEvent, NvmeController, REG_CC, REG_DOORBELL_BASE,
};
use crate::pcie::{
    CfgAddr, PcieEcam, PcieEcamConfig, PcieMmioTarget, PciePioTarget, HDA_BDF, NVME_BDF,
    VIRTIO_BLK_BDF, VIRTIO_CONSOLE_BDF, VIRTIO_GPU_BDF, VIRTIO_GPU_DEVICE_ID, VIRTIO_NET_BDF,
    XHCI_BDF,
};
use crate::pflash::P30NorFlash;
use crate::pl011::Pl011;
use crate::pl031::Pl031;
use crate::ramfb::{Ramfb, RamfbConfig, RAMFB_CONFIG_SIZE, RAMFB_FW_CFG_FILE};
use crate::smbios::{build_smbios, SMBIOS_ANCHOR_FILE, SMBIOS_TABLE_FILE};
use crate::tpm_ppi::{build_qemu_fw_cfg_tpm_config, TpmPpi, TpmPpiStats, TPM_PPI_FW_CFG_FILE};
use crate::tpm_tis::{Tpm2Backend, TpmTis, TpmTisStats};
use crate::virtio_blk::{
    VirtioBlockRequestTrace, VirtioMmioBlock, VirtioMmioBlockResult, VirtioMmioBlockStats,
    VirtioPciBlock, VirtioPciBlockOp, INSTALLER_ISO_SLOT,
};
use crate::virtio_console::{
    VirtioConsoleResult, VirtioConsoleStats, VirtioPciConsole, VirtioPciConsoleOp,
};
use crate::virtio_gpu::{
    VblankWakeState, VirtioGpuResult, VirtioGpuScanout, VirtioGpuStats, VirtioPciGpu,
    VirtioPciGpuOp,
};
use crate::virtio_gpu_3d::GpuShmMapPort;
use crate::virtio_net::{
    LoopbackTestBackend, NetBackend, VirtioNetResult, VirtioNetStats, VirtioPciNet, VirtioPciNetOp,
};
use crate::xhci::{
    PointerInputAction, SetupInputAction, XhciController, XhciEventLifecycleStats,
    XhciHidSemanticStats, XhciPointerInputQueueError, XhciPointerInputReportStats,
    XhciSetupInputQueueError, XhciSetupInputReportStats,
};

const DEFAULT_NVME_DISK_BYTES: usize = 16 * 1024 * 1024;
const HID_BOOT_KEYBOARD_USAGE_SPACE: u8 = 0x2c;
const MAX_XHCI_SETUP_INPUT_DRAIN_ATTEMPTS: usize = 16;
#[cfg(any(feature = "venus", test))]
const DEFAULT_VIRTIO_GPU_3D_SCANOUT_READBACK_MS: u64 = 16;
const DEFAULT_VIRTIO_GPU_VBLANK_HZ: u64 = 120;

fn make_virtio_gpu() -> VirtioPciGpu {
    let (width, height) = virtio_gpu_resolution_from_env();
    if env_flag("BRIDGEVM_VIRTIO_GPU_3D") {
        #[cfg(feature = "venus")]
        {
            let direct = env_flag("BRIDGEVM_VIRTIO_GPU_DIRECT_RENDERER");
            let backend = if direct {
                crate::venus_backend::VenusBackend::new().map(|backend| {
                    let protocol = backend.protocol();
                    (
                        protocol,
                        Box::new(backend) as Box<dyn crate::virtio_gpu_3d::VirtioGpu3dBackend>,
                    )
                })
            } else {
                crate::venus_backend::ThreadedVenusBackend::new().map(|backend| {
                    let protocol = backend.protocol();
                    (
                        protocol,
                        Box::new(backend) as Box<dyn crate::virtio_gpu_3d::VirtioGpu3dBackend>,
                    )
                })
            };
            match backend {
                Ok((protocol, backend)) => {
                    eprintln!(
                        "virtio-gpu: {} 3D backend enabled mode={}",
                        protocol.label(),
                        if direct { "direct-rebind" } else { "threaded" }
                    );
                    let mut gpu = VirtioPciGpu::with_3d_backend(width, height, backend);
                    let interval = virtio_gpu_3d_scanout_readback_interval_from_value(
                        std::env::var("BRIDGEVM_VIRTIO_GPU_SCANOUT_READBACK_MS")
                            .ok()
                            .as_deref(),
                    );
                    gpu.set_3d_scanout_readback_interval(interval);
                    if env_flag("BRIDGEVM_VIRTIO_GPU_ASYNC_SCANOUT") {
                        gpu.set_3d_scanout_deferred(true);
                        eprintln!("virtio-gpu: 3D scanout readback deferred off the flush path");
                    }
                    if env_flag("BRIDGEVM_VIRTIO_GPU_IOSURFACE_SCANOUT") {
                        gpu.set_3d_scanout_iosurface(
                            true,
                            env_flag("BRIDGEVM_VIRTIO_GPU_IOSURFACE_VERIFY"),
                        );
                        eprintln!("virtio-gpu: 3D scanout IOSurface GPU blit enabled");
                    }
                    configure_virtio_gpu_vblank(&mut gpu);
                    eprintln!(
                        "virtio-gpu: 3D scanout readback pacing={}ms",
                        interval.as_millis()
                    );
                    return gpu;
                }
                Err(error) => {
                    panic!("virtio-gpu: requested 3D backend failed to initialize: {error}");
                }
            }
        }
        #[cfg(not(feature = "venus"))]
        {
            panic!(
                "virtio-gpu: BRIDGEVM_VIRTIO_GPU_3D requested but this probe was built without the venus feature"
            );
        }
    }
    let mut gpu = VirtioPciGpu::new(width, height);
    configure_virtio_gpu_vblank(&mut gpu);
    gpu
}

fn configure_virtio_gpu_vblank(gpu: &mut VirtioPciGpu) {
    let value = std::env::var("BRIDGEVM_VBLANK_HZ").ok();
    let interval = virtio_gpu_vblank_interval_from_value(value.as_deref());
    gpu.set_vblank_interval(interval);
    if !interval.is_zero() {
        gpu.set_vblank_wake(Arc::new(VblankWakeState::new()));
        eprintln!(
            "virtio-gpu: host vblank pacing interval={}ns",
            interval.as_nanos()
        );
    }
}

fn virtio_gpu_vblank_interval_from_value(value: Option<&str>) -> Duration {
    let Some(value) = value else {
        return Duration::ZERO;
    };
    let hz = value
        .trim()
        .parse::<u64>()
        .unwrap_or(DEFAULT_VIRTIO_GPU_VBLANK_HZ);
    if hz == 0 {
        return Duration::ZERO;
    }
    Duration::from_nanos((1_000_000_000 / hz).max(1))
}

fn virtio_gpu_3d_enabled_for_pcie() -> bool {
    cfg!(feature = "venus") && env_flag("BRIDGEVM_VIRTIO_GPU_3D")
}

#[cfg(any(feature = "venus", test))]
fn virtio_gpu_3d_scanout_readback_interval_from_value(value: Option<&str>) -> Duration {
    let interval_ms = value
        .and_then(|value| value.trim().parse::<u64>().ok())
        .unwrap_or(DEFAULT_VIRTIO_GPU_3D_SCANOUT_READBACK_MS);
    Duration::from_millis(interval_ms)
}

fn virtio_gpu_resolution_from_env() -> (u32, u32) {
    let value = std::env::var("BRIDGEVM_VIRTIO_GPU_RES").unwrap_or_else(|_| "1280x800".into());
    let Some((width, height)) = value.trim().split_once('x').and_then(|(width, height)| {
        Some((width.parse::<u32>().ok()?, height.parse::<u32>().ok()?))
    }) else {
        panic!("BRIDGEVM_VIRTIO_GPU_RES must be WIDTHxHEIGHT, for example 1600x900");
    };
    assert!(
        width > 0 && height > 0,
        "virtio-gpu resolution must be non-zero"
    );
    (width, height)
}

fn env_flag(name: &str) -> bool {
    let Ok(value) = std::env::var(name) else {
        return false;
    };
    let value = value.trim();
    value == "1"
        || value.eq_ignore_ascii_case("true")
        || value.eq_ignore_ascii_case("yes")
        || value.eq_ignore_ascii_case("on")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VirtPlatformDeviceConfig {
    pub xhci_present: bool,
    pub hda_present: bool,
    pub virtio_boot_media_present: bool,
    pub virtio_net_present: bool,
    pub virtio_gpu_present: bool,
    pub virtio_console_present: bool,
    pub virtio_gpu_pci_device_id: u16,
    pub virtio_net_backend: VirtioNetBackendKind,
    pub legacy_virtio_mmio_present: bool,
    pub ramfb_present: bool,
    pub tpm_tis_present: bool,
}

impl Default for VirtPlatformDeviceConfig {
    fn default() -> Self {
        Self {
            xhci_present: true,
            hda_present: false,
            virtio_boot_media_present: true,
            virtio_net_present: false,
            virtio_gpu_present: false,
            virtio_console_present: false,
            virtio_gpu_pci_device_id: VIRTIO_GPU_DEVICE_ID,
            virtio_net_backend: VirtioNetBackendKind::Nat,
            legacy_virtio_mmio_present: true,
            ramfb_present: false,
            tpm_tis_present: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VirtioNetBackendKind {
    Nat,
    Loopback,
}

impl VirtioNetBackendKind {
    pub fn from_env_value(value: Option<&str>) -> Self {
        let Some(value) = value else {
            return Self::Nat;
        };
        let value = value.trim();
        if value.eq_ignore_ascii_case("nat") {
            Self::Nat
        } else if value.eq_ignore_ascii_case("loopback") {
            Self::Loopback
        } else {
            panic!("BRIDGEVM_VIRTIO_NET_BACKEND must be 'nat' or 'loopback'");
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VirtPlatformConfig {
    pub fdt: VirtFdtConfig,
    pub devices: VirtPlatformDeviceConfig,
}

impl VirtPlatformConfig {
    pub fn new(fdt: VirtFdtConfig) -> Self {
        Self {
            fdt,
            devices: VirtPlatformDeviceConfig::default(),
        }
    }

    pub fn with_ramfb(fdt: VirtFdtConfig) -> Self {
        let mut config = Self::new(fdt);
        config.devices.ramfb_present = true;
        config
    }
}

#[derive(Debug)]
enum PlatformNetBackend {
    Nat(Box<NatBackend<HostSocketOutboundIpv4Handler>>),
    Loopback(LoopbackTestBackend),
}

impl PlatformNetBackend {
    fn new(kind: VirtioNetBackendKind) -> Self {
        match kind {
            VirtioNetBackendKind::Nat => Self::Nat(Box::new(NatBackend::new_host_socket())),
            VirtioNetBackendKind::Loopback => Self::Loopback(LoopbackTestBackend::default()),
        }
    }

    fn nat_stats(&self) -> Option<NatStats> {
        match self {
            Self::Nat(backend) => Some(backend.stats()),
            Self::Loopback(_) => None,
        }
    }
}

impl NetBackend for PlatformNetBackend {
    fn transmit(&mut self, frame: &[u8]) {
        match self {
            Self::Nat(backend) => backend.transmit(frame),
            Self::Loopback(backend) => backend.transmit(frame),
        }
    }

    fn poll_receive(&mut self) -> Option<Vec<u8>> {
        match self {
            Self::Nat(backend) => backend.poll_receive(),
            Self::Loopback(backend) => backend.poll_receive(),
        }
    }

    fn poll_receive_into(&mut self, out: &mut Vec<u8>) -> bool {
        match self {
            Self::Nat(backend) => backend.poll_receive_into(out),
            Self::Loopback(backend) => backend.poll_receive_into(out),
        }
    }

    fn poll_host_sockets(&mut self) {
        if let Self::Nat(backend) = self {
            backend.poll_host_sockets();
        }
    }

    #[cfg(test)]
    fn test_transmitted_frames(&self) -> Option<&[Vec<u8>]> {
        match self {
            Self::Nat(_) => None,
            Self::Loopback(backend) => backend.test_transmitted_frames(),
        }
    }
}

fn make_virtio_net_backend(kind: VirtioNetBackendKind) -> PlatformNetBackend {
    PlatformNetBackend::new(kind)
}

/// A guest MMIO access as decoded from an HVF data-abort exit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MmioOp {
    Read { size: u8 },
    Write { size: u8, value: u64 },
}

/// `BRIDGEVM_TRACE_VENUS_START=1`: log every MMIO access that decodes to any
/// virtio-gpu BAR (MSI-X table BAR1, shm window BAR2, modern transport BAR4).
/// The venus KMD dies before its first BAR4 common-config write, so the first
/// BAR the KMD actually touches — and the last access before the reboot — is
/// the divergence evidence. First 256 accesses, then sampled.
fn venus_start_trace_gpu_bar_access(bar_index: usize, offset: u64, op: &MmioOp) {
    use std::sync::atomic::{AtomicU64, Ordering};
    if !crate::virtio_gpu_trace::venus_start_trace_enabled() {
        return;
    }
    static COUNT: AtomicU64 = AtomicU64::new(0);
    let n = COUNT.fetch_add(1, Ordering::Relaxed) + 1;
    if n <= 256 || n % 1024 == 0 {
        println!("venus-start: gpu bar{bar_index} off={offset:#x} {op:?} n={n}");
    }
}

/// Result of dispatching a guest MMIO access.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MmioOutcome {
    /// A read completed; this value is written back to the faulting register.
    ReadValue(u64),
    /// A write was accepted by a device.
    WriteAck,
    /// The address belongs to a modelled device that is not implemented yet.
    /// Carries the device name so bring-up traces are precise rather than a
    /// generic "unhandled MMIO".
    KnownUnimplemented(&'static str),
    /// The address belongs to no device in the machine map.
    Unmapped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MmioPostDrain {
    xhci_setup_input_attempted: bool,
}

impl MmioPostDrain {
    pub const NONE: Self = Self {
        xhci_setup_input_attempted: false,
    };

    pub const XHCI_SETUP_INPUT: Self = Self {
        xhci_setup_input_attempted: true,
    };

    pub fn xhci_setup_input_attempted(self) -> bool {
        self.xhci_setup_input_attempted
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct XhciHidBootKeyReportStats {
    pub queued_space_reports: u64,
    pub unsupported_usage_rejections: u64,
    pub busy_rejections: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XhciHidBootKeyQueueError {
    UnsupportedUsage { usage: u8 },
    Busy,
}

/// Where the firmware, device tree and RAM live in the guest address space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuestMemoryLayout {
    /// pflash bank 0 — firmware code (read-only).
    pub flash_code: Region,
    /// pflash bank 1 — UEFI variable store (writable).
    pub flash_vars: Region,
    /// System RAM.
    pub ram: Region,
    /// Address the flattened device tree is loaded at (inside RAM).
    pub dtb_load: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NvmePcieLiveness {
    pub nvme_advertised: bool,
    pub nvme_ecam_touched: bool,
    pub nvme_command_memory_enabled: bool,
    pub nvme_command_bus_master_enabled: bool,
    pub nvme_bar0_assigned: bool,
    pub nvme_mmio_reached: bool,
    pub nvme_cc_enabled: bool,
    pub nvme_admin_doorbell_rung: bool,
}

/// The assembled Path A platform.
#[derive(Debug)]
pub struct VirtPlatform {
    cfg: VirtFdtConfig,
    devices: VirtPlatformDeviceConfig,
    fw_cfg: FwCfg,
    uart: Pl011,
    rtc: Pl031,
    pcie: PcieEcam,
    nvme: NvmeController,
    xhci: XhciController,
    hda: Option<HdaController>,
    virtio_iso: Option<VirtioMmioBlock>,
    pci_boot_media: Option<VirtioPciBlock>,
    virtio_net: Option<VirtioPciNet<PlatformNetBackend>>,
    virtio_gpu: Option<VirtioPciGpu>,
    virtio_console: Option<VirtioPciConsole>,
    tpm_tis: Option<TpmTis>,
    tpm_ppi: Option<TpmPpi>,
    ramfb: Ramfb,
    flash_vars: P30NorFlash,
    pending_msix: Vec<MsixMessage>,
    pending_spi_levels: Vec<(u32, bool)>,
    nvme_completion_scratch: Vec<NvmeCompletionEvent>,
    xhci_hid_boot_key_report_stats: XhciHidBootKeyReportStats,
    nvme_ecam_touched: bool,
    nvme_mmio_reached: bool,
    nvme_cc_enabled: bool,
    nvme_admin_doorbell_rung: bool,
    dtb: Vec<u8>,
    // Minimum host-time spacing between consecutive HID interrupt-IN report
    // emissions. Windows drops keystrokes when many reports land microseconds
    // apart (the guest coalesces the interrupt-IN completions), so live runs
    // throttle emission; `Duration::ZERO` means no pacing (the default, so unit
    // tests drain a queued sequence in one call). The clock read stays in the
    // probe: it pushes `Instant::now()` in via `set_host_now`, and while
    // `host_now` is `None` (the unit-test default) the drain paths are unpaced.
    xhci_report_interval: Duration,
    host_now: Option<Instant>,
    xhci_dci3_last_emission: Option<Instant>,
    xhci_dci5_last_emission: Option<Instant>,
}

const _: fn() = || {
    fn assert_send<T: Send>() {}
    assert_send::<crate::platform_virt::VirtPlatform>();
};

impl VirtPlatform {
    /// Build the platform: generate the device tree from the machine map and
    /// stand up `fw_cfg` with its standard control entries and generated ACPI
    /// table-loader blobs.
    pub fn new(cfg: VirtFdtConfig) -> Self {
        Self::new_with_config(VirtPlatformConfig::new(cfg))
    }

    /// Build the platform with QEMU's `ramfb` fw_cfg file registered. QEMU only
    /// exposes this surface when a framebuffer device is requested, not for
    /// `-display none`, so the default constructor leaves it absent.
    pub fn new_with_ramfb(cfg: VirtFdtConfig) -> Self {
        Self::new_with_config(VirtPlatformConfig::with_ramfb(cfg))
    }

    pub fn new_with_config(config: VirtPlatformConfig) -> Self {
        Self::new_with_config_and_tpm_backend(config, None)
    }

    /// Build a platform with an optional TPM command backend. Presence is
    /// explicit in `config.devices.tpm_tis_present`; requiring it to match the
    /// backend prevents ACPI from advertising a device that cannot execute.
    pub fn new_with_config_and_tpm_backend(
        config: VirtPlatformConfig,
        tpm_backend: Option<Box<dyn Tpm2Backend>>,
    ) -> Self {
        assert_eq!(
            config.devices.tpm_tis_present,
            tpm_backend.is_some(),
            "TPM ACPI/MMIO presence must match a concrete TPM backend"
        );
        let dtb = build_virt_fdt_with_devices(
            &config.fdt,
            VirtFdtDeviceConfig {
                legacy_virtio_mmio_present: config.devices.legacy_virtio_mmio_present,
                tpm_tis_present: config.devices.tpm_tis_present,
            },
        );
        let mut fw_cfg = FwCfg::new();
        // Minimal real control entries the firmware/OS consult.
        if config.devices.virtio_boot_media_present {
            fw_cfg.add_file("bootorder", bootorder::qemu_virtio_blk_pci_bootorder());
        }
        // `etc/system-states` advertises which ACPI S-states are enabled; the
        // firmware may write it back, so it is writable. 6 bytes: S3, S4, ... .
        fw_cfg.add_writable_file("etc/system-states", vec![0u8; 6]);
        if config.devices.ramfb_present {
            fw_cfg.add_writable_file(RAMFB_FW_CFG_FILE, vec![0u8; RAMFB_CONFIG_SIZE]);
        }
        let acpi = build_acpi_with_devices(
            config.fdt.cpu_count,
            AcpiDeviceConfig {
                tpm_tis_present: config.devices.tpm_tis_present,
            },
        );
        fw_cfg.add_file(ACPI_RSDP_FILE, acpi.rsdp);
        fw_cfg.add_file(ACPI_TABLE_FILE, acpi.tables);
        fw_cfg.add_file(ACPI_LOADER_FILE, acpi.loader);
        if let Some(tpm_log) = acpi.tpm_log {
            fw_cfg.add_file(ACPI_TPM_LOG_FILE, tpm_log);
            // The pinned ArmVirtQemu EDK2 firmware uses this QEMU-compatible
            // record to discover and initialize the PPI page before processing
            // pending OS requests. Keep it under the same TPM-presence branch
            // as ACPI, the event log, MMIO routing, and the concrete backend.
            let ppi_address = u32::try_from(machine::TPM_PPI.base)
                .expect("TPM PPI address must fit QEMU's 32-bit fw_cfg contract");
            fw_cfg.add_file(
                TPM_PPI_FW_CFG_FILE,
                build_qemu_fw_cfg_tpm_config(ppi_address).to_vec(),
            );
        }
        let smbios = build_smbios(config.fdt.cpu_count, config.fdt.ram_size);
        fw_cfg.add_file(SMBIOS_ANCHOR_FILE, smbios.anchor);
        fw_cfg.add_file(SMBIOS_TABLE_FILE, smbios.tables);
        let mut nvme = NvmeController::new(DEFAULT_NVME_DISK_BYTES);
        nvme.set_direct_dma_enabled(!env_flag("BRIDGEVM_NVME_BUFFERED_IO"));
        Self {
            cfg: config.fdt,
            devices: config.devices,
            fw_cfg,
            uart: Pl011::new(),
            rtc: Pl031::new(),
            pcie: PcieEcam::new_with_config(PcieEcamConfig {
                xhci_present: config.devices.xhci_present,
                hda_present: config.devices.hda_present,
                virtio_blk_present: config.devices.virtio_boot_media_present,
                virtio_net_present: config.devices.virtio_net_present,
                virtio_gpu_present: config.devices.virtio_gpu_present,
                virtio_console_present: config.devices.virtio_console_present,
                virtio_gpu_pci_device_id: config.devices.virtio_gpu_pci_device_id,
                virtio_gpu_3d_enabled: virtio_gpu_3d_enabled_for_pcie(),
            }),
            nvme,
            xhci: XhciController::new(),
            hda: config.devices.hda_present.then(HdaController::new),
            virtio_iso: None,
            pci_boot_media: None,
            virtio_net: config.devices.virtio_net_present.then(|| {
                VirtioPciNet::new(make_virtio_net_backend(config.devices.virtio_net_backend))
            }),
            virtio_gpu: config.devices.virtio_gpu_present.then(make_virtio_gpu),
            virtio_console: config
                .devices
                .virtio_console_present
                .then(VirtioPciConsole::new),
            tpm_tis: tpm_backend.map(TpmTis::new),
            tpm_ppi: config.devices.tpm_tis_present.then(TpmPpi::new),
            ramfb: Ramfb::new(),
            flash_vars: P30NorFlash::new(
                machine::FLASH_VARS.base,
                machine::FLASH_VARS.size as usize,
                0x40000,
            ),
            pending_msix: Vec::new(),
            pending_spi_levels: Vec::new(),
            nvme_completion_scratch: Vec::new(),
            xhci_hid_boot_key_report_stats: XhciHidBootKeyReportStats::default(),
            nvme_ecam_touched: false,
            nvme_mmio_reached: false,
            nvme_cc_enabled: false,
            nvme_admin_doorbell_rung: false,
            dtb,
            xhci_report_interval: Duration::ZERO,
            host_now: None,
            xhci_dci3_last_emission: None,
            xhci_dci5_last_emission: None,
        }
    }

    /// Set the minimum host-time interval between consecutive HID interrupt-IN
    /// report emissions on DCI3/DCI5. `Duration::ZERO` disables pacing (drain a
    /// queued sequence as fast as the guest arms transfer descriptors). Live
    /// runs set this to avoid bursting keystrokes the guest then drops.
    pub fn set_xhci_report_interval(&mut self, interval: Duration) {
        self.xhci_report_interval = interval;
    }

    /// Feed the current host time to the platform once per run-loop iteration.
    /// The report-pacing gate reads this; the `Instant::now()` call itself stays
    /// in the probe so this crate holds no clock and unit tests stay
    /// deterministic (they never call this, so pacing is inert).
    pub fn set_host_now(&mut self, now: Instant) {
        self.host_now = Some(now);
    }

    /// Load the writable pflash bank backing bytes. Live HVF code leaves the vars
    /// bank unmapped so NOR command/status reads and writes trap here instead of
    /// being treated as plain RAM stores.
    pub fn load_flash_vars(&mut self, data: &[u8]) {
        self.flash_vars.load(data);
    }

    pub fn reset(&mut self) {
        let virtio_iso_irq_was_high = self
            .virtio_iso
            .as_ref()
            .is_some_and(VirtioMmioBlock::interrupt_line_level);
        let pci_boot_media_irq_was_high = self
            .pci_boot_media
            .as_ref()
            .is_some_and(VirtioPciBlock::interrupt_line_level);
        self.fw_cfg.reset_runtime_state();
        if self.devices.ramfb_present {
            self.fw_cfg.reset_file_bytes(RAMFB_FW_CFG_FILE, 0);
        }
        self.uart = Pl011::new();
        self.rtc = Pl031::new();
        self.pcie = PcieEcam::new_with_config(PcieEcamConfig {
            xhci_present: self.devices.xhci_present,
            hda_present: self.devices.hda_present,
            virtio_blk_present: self.devices.virtio_boot_media_present,
            virtio_net_present: self.devices.virtio_net_present,
            virtio_gpu_present: self.devices.virtio_gpu_present,
            virtio_console_present: self.devices.virtio_console_present,
            virtio_gpu_pci_device_id: self.devices.virtio_gpu_pci_device_id,
            virtio_gpu_3d_enabled: virtio_gpu_3d_enabled_for_pcie(),
        });
        self.nvme.reset_registers_keep_disks();
        self.xhci = XhciController::new();
        if self.devices.hda_present {
            if let Some(hda) = self.hda.as_mut() {
                hda.reset_runtime_state();
            } else {
                self.hda = Some(HdaController::new());
            }
        } else {
            self.hda = None;
        }
        self.ramfb = Ramfb::new();
        self.flash_vars.reset_runtime_state();
        if let Some(dev) = self.virtio_iso.as_mut() {
            dev.reset_runtime_state();
        }
        if let Some(dev) = self.pci_boot_media.as_mut() {
            dev.reset_runtime_state();
        }
        if let Some(dev) = self.virtio_net.as_mut() {
            dev.reset_runtime_state();
        }
        if let Some(dev) = self.virtio_gpu.as_mut() {
            dev.reset_runtime_state();
        }
        if let Some(dev) = self.virtio_console.as_mut() {
            dev.reset_runtime_state();
        }
        if let Some(tpm) = self.tpm_tis.as_mut() {
            tpm.reset().expect("TPM backend reset failed");
        }
        if let Some(ppi) = self.tpm_ppi.as_mut() {
            ppi.reset_runtime_state();
        }
        self.pending_msix.clear();
        self.pending_spi_levels.clear();
        if virtio_iso_irq_was_high && self.devices.legacy_virtio_mmio_present {
            self.pending_spi_levels.push((
                machine::spi_to_intid(machine::virtio_mmio_spi(INSTALLER_ISO_SLOT as u32)),
                false,
            ));
        }
        if pci_boot_media_irq_was_high && self.devices.virtio_boot_media_present {
            self.pending_spi_levels
                .push((machine::spi_to_intid(machine::SPI_PCIE_INTA), false));
        }
        self.xhci_hid_boot_key_report_stats = XhciHidBootKeyReportStats::default();
        self.nvme_ecam_touched = false;
        self.nvme_mmio_reached = false;
        self.nvme_cc_enabled = false;
        self.nvme_admin_doorbell_rung = false;
        // The interrupt-IN endpoints are re-armed from scratch after reset;
        // start report pacing fresh (the configured interval is preserved).
        self.xhci_dci3_last_emission = None;
        self.xhci_dci5_last_emission = None;
    }

    /// Snapshot the writable pflash variable bank, including guest writes
    /// accepted through the NOR command protocol.
    pub fn flash_vars_image(&self) -> &[u8] {
        self.flash_vars.image()
    }

    pub fn tpm_tis_stats(&self) -> Option<TpmTisStats> {
        self.tpm_tis.as_ref().map(TpmTis::stats)
    }

    pub fn tpm_ppi_stats(&self) -> Option<TpmPpiStats> {
        self.tpm_ppi.as_ref().map(TpmPpi::stats)
    }

    pub fn tpm_memory_overwrite_requested(&self) -> bool {
        self.tpm_ppi
            .as_ref()
            .is_some_and(TpmPpi::memory_overwrite_requested)
    }

    /// Replace the first NVMe disk image. The image is padded to full 512-byte
    /// LBAs by the controller.
    pub fn load_nvme_disk(&mut self, data: Vec<u8>) {
        self.nvme.load_disk_image(data);
    }

    /// Attach a host raw disk file as the first NVMe namespace without reading
    /// the whole image into memory. With `write_back = false`, guest writes stay
    /// in a sparse overlay and are only visible through explicit export.
    pub fn attach_nvme_raw_file(
        &mut self,
        path: impl AsRef<Path>,
        write_back: bool,
    ) -> io::Result<()> {
        self.nvme.load_raw_file(path, write_back)
    }

    /// Whether NVMe reads and writes may use stable guest-RAM host pointers.
    /// `BRIDGEVM_NVME_BUFFERED_IO=1` disables this for process-local A/B tests.
    pub fn nvme_direct_dma_enabled(&self) -> bool {
        self.nvme.direct_dma_enabled()
    }

    /// Attach a blank NSID-2 target namespace of `disk_bytes` in-memory storage,
    /// so Windows sees a second empty disk to install onto.
    pub fn attach_nvme_second_namespace(&mut self, disk_bytes: usize) {
        self.nvme.attach_second_namespace(disk_bytes);
    }

    /// Attach a host raw file as the NSID-2 target namespace (sparse overlay when
    /// `write_back = false`, direct writes when true).
    pub fn attach_nvme_second_namespace_raw_file(
        &mut self,
        path: impl AsRef<Path>,
        write_back: bool,
    ) -> io::Result<()> {
        self.nvme.attach_second_namespace_raw_file(path, write_back)
    }

    /// Attach a read-only Windows/Linux installer ISO to the last QEMU virt
    /// virtio-mmio transport slot. QEMU's own `virtio-blk-device` oracle uses
    /// slot 31 (`0x0a003e00`) for an explicitly added MMIO block device.
    pub fn attach_virtio_iso(&mut self, path: impl AsRef<Path>) -> io::Result<()> {
        if !self.devices.legacy_virtio_mmio_present {
            return Ok(());
        }
        self.virtio_iso = Some(VirtioMmioBlock::open_read_only(path)?);
        Ok(())
    }

    pub fn attach_pci_boot_media(&mut self, path: impl AsRef<Path>) -> io::Result<()> {
        if !self.devices.virtio_boot_media_present {
            return Ok(());
        }
        self.pci_boot_media = Some(VirtioPciBlock::open_read_only(path)?);
        Ok(())
    }

    /// Current read/queue counters for the installer ISO block device, if one is
    /// attached. Live probes print this when diagnosing Windows media boot.
    pub fn virtio_iso_stats(&self) -> Option<VirtioMmioBlockStats> {
        self.virtio_iso.as_ref().map(VirtioMmioBlock::stats)
    }

    pub fn pci_boot_media_stats(&self) -> Option<VirtioMmioBlockStats> {
        self.pci_boot_media.as_ref().map(VirtioPciBlock::stats)
    }

    pub fn pci_boot_media_request_trace(&self) -> Option<Vec<VirtioBlockRequestTrace>> {
        self.pci_boot_media
            .as_ref()
            .map(VirtioPciBlock::recent_request_trace)
    }

    pub fn virtio_net_stats(&self) -> Option<VirtioNetStats> {
        self.virtio_net.as_ref().map(VirtioPciNet::stats)
    }

    pub fn virtio_net_nat_stats(&self) -> Option<NatStats> {
        self.virtio_net
            .as_ref()
            .and_then(|dev| dev.backend().nat_stats())
    }

    pub fn virtio_gpu_stats(&self) -> Option<VirtioGpuStats> {
        self.virtio_gpu.as_ref().map(VirtioPciGpu::stats)
    }

    pub fn virtio_console_stats(&self) -> Option<VirtioConsoleStats> {
        self.virtio_console.as_ref().map(VirtioPciConsole::stats)
    }

    pub fn virtio_gpu_scanout(&self) -> Option<VirtioGpuScanout<'_>> {
        self.virtio_gpu.as_ref().and_then(VirtioPciGpu::scanout)
    }

    /// Wake signal for the host vblank waker thread; `Some` only when
    /// `BRIDGEVM_VBLANK_HZ` pacing is active on the virtio-gpu device.
    pub fn virtio_gpu_vblank_wake(&self) -> Option<Arc<VblankWakeState>> {
        self.virtio_gpu.as_ref().and_then(VirtioPciGpu::vblank_wake)
    }

    /// Request a guest scanout resize. Returns true when the geometry changed
    /// and the DISPLAY event + config-change interrupt were armed; the pending
    /// MSI-X is delivered by the next `drain_pending_msix_into` on the drain
    /// path. Returns false when no GPU is present or the size is a no-op.
    pub fn request_virtio_gpu_resolution(&mut self, width: u32, height: u32) -> bool {
        self.virtio_gpu
            .as_mut()
            .is_some_and(|gpu| gpu.request_display_resolution(width, height))
    }

    /// Current virtio-gpu scanout geometry, if the device is present.
    pub fn virtio_gpu_resolution(&self) -> Option<(u32, u32)> {
        self.virtio_gpu
            .as_ref()
            .map(VirtioPciGpu::display_resolution)
    }

    pub fn set_virtio_gpu_shm_map_port(&mut self, port: Box<dyn GpuShmMapPort>) -> bool {
        let Some(window_size) = self.pcie.virtio_gpu_host_visible_bar_size() else {
            return false;
        };
        let Some(gpu) = self.virtio_gpu.as_mut() else {
            return false;
        };
        gpu.set_shm_map_port(port, window_size);
        true
    }

    pub fn virtio_gpu_host_visible_bar_base(&self) -> Option<u64> {
        self.pcie.virtio_gpu_host_visible_bar_base()
    }

    pub fn pump_virtio_net_receive(&mut self, mem: &mut dyn GuestMemoryMut) -> bool {
        self.poll_virtio_net(mem)
    }

    pub fn poll_virtio_net(&mut self, mem: &mut dyn GuestMemoryMut) -> bool {
        let Some(dev) = self.virtio_net.as_mut() else {
            return false;
        };
        dev.poll_host_sockets();
        // Every poll may enqueue an unbounded batch of frames from drained
        // host sockets, so delivering a single frame per poll lets the shared
        // reply queue grow without bound under bulk host->guest traffic and
        // starves newer connections behind it (live guest fetches collapsed
        // below curl's 1000 B/s abort threshold). Drain a bounded burst; the
        // loop also stops as soon as the guest has no free RX descriptor.
        const RX_BURST_FRAMES: usize = 256;
        let mut delivered = false;
        for _ in 0..RX_BURST_FRAMES {
            if !dev.pump_receive(mem) {
                break;
            }
            delivered = true;
        }
        if delivered {
            self.flush_virtio_net_pending_msix();
        }
        delivered
    }

    pub fn virtio_console_agent_send(&mut self, data: &[u8], mem: &mut dyn GuestMemoryMut) {
        let Some(dev) = self.virtio_console.as_mut() else {
            return;
        };
        dev.agent_send(data);
        dev.poll(mem);
        self.flush_virtio_console_pending_msix();
    }

    pub fn virtio_console_agent_take_inbound(&mut self) -> Vec<u8> {
        self.virtio_console
            .as_mut()
            .map(VirtioPciConsole::take_inbound)
            .unwrap_or_default()
    }

    pub fn virtio_console_agent_drain_inbound_into(&mut self, out: &mut Vec<u8>) {
        let Some(dev) = self.virtio_console.as_mut() else {
            return;
        };
        dev.drain_inbound_into(out);
    }

    pub fn poll_virtio_console(&mut self, mem: &mut dyn GuestMemoryMut) -> bool {
        let Some(dev) = self.virtio_console.as_mut() else {
            return false;
        };
        let progressed = dev.poll(mem);
        if progressed {
            self.flush_virtio_console_pending_msix();
        }
        progressed
    }

    pub fn virtio_iso_request_trace(&self) -> Option<Vec<VirtioBlockRequestTrace>> {
        self.virtio_iso
            .as_ref()
            .map(VirtioMmioBlock::recent_request_trace)
    }

    pub fn queue_xhci_hid_boot_key_usage(
        &mut self,
        usage: u8,
    ) -> Result<(), XhciHidBootKeyQueueError> {
        if !self.devices.xhci_present {
            self.xhci_hid_boot_key_report_stats.busy_rejections = self
                .xhci_hid_boot_key_report_stats
                .busy_rejections
                .saturating_add(1);
            return Err(XhciHidBootKeyQueueError::Busy);
        }
        if usage != HID_BOOT_KEYBOARD_USAGE_SPACE {
            self.xhci_hid_boot_key_report_stats
                .unsupported_usage_rejections = self
                .xhci_hid_boot_key_report_stats
                .unsupported_usage_rejections
                .saturating_add(1);
            return Err(XhciHidBootKeyQueueError::UnsupportedUsage { usage });
        }
        self.queue_xhci_setup_input_actions(&[SetupInputAction::Space])
            .map_err(|error| match error {
                XhciSetupInputQueueError::Busy => XhciHidBootKeyQueueError::Busy,
                XhciSetupInputQueueError::EmptySequence
                | XhciSetupInputQueueError::TooManyActions { .. } => XhciHidBootKeyQueueError::Busy,
            })?;
        self.xhci_hid_boot_key_report_stats.queued_space_reports = self
            .xhci_hid_boot_key_report_stats
            .queued_space_reports
            .saturating_add(1);
        Ok(())
    }

    pub fn queue_xhci_setup_input_actions(
        &mut self,
        actions: &[SetupInputAction],
    ) -> Result<(), XhciSetupInputQueueError> {
        if !self.devices.xhci_present {
            self.xhci_hid_boot_key_report_stats.busy_rejections = self
                .xhci_hid_boot_key_report_stats
                .busy_rejections
                .saturating_add(1);
            return Err(XhciSetupInputQueueError::Busy);
        }
        match self.xhci.queue_setup_input_actions(actions) {
            Ok(()) => Ok(()),
            Err(XhciSetupInputQueueError::Busy) => {
                self.xhci_hid_boot_key_report_stats.busy_rejections = self
                    .xhci_hid_boot_key_report_stats
                    .busy_rejections
                    .saturating_add(1);
                Err(XhciSetupInputQueueError::Busy)
            }
            Err(error) => Err(error),
        }
    }

    pub fn queue_xhci_setup_input_actions_with_mem(
        &mut self,
        actions: &[SetupInputAction],
        mem: &mut dyn GuestMemoryMut,
    ) -> Result<(), XhciSetupInputQueueError> {
        self.queue_xhci_setup_input_actions(actions)?;
        self.drain_xhci_setup_input_reports(mem);
        Ok(())
    }

    pub fn drain_xhci_setup_input_reports(&mut self, mem: &mut dyn GuestMemoryMut) -> bool {
        if !self.devices.xhci_present {
            return false;
        }
        let mut posted_completion = false;
        let stats = self.xhci.setup_input_report_stats();
        let emitted_reports = stats
            .emitted_key_reports
            .saturating_add(stats.emitted_release_reports);
        let pending_reports = stats.queued_reports.saturating_sub(emitted_reports);
        for _ in 0..pending_reports.min(MAX_XHCI_SETUP_INPUT_DRAIN_ATTEMPTS as u64) {
            if !self.report_pacing_allows_emission(self.xhci_dci3_last_emission) {
                break;
            }
            if !self.xhci.process_queued_dci3_input(mem) {
                break;
            }
            posted_completion = true;
            self.queue_xhci_completion_msix();
            self.xhci_dci3_last_emission = self.host_now.or(self.xhci_dci3_last_emission);
        }
        if posted_completion {
            self.flush_xhci_pending_msix();
        }
        posted_completion
    }

    /// Report-pacing gate: while `host_now` is unset (unit tests) or the interval
    /// is zero, every emission is allowed (unpaced). Otherwise an emission is
    /// held off until the configured interval has elapsed since this endpoint's
    /// last emission. Because a single drain call sees one fixed `host_now`, this
    /// releases at most one report per run-loop iteration once pacing is active.
    fn report_pacing_allows_emission(&self, last_emission: Option<Instant>) -> bool {
        match self.host_now {
            None => true,
            Some(now) => {
                report_pacing_allows_emission(self.xhci_report_interval, last_emission, now)
            }
        }
    }

    pub fn xhci_hid_boot_key_report_stats(&self) -> XhciHidBootKeyReportStats {
        self.xhci_hid_boot_key_report_stats
    }

    pub fn xhci_setup_input_report_stats(&self) -> XhciSetupInputReportStats {
        self.xhci.setup_input_report_stats()
    }

    pub fn queue_xhci_pointer_input_actions(
        &mut self,
        actions: &[PointerInputAction],
    ) -> Result<(), XhciPointerInputQueueError> {
        if !self.devices.xhci_present {
            return Err(XhciPointerInputQueueError::Busy);
        }
        self.xhci.queue_pointer_input_actions(actions)
    }

    pub fn queue_xhci_pointer_input_actions_with_mem(
        &mut self,
        actions: &[PointerInputAction],
        mem: &mut dyn GuestMemoryMut,
    ) -> Result<(), XhciPointerInputQueueError> {
        self.queue_xhci_pointer_input_actions(actions)?;
        self.drain_xhci_pointer_input_reports(mem);
        Ok(())
    }

    pub fn drain_xhci_pointer_input_reports(&mut self, mem: &mut dyn GuestMemoryMut) -> bool {
        if !self.devices.xhci_present {
            return false;
        }
        let mut posted_completion = false;
        let stats = self.xhci.pointer_input_report_stats();
        let emitted_reports = stats
            .emitted_move_reports
            .saturating_add(stats.emitted_button_reports)
            .saturating_add(stats.emitted_release_reports)
            .saturating_add(stats.emitted_wheel_reports);
        let pending_reports = stats.queued_reports.saturating_sub(emitted_reports);
        for _ in 0..pending_reports.min(MAX_XHCI_SETUP_INPUT_DRAIN_ATTEMPTS as u64) {
            if !self.report_pacing_allows_emission(self.xhci_dci5_last_emission) {
                break;
            }
            if !self.xhci.process_queued_dci5_pointer_input(mem) {
                break;
            }
            posted_completion = true;
            self.queue_xhci_completion_msix();
            self.xhci_dci5_last_emission = self.host_now.or(self.xhci_dci5_last_emission);
        }
        if posted_completion {
            self.flush_xhci_pending_msix();
        }
        posted_completion
    }

    pub fn xhci_pointer_input_report_stats(&self) -> XhciPointerInputReportStats {
        self.xhci.pointer_input_report_stats()
    }

    pub fn xhci_event_lifecycle_stats(&self) -> XhciEventLifecycleStats {
        self.xhci.event_lifecycle_stats()
    }

    pub fn xhci_hid_semantic_stats(&self) -> XhciHidSemanticStats {
        self.xhci.hid_semantic_stats()
    }

    pub fn ramfb_config(&self) -> Option<RamfbConfig> {
        if !self.devices.ramfb_present {
            return None;
        }
        self.ramfb.config()
    }

    /// Snapshot the first NVMe disk image, including guest writes processed so
    /// far. Live probes use this to persist an explicitly writable image.
    pub fn nvme_disk(&self) -> &[u8] {
        self.nvme.disk_image()
    }

    /// In-memory snapshot of the first NVMe disk, if the media is memory-backed.
    pub fn nvme_disk_if_memory(&self) -> Option<&[u8]> {
        self.nvme.disk_image_if_memory()
    }

    pub fn nvme_second_namespace_disk_if_memory(&self) -> Option<&[u8]> {
        self.nvme.second_namespace_disk_image_if_memory()
    }

    /// Export current NVMe media to a raw file, applying sparse overlay writes.
    pub fn export_nvme_disk(&mut self, path: impl AsRef<Path>) -> io::Result<u64> {
        self.nvme.export_disk_image(path)
    }

    pub fn export_nvme_second_namespace_disk(&mut self, path: impl AsRef<Path>) -> io::Result<u64> {
        self.nvme.export_second_namespace_disk_image(path)
    }

    /// Flush write-through NVMe media.
    pub fn flush_nvme_disk(&mut self) -> io::Result<()> {
        self.nvme.flush_disk()
    }

    pub fn flush_nvme_second_namespace_disk(&mut self) -> io::Result<()> {
        self.nvme.flush_second_namespace_disk()
    }

    /// Current byte length of the NVMe namespace backing media.
    pub fn nvme_disk_len(&self) -> u64 {
        self.nvme.disk_len()
    }

    pub fn nvme_second_namespace_disk_len(&self) -> Option<u64> {
        self.nvme.second_namespace_disk_len()
    }

    /// Recent NVMe commands processed by the controller, oldest first. Live
    /// probes use this to diagnose Windows setup stalls without enabling a
    /// firehose of per-command logging.
    pub fn nvme_command_trace(&self) -> Vec<NvmeCommandTrace> {
        self.nvme.recent_command_trace()
    }

    pub fn nvme_pcie_liveness(&self) -> NvmePcieLiveness {
        let state = self.pcie.nvme_endpoint_state();
        NvmePcieLiveness {
            nvme_advertised: state.advertised,
            nvme_ecam_touched: self.nvme_ecam_touched,
            nvme_command_memory_enabled: state.command_memory_enabled,
            nvme_command_bus_master_enabled: state.command_bus_master_enabled,
            nvme_bar0_assigned: state.bar0_assigned,
            nvme_mmio_reached: self.nvme_mmio_reached,
            nvme_cc_enabled: self.nvme_cc_enabled,
            nvme_admin_doorbell_rung: self.nvme_admin_doorbell_rung,
        }
    }

    /// Resolve a guest-physical PCIe MMIO address against the currently
    /// programmed endpoint BARs without dispatching the access.
    pub fn pcie_mmio_target(&self, gpa: u64) -> Option<PcieMmioTarget> {
        self.pcie.mmio_target(gpa)
    }

    /// Resolve a guest-physical PCIe I/O-window address against the currently
    /// programmed endpoint I/O BARs without dispatching the access.
    pub fn pcie_pio_target(&self, gpa: u64) -> Option<PciePioTarget> {
        let port = gpa.checked_sub(machine::PCIE_PIO.base)?;
        self.pcie.pio_target(port)
    }

    /// Drain MSI-X messages raised by PCIe devices since the last call. The live
    /// HVF run loop turns these into `hv_gic_send_msi` calls after configuring
    /// Apple `hv_gic`'s MSI frame.
    pub fn take_pending_msix(&mut self) -> Vec<MsixMessage> {
        if self.pending_msix.is_empty() {
            Vec::new()
        } else {
            self.pending_msix.drain(..).collect()
        }
    }

    /// Drain pending MSI-X messages into caller-owned storage.
    pub fn drain_pending_msix_into(&mut self, out: &mut Vec<MsixMessage>) {
        out.append(&mut self.pending_msix);
    }

    /// Drain level changes for legacy SPI-backed devices such as virtio-mmio.
    /// The live HVF loop turns these into `hv_gic_set_spi(intid, level)`.
    pub fn take_pending_spi_levels(&mut self) -> Vec<(u32, bool)> {
        if self.pending_spi_levels.is_empty() {
            Vec::new()
        } else {
            self.pending_spi_levels.drain(..).collect()
        }
    }

    /// Drain pending SPI level changes into caller-owned storage.
    pub fn drain_pending_spi_levels_into(&mut self, out: &mut Vec<(u32, bool)>) {
        out.append(&mut self.pending_spi_levels);
    }

    /// Register QEMU direct-Linux-boot payloads in the fixed fw_cfg slots that
    /// ArmVirtQemu's `QemuKernelLoaderFsDxe` reads before BDS falls through to
    /// normal boot options. `cmdline` must include the terminating NUL byte.
    pub fn set_linux_boot_blobs(
        &mut self,
        kernel: Vec<u8>,
        initrd: Option<Vec<u8>>,
        cmdline: Vec<u8>,
    ) {
        assert!(
            cmdline.last().copied() == Some(0),
            "Linux fw_cfg cmdline blob must be NUL-terminated"
        );
        let initrd = initrd.unwrap_or_default();
        // SAFE-EXPECT: fw_cfg direct-boot size registers are u32 by QEMU contract.
        let kernel_len = u32::try_from(kernel.len()).expect("kernel blob >4 GiB");
        // SAFE-EXPECT: fw_cfg direct-boot size registers are u32 by QEMU contract.
        let initrd_len = u32::try_from(initrd.len()).expect("initrd blob >4 GiB");
        // SAFE-EXPECT: fw_cfg direct-boot size registers are u32 by QEMU contract.
        let cmdline_len = u32::try_from(cmdline.len()).expect("cmdline blob >4 GiB");
        self.fw_cfg
            .add_item(KEY_KERNEL_SIZE, kernel_len.to_le_bytes().to_vec());
        self.fw_cfg.add_item(KEY_KERNEL_DATA, kernel);
        self.fw_cfg
            .add_item(KEY_INITRD_SIZE, initrd_len.to_le_bytes().to_vec());
        self.fw_cfg.add_item(KEY_INITRD_DATA, initrd);
        self.fw_cfg
            .add_item(KEY_CMDLINE_SIZE, cmdline_len.to_le_bytes().to_vec());
        self.fw_cfg.add_item(KEY_CMDLINE_DATA, cmdline);
    }

    /// Register the guest ACPI tables (`etc/acpi/rsdp`, `etc/acpi/tables`,
    /// `etc/table-loader`) the firmware installs. On Path A these come from the
    /// QEMU-style table generator; until that lands this lets callers attach
    /// known-good bytes (e.g. captured from the QEMU oracle) so the rest of the
    /// pipeline can be exercised end to end.
    pub fn set_acpi_tables(&mut self, rsdp: Vec<u8>, tables: Vec<u8>, loader: Vec<u8>) {
        self.fw_cfg.add_file(ACPI_RSDP_FILE, rsdp);
        self.fw_cfg.add_file(ACPI_TABLE_FILE, tables);
        self.fw_cfg.add_file(ACPI_LOADER_FILE, loader);
    }

    /// Register the SMBIOS entry point + tables (`etc/smbios/smbios-anchor`,
    /// `etc/smbios/smbios-tables`).
    pub fn set_smbios(&mut self, anchor: Vec<u8>, tables: Vec<u8>) {
        self.fw_cfg.add_file(SMBIOS_ANCHOR_FILE, anchor);
        self.fw_cfg.add_file(SMBIOS_TABLE_FILE, tables);
    }

    /// The generated device tree blob (DTB magic `0xd00dfeed`).
    pub fn dtb(&self) -> &[u8] {
        &self.dtb
    }

    /// The guest memory layout. The DTB is placed at the base of RAM, where the
    /// firmware looks for it; the kernel/initrd are loaded above it.
    pub fn memory_layout(&self) -> GuestMemoryLayout {
        GuestMemoryLayout {
            flash_code: machine::FLASH_CODE,
            flash_vars: machine::FLASH_VARS,
            ram: Region::new(machine::RAM_BASE, self.cfg.ram_size),
            dtb_load: machine::RAM_BASE,
        }
    }

    /// Dispatch a guest MMIO access and return only the guest-visible result.
    pub fn on_mmio(&mut self, gpa: u64, op: MmioOp, mem: &mut dyn GuestMemoryMut) -> MmioOutcome {
        self.on_mmio_with_post_drain(gpa, op, mem).0
    }

    /// Dispatch a guest MMIO access and report which post-dispatch drains ran.
    /// Live HVF data-abort handling uses this to avoid repeating an empty drain in
    /// the same platform-lock hold.
    pub fn on_mmio_with_post_drain(
        &mut self,
        gpa: u64,
        op: MmioOp,
        mem: &mut dyn GuestMemoryMut,
    ) -> (MmioOutcome, MmioPostDrain) {
        let Some(device) = machine::device_at(gpa) else {
            return (MmioOutcome::Unmapped, MmioPostDrain::NONE);
        };
        let pcie_mmio_target = match device {
            "pcie-mmio-32" | "pcie-mmio-64" => self.pcie.mmio_target(gpa),
            _ => None,
        };
        let retry_setup_input_after_mmio = match device {
            "pcie-mmio-32" | "pcie-mmio-64" => !matches!(
                pcie_mmio_target,
                Some(target) if target.bdf == XHCI_BDF && target.bar_index == 0
            ),
            _ => true,
        };
        let outcome = match device {
            "fw-cfg" => self.fw_cfg_access(gpa - machine::FW_CFG.base, op, mem),
            "uart" => self.uart_access(gpa - machine::UART.base, op),
            "rtc" => self.rtc_access(gpa - machine::RTC.base, op),
            "pcie-ecam" => self.pcie_access(gpa - machine::PCIE_ECAM.base, op),
            "pcie-mmio-32" => self.pcie_mmio_access("pcie-mmio-32", pcie_mmio_target, op, mem),
            "pcie-mmio-64" => self.pcie_mmio_access("pcie-mmio-64", pcie_mmio_target, op, mem),
            "pcie-pio" => self.pcie_pio_access(gpa, op, mem),
            "virtio-mmio" => self.virtio_mmio_access(gpa - machine::VIRTIO_MMIO.base, op, mem),
            "tpm-tis" if self.devices.tpm_tis_present => {
                let tpm = self
                    .tpm_tis
                    .as_mut()
                    .expect("TPM presence requires a backend");
                let offset = gpa - machine::TPM_TIS.base;
                match op {
                    MmioOp::Read { size } => MmioOutcome::ReadValue(tpm.mmio_read(offset, size)),
                    MmioOp::Write { size, value } => {
                        tpm.mmio_write(offset, size, value);
                        MmioOutcome::WriteAck
                    }
                }
            }
            "tpm-tis" => MmioOutcome::Unmapped,
            "tpm-ppi" if self.devices.tpm_tis_present => {
                let ppi = self
                    .tpm_ppi
                    .as_mut()
                    .expect("TPM presence requires a PPI mailbox");
                let offset = gpa - machine::TPM_PPI.base;
                match op {
                    MmioOp::Read { size } => MmioOutcome::ReadValue(ppi.mmio_read(offset, size)),
                    MmioOp::Write { size, value } => {
                        ppi.mmio_write(offset, size, value);
                        MmioOutcome::WriteAck
                    }
                }
            }
            "tpm-ppi" => MmioOutcome::Unmapped,
            "flash-vars" => self.flash_vars.access(gpa, op),
            // Modelled in the machine map but no device behaviour yet — surfaced
            // precisely so bring-up traces show the next thing to implement.
            other => MmioOutcome::KnownUnimplemented(other),
        };
        if retry_setup_input_after_mmio {
            self.drain_xhci_setup_input_reports(mem);
            return (outcome, MmioPostDrain::XHCI_SETUP_INPUT);
        }
        (outcome, MmioPostDrain::NONE)
    }

    /// Empty virtio-mmio transport slot. Advertise a valid legacy register block with
    /// DeviceID 0 so the firmware sees "valid transport, no device" and skips it
    /// silently — matching QEMU's empty slots. Returning 0 (no magic) instead made
    /// VirtioMmioInit fail with EFI_UNSUPPORTED and log 32 errors per boot.
    fn virtio_mmio_access(
        &mut self,
        slot_offset: u64,
        op: MmioOp,
        mem: &mut dyn GuestMemoryMut,
    ) -> MmioOutcome {
        if !self.devices.legacy_virtio_mmio_present {
            return MmioOutcome::Unmapped;
        }
        let slot = slot_offset / machine::VIRTIO_MMIO_SLOT_SIZE;
        let reg = slot_offset % machine::VIRTIO_MMIO_SLOT_SIZE;
        if slot == INSTALLER_ISO_SLOT {
            if let Some(dev) = self.virtio_iso.as_mut() {
                let (is_write, size, value) = match op {
                    MmioOp::Read { size } => (false, size, 0),
                    MmioOp::Write { size, value } => (true, size, value),
                };
                let old_irq = dev.interrupt_line_level();
                let result = dev.access(reg, is_write, size, value, mem);
                let new_irq = dev.interrupt_line_level();
                if old_irq != new_irq {
                    self.pending_spi_levels.push((
                        machine::spi_to_intid(machine::virtio_mmio_spi(INSTALLER_ISO_SLOT as u32)),
                        new_irq,
                    ));
                }
                return match result {
                    VirtioMmioBlockResult::ReadValue(v) => MmioOutcome::ReadValue(v),
                    VirtioMmioBlockResult::WriteAck => MmioOutcome::WriteAck,
                };
            }
        }
        match op {
            MmioOp::Read { .. } => {
                let value = match reg {
                    0x00 => 0x7472_6976, // MagicValue, "virt"
                    0x04 => 0x1,         // Version: virtio-mmio 0.9.5
                    0x08 => 0x0,         // DeviceID: 0 = no device present
                    0x0c => 0x554d_4551, // VendorID, "QEMU"
                    _ => 0,
                };
                MmioOutcome::ReadValue(value)
            }
            MmioOp::Write { .. } => MmioOutcome::WriteAck,
        }
    }

    /// PCIe ECAM config-space access: a real host bridge at 00:00.0, all-ones
    /// (no device) elsewhere. Replaces the earlier blanket all-ones stub.
    fn pcie_access(&mut self, ecam_offset: u64, op: MmioOp) -> MmioOutcome {
        if CfgAddr::from_ecam_offset(ecam_offset).bdf() == NVME_BDF {
            self.nvme_ecam_touched = true;
        }
        match op {
            MmioOp::Read { size } => MmioOutcome::ReadValue(self.pcie.cfg_read(ecam_offset, size)),
            MmioOp::Write { size, value } => {
                self.pcie.cfg_write(ecam_offset, size, value);
                self.flush_nvme_pending_msix();
                self.flush_xhci_pending_msix();
                self.flush_hda_pending_msi();
                self.flush_virtio_net_pending_msix();
                self.flush_virtio_gpu_pending_msix();
                self.flush_virtio_console_pending_msix();
                MmioOutcome::WriteAck
            }
        }
    }

    fn pcie_pio_access(
        &mut self,
        gpa: u64,
        op: MmioOp,
        mem: &mut dyn GuestMemoryMut,
    ) -> MmioOutcome {
        let Some(target) = self.pcie_pio_target(gpa) else {
            return MmioOutcome::KnownUnimplemented("pcie-pio");
        };
        match (target.bdf, target.bar_index) {
            (VIRTIO_BLK_BDF, 0) => self.pci_boot_media_legacy_pio_access(target.offset, op, mem),
            _ => MmioOutcome::KnownUnimplemented("pcie-pio"),
        }
    }

    fn pcie_mmio_access(
        &mut self,
        aperture: &'static str,
        target: Option<PcieMmioTarget>,
        op: MmioOp,
        mem: &mut dyn GuestMemoryMut,
    ) -> MmioOutcome {
        let Some(target) = target else {
            return MmioOutcome::KnownUnimplemented(aperture);
        };
        if target.bdf == NVME_BDF && target.bar_index == 0 {
            self.nvme_mmio_reached = true;
            match op {
                MmioOp::Write { value, .. } if target.offset == REG_CC && value & 1 != 0 => {
                    self.nvme_cc_enabled = true;
                }
                MmioOp::Write { .. } if target.offset == REG_DOORBELL_BASE => {
                    self.nvme_admin_doorbell_rung = true;
                }
                MmioOp::Read { .. } | MmioOp::Write { .. } => {}
            }
        }
        if target.bdf == VIRTIO_GPU_BDF {
            venus_start_trace_gpu_bar_access(target.bar_index, target.offset, &op);
        }
        match (target.bdf, target.bar_index) {
            (NVME_BDF, 0) => self.nvme_access(target.offset, op, mem),
            (HDA_BDF, 0) => self.hda_access(target.offset, op, mem),
            (XHCI_BDF, 0) => match op {
                MmioOp::Read { size } => {
                    MmioOutcome::ReadValue(self.xhci.mmio_read(target.offset, size))
                }
                MmioOp::Write { size, value } => {
                    let posted_completion =
                        self.xhci
                            .mmio_write_with_mem(target.offset, size, value, mem);
                    if posted_completion {
                        self.queue_xhci_completion_msix();
                    }
                    self.flush_xhci_pending_msix();
                    MmioOutcome::WriteAck
                }
            },
            (VIRTIO_BLK_BDF, 1) => self.pci_boot_media_msix_access(target.offset, op),
            (VIRTIO_BLK_BDF, 4) => self.pci_boot_media_access(target.offset, op, mem),
            (VIRTIO_NET_BDF, 1) => self.virtio_net_msix_access(target.offset, op),
            (VIRTIO_NET_BDF, 4) => self.virtio_net_access(target.offset, op, mem),
            (VIRTIO_GPU_BDF, 1) => self.virtio_gpu_msix_access(target.offset, op),
            (VIRTIO_GPU_BDF, 2) => {
                // Host-visible shm window (BAR2). Real backing appears only via
                // hv_vm_map when a blob is mapped; a CPU access that exits here
                // hit a region with no mapped blob. RAZ/WI keeps the guest
                // alive; the venus-start trace above records it.
                match op {
                    MmioOp::Read { .. } => MmioOutcome::ReadValue(0),
                    MmioOp::Write { .. } => MmioOutcome::WriteAck,
                }
            }
            (VIRTIO_GPU_BDF, 4) => self.virtio_gpu_access(target.offset, op, mem),
            (VIRTIO_CONSOLE_BDF, 1) => self.virtio_console_msix_access(target.offset, op),
            (VIRTIO_CONSOLE_BDF, 4) => self.virtio_console_access(target.offset, op, mem),
            _ => MmioOutcome::KnownUnimplemented(aperture),
        }
    }

    fn pci_boot_media_msix_access(&mut self, offset: u64, op: MmioOp) -> MmioOutcome {
        let Some(dev) = self.pci_boot_media.as_mut() else {
            return MmioOutcome::KnownUnimplemented("virtio-blk-pci");
        };
        let result = match op {
            MmioOp::Read { size } => dev.msix_bar_access(offset, VirtioPciBlockOp::Read { size }),
            MmioOp::Write { size, value } => {
                dev.msix_bar_access(offset, VirtioPciBlockOp::Write { size, value })
            }
        };
        match result {
            VirtioMmioBlockResult::ReadValue(v) => MmioOutcome::ReadValue(v),
            VirtioMmioBlockResult::WriteAck => MmioOutcome::WriteAck,
        }
    }

    fn pci_boot_media_access(
        &mut self,
        offset: u64,
        op: MmioOp,
        mem: &mut dyn GuestMemoryMut,
    ) -> MmioOutcome {
        let Some(dev) = self.pci_boot_media.as_mut() else {
            return MmioOutcome::KnownUnimplemented("virtio-blk-pci");
        };
        let old_irq = dev.interrupt_line_level();
        let result = match op {
            MmioOp::Read { size } => dev.access(offset, VirtioPciBlockOp::Read { size }, mem),
            MmioOp::Write { size, value } => {
                dev.access(offset, VirtioPciBlockOp::Write { size, value }, mem)
            }
        };
        let new_irq = dev.interrupt_line_level();
        if old_irq != new_irq {
            self.pending_spi_levels
                .push((machine::spi_to_intid(machine::SPI_PCIE_INTA), new_irq));
        }
        match result {
            VirtioMmioBlockResult::ReadValue(v) => MmioOutcome::ReadValue(v),
            VirtioMmioBlockResult::WriteAck => MmioOutcome::WriteAck,
        }
    }

    fn virtio_net_msix_access(&mut self, offset: u64, op: MmioOp) -> MmioOutcome {
        let Some(dev) = self.virtio_net.as_mut() else {
            return MmioOutcome::KnownUnimplemented("virtio-net-pci");
        };
        let is_write = matches!(op, MmioOp::Write { .. });
        let result = match op {
            MmioOp::Read { size } => dev.msix_bar_access(offset, VirtioPciNetOp::Read { size }),
            MmioOp::Write { size, value } => {
                dev.msix_bar_access(offset, VirtioPciNetOp::Write { size, value })
            }
        };
        if is_write {
            self.flush_virtio_net_pending_msix();
        }
        match result {
            VirtioNetResult::ReadValue(v) => MmioOutcome::ReadValue(v),
            VirtioNetResult::WriteAck => MmioOutcome::WriteAck,
        }
    }

    fn virtio_net_access(
        &mut self,
        offset: u64,
        op: MmioOp,
        mem: &mut dyn GuestMemoryMut,
    ) -> MmioOutcome {
        let Some(dev) = self.virtio_net.as_mut() else {
            return MmioOutcome::KnownUnimplemented("virtio-net-pci");
        };
        let result = match op {
            MmioOp::Read { size } => dev.access(offset, VirtioPciNetOp::Read { size }, mem),
            MmioOp::Write { size, value } => {
                dev.access(offset, VirtioPciNetOp::Write { size, value }, mem)
            }
        };
        self.flush_virtio_net_pending_msix();
        match result {
            VirtioNetResult::ReadValue(v) => MmioOutcome::ReadValue(v),
            VirtioNetResult::WriteAck => MmioOutcome::WriteAck,
        }
    }

    fn virtio_gpu_msix_access(&mut self, offset: u64, op: MmioOp) -> MmioOutcome {
        let Some(dev) = self.virtio_gpu.as_mut() else {
            return MmioOutcome::KnownUnimplemented("virtio-gpu-pci");
        };
        let is_write = matches!(op, MmioOp::Write { .. });
        let result = match op {
            MmioOp::Read { size } => dev.msix_bar_access(offset, VirtioPciGpuOp::Read { size }),
            MmioOp::Write { size, value } => {
                dev.msix_bar_access(offset, VirtioPciGpuOp::Write { size, value })
            }
        };
        if is_write {
            self.flush_virtio_gpu_pending_msix();
        }
        match result {
            VirtioGpuResult::ReadValue(v) => MmioOutcome::ReadValue(v),
            VirtioGpuResult::WriteAck => MmioOutcome::WriteAck,
        }
    }

    fn virtio_gpu_access(
        &mut self,
        offset: u64,
        op: MmioOp,
        mem: &mut dyn GuestMemoryMut,
    ) -> MmioOutcome {
        let Some(dev) = self.virtio_gpu.as_mut() else {
            return MmioOutcome::KnownUnimplemented("virtio-gpu-pci");
        };
        let result = match op {
            MmioOp::Read { size } => dev.access(offset, VirtioPciGpuOp::Read { size }, mem),
            MmioOp::Write { size, value } => {
                dev.access(offset, VirtioPciGpuOp::Write { size, value }, mem)
            }
        };
        dev.drain_completed_fences(mem);
        self.flush_virtio_gpu_pending_msix();
        match result {
            VirtioGpuResult::ReadValue(v) => MmioOutcome::ReadValue(v),
            VirtioGpuResult::WriteAck => MmioOutcome::WriteAck,
        }
    }

    fn virtio_console_msix_access(&mut self, offset: u64, op: MmioOp) -> MmioOutcome {
        let Some(dev) = self.virtio_console.as_mut() else {
            return MmioOutcome::KnownUnimplemented("virtio-console-pci");
        };
        let is_write = matches!(op, MmioOp::Write { .. });
        let result = match op {
            MmioOp::Read { size } => dev.msix_bar_access(offset, VirtioPciConsoleOp::Read { size }),
            MmioOp::Write { size, value } => {
                dev.msix_bar_access(offset, VirtioPciConsoleOp::Write { size, value })
            }
        };
        if is_write {
            self.flush_virtio_console_pending_msix();
        }
        match result {
            VirtioConsoleResult::ReadValue(v) => MmioOutcome::ReadValue(v),
            VirtioConsoleResult::WriteAck => MmioOutcome::WriteAck,
        }
    }

    fn virtio_console_access(
        &mut self,
        offset: u64,
        op: MmioOp,
        mem: &mut dyn GuestMemoryMut,
    ) -> MmioOutcome {
        let Some(dev) = self.virtio_console.as_mut() else {
            return MmioOutcome::KnownUnimplemented("virtio-console-pci");
        };
        let result = match op {
            MmioOp::Read { size } => dev.access(offset, VirtioPciConsoleOp::Read { size }, mem),
            MmioOp::Write { size, value } => {
                dev.access(offset, VirtioPciConsoleOp::Write { size, value }, mem)
            }
        };
        dev.poll(mem);
        self.flush_virtio_console_pending_msix();
        match result {
            VirtioConsoleResult::ReadValue(v) => MmioOutcome::ReadValue(v),
            VirtioConsoleResult::WriteAck => MmioOutcome::WriteAck,
        }
    }

    fn pci_boot_media_legacy_pio_access(
        &mut self,
        offset: u64,
        op: MmioOp,
        mem: &mut dyn GuestMemoryMut,
    ) -> MmioOutcome {
        let Some(dev) = self.pci_boot_media.as_mut() else {
            return MmioOutcome::KnownUnimplemented("virtio-blk-pci");
        };
        let old_irq = dev.interrupt_line_level();
        let result = match op {
            MmioOp::Read { size } => {
                dev.legacy_io_access(offset, VirtioPciBlockOp::Read { size }, mem)
            }
            MmioOp::Write { size, value } => {
                dev.legacy_io_access(offset, VirtioPciBlockOp::Write { size, value }, mem)
            }
        };
        let new_irq = dev.interrupt_line_level();
        if old_irq != new_irq {
            self.pending_spi_levels
                .push((machine::spi_to_intid(machine::SPI_PCIE_INTA), new_irq));
        }
        match result {
            VirtioMmioBlockResult::ReadValue(v) => MmioOutcome::ReadValue(v),
            VirtioMmioBlockResult::WriteAck => MmioOutcome::WriteAck,
        }
    }

    fn nvme_access(
        &mut self,
        offset: u64,
        op: MmioOp,
        mem: &mut dyn GuestMemoryMut,
    ) -> MmioOutcome {
        match op {
            MmioOp::Read { size } => MmioOutcome::ReadValue(self.nvme.mmio_read(offset, size)),
            MmioOp::Write { size, value } => {
                self.nvme.mmio_write(offset, size, value);
                self.nvme_completion_scratch.clear();
                self.nvme
                    .process_into(mem, &mut self.nvme_completion_scratch);
                self.queue_nvme_completion_msix();
                self.flush_nvme_pending_msix();
                MmioOutcome::WriteAck
            }
        }
    }

    fn hda_access(&mut self, offset: u64, op: MmioOp, mem: &mut dyn GuestMemoryMut) -> MmioOutcome {
        let outcome = {
            let Some(hda) = self.hda.as_mut() else {
                return MmioOutcome::KnownUnimplemented("intel-hda");
            };
            match op {
                MmioOp::Read { size } => MmioOutcome::ReadValue(hda.mmio_read(offset, size)),
                MmioOp::Write { size, value } => {
                    hda.mmio_write(offset, size, value, mem);
                    MmioOutcome::WriteAck
                }
            }
        };
        self.flush_hda_pending_msi();
        outcome
    }

    fn queue_nvme_completion_msix(&mut self) {
        let control = self.pcie.nvme_msix_control();
        for completion in &self.nvme_completion_scratch {
            if let Some(message) =
                self.nvme
                    .raise_msix(completion.vector, control.enabled, control.function_masked)
            {
                self.pending_msix.push(message);
            }
        }
        self.nvme_completion_scratch.clear();
    }

    fn flush_nvme_pending_msix(&mut self) {
        let control = self.pcie.nvme_msix_control();
        self.nvme.drain_pending_msix_into(
            control.enabled,
            control.function_masked,
            &mut self.pending_msix,
        );
    }

    fn queue_xhci_completion_msix(&mut self) {
        if !self.devices.xhci_present {
            return;
        }
        let control = self.pcie.xhci_msix_control();
        self.xhci.raise_pending_interrupter_msix_into(
            control.enabled,
            control.function_masked,
            &mut self.pending_msix,
        );
    }

    fn flush_xhci_pending_msix(&mut self) {
        if !self.devices.xhci_present {
            return;
        }
        let control = self.pcie.xhci_msix_control();
        self.xhci.drain_pending_msix_into(
            control.enabled,
            control.function_masked,
            &mut self.pending_msix,
        );
    }

    fn flush_virtio_net_pending_msix(&mut self) {
        if !self.devices.virtio_net_present {
            return;
        }
        let Some(dev) = self.virtio_net.as_mut() else {
            return;
        };
        let control = self.pcie.virtio_net_msix_control();
        dev.drain_pending_msix_into(
            control.enabled,
            control.function_masked,
            &mut self.pending_msix,
        );
    }

    /// Retire host-paced vblank NOPs and venus fences, then flush their
    /// interrupts without guest MMIO. Completion must not depend on the guest
    /// touching the device: a guest blocked in vkWaitForFences sits in WFI
    /// generating no virtio-gpu accesses, so the per-exit drain path calls this
    /// while timer exits keep it running.
    pub fn poll_virtio_gpu_fences(&mut self, mem: &mut dyn GuestMemoryMut) {
        if let Some(dev) = self.virtio_gpu.as_mut() {
            dev.service_deferred_3d_scanout();
            dev.drain_host_vblank(mem);
            dev.drain_completed_fences(mem);
            self.flush_virtio_gpu_pending_msix();
        }
    }

    /// Advance the host-clock-paced HDA playback stream and flush standard MSI raised
    /// by IOC or DMA errors into the platform's pending-message aggregation.
    pub fn poll_hda(&mut self, mem: &mut dyn GuestMemoryMut) {
        if let Some(hda) = self.hda.as_mut() {
            hda.poll(mem, self.host_now);
        }
        self.flush_hda_pending_msi();
    }

    /// Install or clear the host PCM sink for the optional HDA controller.
    pub fn set_hda_pcm_sink(&mut self, sink: Option<Box<dyn HdaPcmSink>>) {
        if let Some(hda) = self.hda.as_mut() {
            hda.set_pcm_sink(sink);
        }
    }

    fn flush_hda_pending_msi(&mut self) {
        if !self.devices.hda_present {
            return;
        }
        let config = self.pcie.hda_msi_config();
        let Some(hda) = self.hda.as_mut() else {
            return;
        };
        hda.drain_pending_msi_into(
            config.enabled,
            config.address,
            config.data,
            &mut self.pending_msix,
        );
    }

    fn flush_virtio_gpu_pending_msix(&mut self) {
        if !self.devices.virtio_gpu_present {
            return;
        }
        let Some(dev) = self.virtio_gpu.as_mut() else {
            return;
        };
        let control = self.pcie.virtio_gpu_msix_control();
        dev.drain_pending_msix_into(
            control.enabled,
            control.function_masked,
            &mut self.pending_msix,
        );
    }

    fn flush_virtio_console_pending_msix(&mut self) {
        if !self.devices.virtio_console_present {
            return;
        }
        let Some(dev) = self.virtio_console.as_mut() else {
            return;
        };
        let control = self.pcie.virtio_console_msix_control();
        dev.drain_pending_msix_into(
            control.enabled,
            control.function_masked,
            &mut self.pending_msix,
        );
    }

    fn uart_access(&mut self, offset: u64, op: MmioOp) -> MmioOutcome {
        match op {
            MmioOp::Read { size } => MmioOutcome::ReadValue(self.uart.mmio_read(offset, size)),
            MmioOp::Write { size, value } => {
                self.uart.mmio_write(offset, size, value);
                MmioOutcome::WriteAck
            }
        }
    }

    fn rtc_access(&mut self, offset: u64, op: MmioOp) -> MmioOutcome {
        match op {
            MmioOp::Read { size } => MmioOutcome::ReadValue(self.rtc.mmio_read(offset, size)),
            MmioOp::Write { size, value } => {
                self.rtc.mmio_write(offset, size, value);
                MmioOutcome::WriteAck
            }
        }
    }

    /// Bytes the guest/firmware has written to the UART so far.
    pub fn uart_output(&self) -> &[u8] {
        self.uart.output()
    }

    /// Drain and return everything the guest has written to the UART since the
    /// last drain. Used by the KD serial bridge to forward the guest's
    /// KDCOM/serial-debug transmit stream to a host socket; unlike
    /// `uart_output()` (a non-draining borrow the boot scanner reads) this
    /// consumes the buffer so bytes are forwarded exactly once.
    pub fn take_uart_output(&mut self) -> Vec<u8> {
        self.uart.take_output()
    }

    /// Queue bytes that the guest can read from the PL011 UART data register.
    /// Live probes use this to test firmware/loader input paths while the default
    /// platform remains an unattached, receive-empty serial backend.
    pub fn push_uart_input(&mut self, bytes: &[u8]) {
        self.uart.push_input(bytes);
    }

    /// Number of preloaded PL011 input bytes still waiting to be read.
    pub fn uart_input_len(&self) -> usize {
        self.uart.input_len()
    }

    fn fw_cfg_access(
        &mut self,
        offset: u64,
        op: MmioOp,
        mem: &mut dyn GuestMemoryMut,
    ) -> MmioOutcome {
        match op {
            MmioOp::Read { size } => MmioOutcome::ReadValue(self.fw_cfg.mmio_read(offset, size)),
            MmioOp::Write { size, value } => {
                self.fw_cfg.mmio_write(offset, size, value, mem);
                self.refresh_ramfb();
                MmioOutcome::WriteAck
            }
        }
    }

    fn refresh_ramfb(&mut self) {
        if !self.devices.ramfb_present {
            return;
        }
        if let Some(bytes) = self.fw_cfg.file_bytes(RAMFB_FW_CFG_FILE) {
            self.ramfb.update_from_fw_cfg(bytes);
        }
    }

    pub fn snapshot_state(&self) -> Vec<u8> {
        let mut out = crate::checkpoint::StateWriter::new();
        out.write_u32(1);

        out.write_blob(&self.pcie.snapshot_state());
        out.write_blob(&self.nvme.snapshot_state());
        out.write_blob(&self.flash_vars.snapshot_state());

        out.write_bool(self.virtio_iso.is_some());
        if let Some(device) = &self.virtio_iso {
            out.write_blob(&device.snapshot_state());
        }

        out.write_bool(self.pci_boot_media.is_some());
        if let Some(device) = &self.pci_boot_media {
            out.write_blob(&device.snapshot_state());
        }

        out.write_bool(self.virtio_net.is_some());
        if let Some(device) = &self.virtio_net {
            out.write_blob(&device.snapshot_state());
        }

        out.write_bool(self.virtio_gpu.is_some());
        if let Some(device) = &self.virtio_gpu {
            out.write_blob(&device.snapshot_state());
        }

        out.write_bool(self.virtio_console.is_some());
        if let Some(device) = &self.virtio_console {
            out.write_blob(&device.snapshot_state());
        }

        out.write_u32(self.pending_msix.len() as u32);
        for message in &self.pending_msix {
            out.write_u16(message.vector);
            out.write_u16(0);
            out.write_u32(message.data);
            out.write_u64(message.address);
        }

        out.write_u32(self.pending_spi_levels.len() as u32);
        for &(intid, level) in &self.pending_spi_levels {
            out.write_u32(intid);
            out.write_bool(level);
            out.write_u8(0);
            out.write_u16(0);
        }

        out.write_bool(self.nvme_ecam_touched);
        out.write_bool(self.nvme_mmio_reached);
        out.write_bool(self.nvme_cc_enabled);
        out.write_bool(self.nvme_admin_doorbell_rung);
        out.into_inner()
    }

    pub fn restore_state(&mut self, data: &[u8]) {
        let mut input = crate::checkpoint::StateReader::new(data);
        assert_eq!(
            input.read_u32(),
            1,
            "unsupported VirtPlatform snapshot version"
        );

        self.pcie.restore_state(&input.read_blob());
        self.nvme.restore_state(&input.read_blob());
        self.flash_vars.restore_state(&input.read_blob());

        let has_virtio_iso = input.read_bool();
        assert_eq!(
            has_virtio_iso,
            self.virtio_iso.is_some(),
            "legacy virtio ISO attachment mismatch on restore"
        );
        if let Some(device) = self.virtio_iso.as_mut() {
            device.restore_state(&input.read_blob());
        }

        let has_pci_boot_media = input.read_bool();
        assert_eq!(
            has_pci_boot_media,
            self.pci_boot_media.is_some(),
            "PCI boot-media attachment mismatch on restore"
        );
        if let Some(device) = self.pci_boot_media.as_mut() {
            device.restore_state(&input.read_blob());
        }

        let has_net = input.read_bool();
        assert_eq!(
            has_net,
            self.virtio_net.is_some(),
            "virtio-net presence mismatch on restore"
        );
        if let Some(device) = self.virtio_net.as_mut() {
            device.restore_state(&input.read_blob());
        }

        let has_gpu = input.read_bool();
        assert_eq!(
            has_gpu,
            self.virtio_gpu.is_some(),
            "virtio-gpu presence mismatch on restore"
        );
        if let Some(device) = self.virtio_gpu.as_mut() {
            device.restore_state(&input.read_blob());
        }

        let has_console = input.read_bool();
        assert_eq!(
            has_console,
            self.virtio_console.is_some(),
            "virtio-console presence mismatch on restore"
        );
        if let Some(device) = self.virtio_console.as_mut() {
            device.restore_state(&input.read_blob());
        }

        self.pending_msix.clear();
        let pending_msix = input.read_u32() as usize;
        self.pending_msix.reserve(pending_msix);
        for _ in 0..pending_msix {
            let vector = input.read_u16();
            assert_eq!(input.read_u16(), 0, "invalid pending MSI-X snapshot");
            let data = input.read_u32();
            let address = input.read_u64();
            self.pending_msix.push(MsixMessage {
                vector,
                address,
                data,
            });
        }

        self.pending_spi_levels.clear();
        let pending_spis = input.read_u32() as usize;
        self.pending_spi_levels.reserve(pending_spis);
        for _ in 0..pending_spis {
            let intid = input.read_u32();
            let level = input.read_bool();
            assert_eq!(input.read_u8(), 0, "invalid pending SPI snapshot");
            assert_eq!(input.read_u16(), 0, "invalid pending SPI snapshot");
            self.pending_spi_levels.push((intid, level));
        }

        self.nvme_ecam_touched = input.read_bool();
        self.nvme_mmio_reached = input.read_bool();
        self.nvme_cc_enabled = input.read_bool();
        self.nvme_admin_doorbell_rung = input.read_bool();

        self.nvme_completion_scratch.clear();
        self.host_now = None;
        self.xhci_dci3_last_emission = None;
        self.xhci_dci5_last_emission = None;
        input.finish();
    }
}

/// A flat span of guest RAM implementing [`GuestMemoryMut`]. In live use the run
/// loop supplies a view over the HVF-mapped guest memory instead; this is the
/// host-side stand-in used for tests and offline pipeline exercises.
#[derive(Debug)]
pub struct FlatGuestRam {
    base: u64,
    bytes: Vec<u8>,
}

impl FlatGuestRam {
    pub fn new(base: u64, len: usize) -> Self {
        Self {
            base,
            bytes: vec![0u8; len],
        }
    }
    fn offset(&self, gpa: u64) -> Option<usize> {
        gpa.checked_sub(self.base)
            .and_then(|value| usize::try_from(value).ok())
    }
}

impl GuestMemoryMut for FlatGuestRam {
    fn write_bytes(&mut self, gpa: u64, data: &[u8]) -> bool {
        let Some(start) = self.offset(gpa) else {
            return false;
        };
        let Some(end) = start.checked_add(data.len()) else {
            return false;
        };
        if end > self.bytes.len() {
            return false;
        }
        self.bytes[start..end].copy_from_slice(data);
        true
    }
    fn read_bytes(&self, gpa: u64, len: usize) -> Option<Vec<u8>> {
        let start = self.offset(gpa)?;
        let end = start.checked_add(len)?;
        if end > self.bytes.len() {
            return None;
        }
        Some(self.bytes[start..end].to_vec())
    }

    fn read_into(&self, gpa: u64, dst: &mut [u8]) -> bool {
        let Some(start) = self.offset(gpa) else {
            return false;
        };
        let Some(end) = start.checked_add(dst.len()) else {
            return false;
        };
        if end > self.bytes.len() {
            return false;
        }
        dst.copy_from_slice(&self.bytes[start..end]);
        true
    }

    fn host_ptr(&self, gpa: u64, len: usize) -> Option<*mut u8> {
        let start = self.offset(gpa)?;
        let end = start.checked_add(len)?;
        if end > self.bytes.len() {
            return None;
        }
        Some(self.bytes.as_ptr().wrapping_add(start) as *mut u8)
    }
}

/// Report-pacing decision. A zero interval or a not-yet-emitted endpoint always
/// permits the next report; otherwise the caller must wait until `interval` has
/// elapsed since `last_emission`. Kept as a free function so the gate is unit
/// tested deterministically with synthetic `Instant`s.
fn report_pacing_allows_emission(
    interval: Duration,
    last_emission: Option<Instant>,
    now: Instant,
) -> bool {
    if interval.is_zero() {
        return true;
    }
    match last_emission {
        None => true,
        Some(last) => now.saturating_duration_since(last) >= interval,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fwcfg::{DMA_CTL_READ, DMA_CTL_SELECT, DMA_CTL_WRITE, KEY_FILE_DIR, KEY_SIGNATURE};
    use crate::machine;
    use crate::ramfb::{DRM_FORMAT_XRGB8888, RAMFB_CONFIG_SIZE};
    use std::{fs, path::PathBuf, time::SystemTime};

    const REG_DATA: u64 = 0x0;
    const REG_SELECTOR: u64 = 0x8;
    const REG_DMA: u64 = 0x10;
    const NET_COMMON_QUEUE_SELECT: u64 = 0x16;
    const NET_COMMON_QUEUE_SIZE: u64 = 0x18;
    const NET_COMMON_QUEUE_MSIX_VECTOR: u64 = 0x1a;
    const NET_COMMON_QUEUE_ENABLE: u64 = 0x1c;
    const NET_COMMON_QUEUE_DESC: u64 = 0x20;
    const NET_COMMON_QUEUE_DRIVER: u64 = 0x28;
    const NET_COMMON_QUEUE_DEVICE: u64 = 0x30;
    const NET_NOTIFY_CFG_OFFSET: u64 = 0x3000;
    const NET_TX_QUEUE: u16 = 1;
    const NET_VIRTIO_HDR_LEN: usize = 12;
    const NET_DESC_F_NEXT: u16 = 1;

    fn platform() -> VirtPlatform {
        VirtPlatform::new(VirtFdtConfig::default())
    }

    fn platform_with_ramfb() -> VirtPlatform {
        VirtPlatform::new_with_ramfb(VirtFdtConfig::default())
    }

    fn platform_with_devices(devices: VirtPlatformDeviceConfig) -> VirtPlatform {
        VirtPlatform::new_with_config(VirtPlatformConfig {
            fdt: VirtFdtConfig::default(),
            devices,
        })
    }

    #[derive(Debug)]
    struct TestTpmBackend;

    impl crate::tpm_tis::Tpm2Backend for TestTpmBackend {
        fn execute(
            &mut self,
            _locality: u8,
            _command: &[u8],
        ) -> Result<Vec<u8>, crate::tpm_tis::TpmBackendError> {
            Ok(vec![0x80, 0x01, 0, 0, 0, 10, 0, 0, 0, 0])
        }
    }

    #[test]
    fn tpm_presence_wires_mmio_and_acpi_as_one_contract() {
        let mut devices = VirtPlatformDeviceConfig::default();
        devices.tpm_tis_present = true;
        let mut p = VirtPlatform::new_with_config_and_tpm_backend(
            VirtPlatformConfig {
                fdt: VirtFdtConfig::default(),
                devices,
            },
            Some(Box::new(TestTpmBackend)),
        );
        let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0);

        assert_eq!(
            p.on_mmio(
                machine::TPM_TIS.base + crate::tpm_tis::REG_DID_VID,
                MmioOp::Read { size: 4 },
                &mut mem,
            ),
            MmioOutcome::ReadValue(0x0001_1014)
        );
        assert_eq!(p.tpm_tis_stats(), Some(TpmTisStats::default()));

        let (selector, size) = fw_cfg_file_entry(&mut p, ACPI_TABLE_FILE.as_bytes());
        p.fw_cfg.select(selector);
        let acpi_tables = p.fw_cfg.read_data(size);
        assert!(acpi_tables.windows(4).any(|bytes| bytes == b"TPM0"));
        assert!(acpi_tables.windows(8).any(|bytes| bytes == b"MSFT0101"));
        let (_, tpm_log_size) = fw_cfg_file_entry(&mut p, ACPI_TPM_LOG_FILE.as_bytes());
        assert_eq!(tpm_log_size, crate::acpi::TPM_LOG_AREA_MINIMUM_SIZE);
        let (ppi_config_selector, ppi_config_size) =
            fw_cfg_file_entry(&mut p, TPM_PPI_FW_CFG_FILE.as_bytes());
        assert_eq!(ppi_config_size, crate::tpm_ppi::TPM_PPI_FW_CFG_CONFIG_SIZE);
        p.fw_cfg.select(ppi_config_selector);
        assert_eq!(
            p.fw_cfg.read_data(ppi_config_size),
            build_qemu_fw_cfg_tpm_config(machine::TPM_PPI.base as u32)
        );
        assert_eq!(
            p.on_mmio(
                machine::TPM_PPI.base + crate::tpm_ppi::PPRQ_OFFSET as u64,
                MmioOp::Write { size: 4, value: 23 },
                &mut mem,
            ),
            MmioOutcome::WriteAck
        );
        assert_eq!(
            p.on_mmio(
                machine::TPM_PPI.base + crate::tpm_ppi::PPRQ_OFFSET as u64,
                MmioOp::Read { size: 4 },
                &mut mem,
            ),
            MmioOutcome::ReadValue(23)
        );
        assert_eq!(
            p.tpm_ppi_stats(),
            Some(TpmPpiStats {
                reads: 1,
                writes: 1,
                rejected_accesses: 0,
            })
        );
    }

    #[test]
    fn disabled_tpm_omits_all_tpm_fw_cfg_discovery_files() {
        let mut p = platform();

        assert_eq!(
            find_fw_cfg_file_entry(&mut p, ACPI_TPM_LOG_FILE.as_bytes()),
            None
        );
        assert_eq!(
            find_fw_cfg_file_entry(&mut p, TPM_PPI_FW_CFG_FILE.as_bytes()),
            None
        );
    }

    fn pcie_cfg_gpa(device: u8, function: u8, reg: u16) -> u64 {
        machine::PCIE_ECAM.base
            + (u64::from(device) << 15)
            + (u64::from(function) << 12)
            + u64::from(reg)
    }

    #[test]
    fn pending_irq_drains_preserve_internal_capacity() {
        let mut p = platform();
        let message = crate::msix::MsixMessage {
            vector: 7,
            address: machine::GIC_ITS.base + 0x40,
            data: 42,
        };

        p.pending_msix.reserve(8);
        p.pending_msix.push(message);
        let msix_capacity = p.pending_msix.capacity();
        assert_eq!(p.take_pending_msix(), vec![message]);
        assert!(p.take_pending_msix().is_empty());
        assert_eq!(p.pending_msix.capacity(), msix_capacity);

        p.pending_spi_levels.reserve(8);
        p.pending_spi_levels.push((machine::spi_to_intid(7), true));
        let spi_capacity = p.pending_spi_levels.capacity();
        assert_eq!(
            p.take_pending_spi_levels(),
            vec![(machine::spi_to_intid(7), true)]
        );
        assert!(p.take_pending_spi_levels().is_empty());
        assert_eq!(p.pending_spi_levels.capacity(), spi_capacity);
    }

    #[test]
    fn pending_irq_drain_into_reuses_caller_capacity() {
        let mut p = platform();
        let message = crate::msix::MsixMessage {
            vector: 7,
            address: machine::GIC_ITS.base + 0x40,
            data: 42,
        };

        p.pending_msix.reserve(8);
        p.pending_msix.push(message);
        let msix_internal_capacity = p.pending_msix.capacity();
        let mut msix_out = Vec::with_capacity(8);
        let msix_out_capacity = msix_out.capacity();
        let msix_out_ptr = msix_out.as_ptr();
        p.drain_pending_msix_into(&mut msix_out);
        assert_eq!(msix_out, vec![message]);
        assert_eq!(msix_out.capacity(), msix_out_capacity);
        assert_eq!(msix_out.as_ptr(), msix_out_ptr);
        assert_eq!(p.pending_msix.capacity(), msix_internal_capacity);
        msix_out.clear();
        p.drain_pending_msix_into(&mut msix_out);
        assert!(msix_out.is_empty());
        assert_eq!(msix_out.capacity(), msix_out_capacity);

        p.pending_spi_levels.reserve(8);
        p.pending_spi_levels.push((machine::spi_to_intid(7), true));
        let spi_internal_capacity = p.pending_spi_levels.capacity();
        let mut spi_out = Vec::with_capacity(8);
        let spi_out_capacity = spi_out.capacity();
        let spi_out_ptr = spi_out.as_ptr();
        p.drain_pending_spi_levels_into(&mut spi_out);
        assert_eq!(spi_out, vec![(machine::spi_to_intid(7), true)]);
        assert_eq!(spi_out.capacity(), spi_out_capacity);
        assert_eq!(spi_out.as_ptr(), spi_out_ptr);
        assert_eq!(p.pending_spi_levels.capacity(), spi_internal_capacity);
        spi_out.clear();
        p.drain_pending_spi_levels_into(&mut spi_out);
        assert!(spi_out.is_empty());
        assert_eq!(spi_out.capacity(), spi_out_capacity);
    }

    #[test]
    fn on_mmio_with_post_drain_reports_setup_input_attempts() {
        let mut p = platform();
        let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0);

        let (outcome, post_drain) =
            p.on_mmio_with_post_drain(machine::UART.base, MmioOp::Read { size: 4 }, &mut mem);
        assert!(matches!(outcome, MmioOutcome::ReadValue(_)));
        assert!(post_drain.xhci_setup_input_attempted());

        let (outcome, post_drain) = p.on_mmio_with_post_drain(
            machine::RAM_BASE - 0x1000,
            MmioOp::Read { size: 4 },
            &mut mem,
        );
        assert_eq!(outcome, MmioOutcome::Unmapped);
        assert!(!post_drain.xhci_setup_input_attempted());
    }

    #[test]
    fn on_mmio_with_post_drain_skips_setup_input_for_xhci_bar0() {
        let mut p = platform();
        let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0);
        let xhci_base = machine::PCIE_MMIO_32.base + 0x2_0000;

        for (reg, size, value) in [
            (crate::pcie::REG_BAR0, 4, xhci_base),
            (
                crate::pcie::REG_COMMAND_STATUS,
                2,
                u64::from(crate::pcie::CMD_MEMORY_SPACE | crate::pcie::CMD_BUS_MASTER),
            ),
        ] {
            assert_eq!(
                p.on_mmio(
                    pcie_cfg_gpa(crate::pcie::XHCI_BDF.1, crate::pcie::XHCI_BDF.2, reg),
                    MmioOp::Write { size, value },
                    &mut mem,
                ),
                MmioOutcome::WriteAck
            );
        }

        let (outcome, post_drain) =
            p.on_mmio_with_post_drain(xhci_base, MmioOp::Read { size: 4 }, &mut mem);
        assert!(matches!(outcome, MmioOutcome::ReadValue(_)));
        assert!(!post_drain.xhci_setup_input_attempted());
    }

    fn temp_path(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "bridgevm-hvf-platform-virt-{name}-{}-{nanos}",
            std::process::id()
        ))
    }

    fn write_vring_desc(
        mem: &mut FlatGuestRam,
        table: u64,
        index: u16,
        addr: u64,
        len: u32,
        flags: u16,
        next: u16,
    ) {
        let gpa = table + u64::from(index) * 16;
        assert!(mem.write_bytes(gpa, &addr.to_le_bytes()));
        assert!(mem.write_bytes(gpa + 8, &len.to_le_bytes()));
        assert!(mem.write_bytes(gpa + 12, &flags.to_le_bytes()));
        assert!(mem.write_bytes(gpa + 14, &next.to_le_bytes()));
    }

    fn write_valid_ramfb_config(p: &mut VirtPlatform, mem: &mut FlatGuestRam) {
        let (selector, size) = fw_cfg_file_entry(p, b"etc/ramfb");
        let src = machine::RAM_BASE + 0x100;
        let ctrl = machine::RAM_BASE + 0x200;
        let mut config = [0u8; RAMFB_CONFIG_SIZE];
        config[0..8].copy_from_slice(&0x4010_0000u64.to_be_bytes());
        config[8..12].copy_from_slice(&DRM_FORMAT_XRGB8888.to_be_bytes());
        config[12..16].copy_from_slice(&0u32.to_be_bytes());
        config[16..20].copy_from_slice(&1024u32.to_be_bytes());
        config[20..24].copy_from_slice(&768u32.to_be_bytes());
        config[24..28].copy_from_slice(&(1024u32 * 4).to_be_bytes());
        let control = (u32::from(selector) << 16) | DMA_CTL_SELECT | DMA_CTL_WRITE;
        let mut dma = Vec::new();
        dma.extend_from_slice(&control.to_be_bytes());
        dma.extend_from_slice(&(size as u32).to_be_bytes());
        dma.extend_from_slice(&src.to_be_bytes());
        assert!(mem.write_bytes(src, &config));
        assert!(mem.write_bytes(ctrl, &dma));

        assert_eq!(
            p.on_mmio(
                machine::FW_CFG.base + REG_DMA,
                MmioOp::Write {
                    size: 8,
                    value: ctrl.swap_bytes(),
                },
                mem,
            ),
            MmioOutcome::WriteAck
        );
    }

    fn read_virtio_iso_sector(
        p: &mut VirtPlatform,
        mem: &mut FlatGuestRam,
        sector: u64,
        expected_prefix_len: usize,
    ) -> Vec<u8> {
        const REG_GUEST_PAGE_SIZE: u64 = 0x28;
        const REG_QUEUE_NUM: u64 = 0x38;
        const REG_QUEUE_ALIGN: u64 = 0x3c;
        const REG_QUEUE_PFN: u64 = 0x40;
        const REG_QUEUE_NOTIFY: u64 = 0x50;
        const DESC_F_NEXT: u16 = 1;
        const DESC_F_WRITE: u16 = 2;
        const VIRTIO_BLK_T_IN: u32 = 0;

        let slot_base = machine::virtio_mmio_slot(INSTALLER_ISO_SLOT).base;
        let desc = machine::RAM_BASE + 0x9000;
        let avail = desc + 8 * 16;
        let header = machine::RAM_BASE + 0xb000;
        let data = machine::RAM_BASE + 0xc000;
        let status = machine::RAM_BASE + 0xd000;
        assert!(mem.write_bytes(header, &VIRTIO_BLK_T_IN.to_le_bytes()));
        assert!(mem.write_bytes(header + 8, &sector.to_le_bytes()));
        write_vring_desc(mem, desc, 0, header, 16, DESC_F_NEXT, 1);
        write_vring_desc(mem, desc, 1, data, 512, DESC_F_NEXT | DESC_F_WRITE, 2);
        write_vring_desc(mem, desc, 2, status, 1, DESC_F_WRITE, 0);
        assert!(mem.write_bytes(avail + 2, &1u16.to_le_bytes()));
        assert!(mem.write_bytes(avail + 4, &0u16.to_le_bytes()));

        for (reg, value) in [
            (REG_QUEUE_NUM, 8),
            (REG_GUEST_PAGE_SIZE, 4096),
            (REG_QUEUE_ALIGN, 4096),
            (REG_QUEUE_PFN, desc >> 12),
        ] {
            assert_eq!(
                p.on_mmio(slot_base + reg, MmioOp::Write { size: 4, value }, mem),
                MmioOutcome::WriteAck
            );
        }
        assert_eq!(
            p.on_mmio(
                slot_base + REG_QUEUE_NOTIFY,
                MmioOp::Write { size: 4, value: 0 },
                mem,
            ),
            MmioOutcome::WriteAck
        );

        mem.read_bytes(data, expected_prefix_len).unwrap()
    }

    fn read_pci_boot_media_sector(
        p: &mut VirtPlatform,
        mem: &mut FlatGuestRam,
        bar: u64,
        sector: u64,
        expected_prefix_len: usize,
    ) -> Vec<u8> {
        const PCI_NOTIFY_CFG_OFFSET: u64 = 0x3000;
        const REG_QUEUE_NUM: u64 = 0x038;
        const REG_QUEUE_READY: u64 = 0x044;
        const REG_QUEUE_NOTIFY: u64 = 0x050;
        const REG_QUEUE_DESC_LOW: u64 = 0x080;
        const REG_QUEUE_DRIVER_LOW: u64 = 0x090;
        const REG_QUEUE_DEVICE_LOW: u64 = 0x0a0;
        const DESC_F_NEXT: u16 = 1;
        const DESC_F_WRITE: u16 = 2;
        const VIRTIO_BLK_T_IN: u32 = 0;

        let desc = machine::RAM_BASE + 0x10000;
        let avail = machine::RAM_BASE + 0x11000;
        let used = machine::RAM_BASE + 0x12000;
        let header = machine::RAM_BASE + 0x13000;
        let data = machine::RAM_BASE + 0x14000;
        let status = machine::RAM_BASE + 0x15000;
        assert!(mem.write_bytes(header, &VIRTIO_BLK_T_IN.to_le_bytes()));
        assert!(mem.write_bytes(header + 8, &sector.to_le_bytes()));
        write_vring_desc(mem, desc, 0, header, 16, DESC_F_NEXT, 1);
        write_vring_desc(mem, desc, 1, data, 512, DESC_F_NEXT | DESC_F_WRITE, 2);
        write_vring_desc(mem, desc, 2, status, 1, DESC_F_WRITE, 0);
        assert!(mem.write_bytes(avail + 2, &1u16.to_le_bytes()));
        assert!(mem.write_bytes(avail + 4, &0u16.to_le_bytes()));

        for (reg, value) in [
            (REG_QUEUE_NUM, 8),
            (REG_QUEUE_DESC_LOW, desc),
            (REG_QUEUE_DRIVER_LOW, avail),
            (REG_QUEUE_DEVICE_LOW, used),
            (REG_QUEUE_READY, 1),
        ] {
            assert_eq!(
                p.on_mmio(bar + reg, MmioOp::Write { size: 4, value }, mem),
                MmioOutcome::WriteAck
            );
        }
        assert_eq!(
            p.on_mmio(
                bar + PCI_NOTIFY_CFG_OFFSET + REG_QUEUE_NOTIFY,
                MmioOp::Write { size: 4, value: 0 },
                mem,
            ),
            MmioOutcome::WriteAck
        );

        mem.read_bytes(data, expected_prefix_len).unwrap()
    }

    fn program_nvme_bar0(p: &mut VirtPlatform, mem: &mut FlatGuestRam) {
        p.on_mmio(
            pcie_cfg_gpa(1, 0, crate::pcie::REG_BAR0),
            MmioOp::Write {
                size: 4,
                value: machine::PCIE_MMIO_32.base,
            },
            mem,
        );
        p.on_mmio(
            pcie_cfg_gpa(1, 0, crate::pcie::REG_COMMAND_STATUS),
            MmioOp::Write {
                size: 2,
                value: u64::from(crate::pcie::CMD_MEMORY_SPACE | crate::pcie::CMD_BUS_MASTER),
            },
            mem,
        );
    }

    fn program_virtio_blk_bar4(p: &mut VirtPlatform, mem: &mut FlatGuestRam, base: u64) {
        p.on_mmio(
            pcie_cfg_gpa(3, 0, crate::pcie::REG_BAR0 + 4 * 4),
            MmioOp::Write {
                size: 4,
                value: base,
            },
            mem,
        );
        p.on_mmio(
            pcie_cfg_gpa(3, 0, crate::pcie::REG_COMMAND_STATUS),
            MmioOp::Write {
                size: 2,
                value: u64::from(crate::pcie::CMD_MEMORY_SPACE | crate::pcie::CMD_BUS_MASTER),
            },
            mem,
        );
    }

    fn program_virtio_blk_bar1(p: &mut VirtPlatform, mem: &mut FlatGuestRam, base: u64) {
        p.on_mmio(
            pcie_cfg_gpa(3, 0, crate::pcie::REG_BAR0 + 4),
            MmioOp::Write {
                size: 4,
                value: base,
            },
            mem,
        );
        p.on_mmio(
            pcie_cfg_gpa(3, 0, crate::pcie::REG_COMMAND_STATUS),
            MmioOp::Write {
                size: 2,
                value: u64::from(crate::pcie::CMD_MEMORY_SPACE | crate::pcie::CMD_BUS_MASTER),
            },
            mem,
        );
    }

    fn program_virtio_blk_bar0_pio(p: &mut VirtPlatform, mem: &mut FlatGuestRam, port: u64) {
        p.on_mmio(
            pcie_cfg_gpa(3, 0, crate::pcie::REG_BAR0),
            MmioOp::Write {
                size: 4,
                value: port,
            },
            mem,
        );
        p.on_mmio(
            pcie_cfg_gpa(3, 0, crate::pcie::REG_COMMAND_STATUS),
            MmioOp::Write {
                size: 2,
                value: u64::from(crate::pcie::CMD_IO_SPACE | crate::pcie::CMD_BUS_MASTER),
            },
            mem,
        );
    }

    fn program_virtio_net_bar4(p: &mut VirtPlatform, mem: &mut FlatGuestRam, base: u64) {
        p.on_mmio(
            pcie_cfg_gpa(
                crate::pcie::VIRTIO_NET_BDF.1,
                crate::pcie::VIRTIO_NET_BDF.2,
                crate::pcie::REG_BAR0 + 4 * 4,
            ),
            MmioOp::Write {
                size: 4,
                value: base,
            },
            mem,
        );
        p.on_mmio(
            pcie_cfg_gpa(
                crate::pcie::VIRTIO_NET_BDF.1,
                crate::pcie::VIRTIO_NET_BDF.2,
                crate::pcie::REG_COMMAND_STATUS,
            ),
            MmioOp::Write {
                size: 2,
                value: u64::from(crate::pcie::CMD_MEMORY_SPACE | crate::pcie::CMD_BUS_MASTER),
            },
            mem,
        );
    }

    fn program_virtio_net_bar1(p: &mut VirtPlatform, mem: &mut FlatGuestRam, base: u64) {
        p.on_mmio(
            pcie_cfg_gpa(
                crate::pcie::VIRTIO_NET_BDF.1,
                crate::pcie::VIRTIO_NET_BDF.2,
                crate::pcie::REG_BAR0 + 4,
            ),
            MmioOp::Write {
                size: 4,
                value: base,
            },
            mem,
        );
        p.on_mmio(
            pcie_cfg_gpa(
                crate::pcie::VIRTIO_NET_BDF.1,
                crate::pcie::VIRTIO_NET_BDF.2,
                crate::pcie::REG_COMMAND_STATUS,
            ),
            MmioOp::Write {
                size: 2,
                value: u64::from(crate::pcie::CMD_MEMORY_SPACE | crate::pcie::CMD_BUS_MASTER),
            },
            mem,
        );
    }

    fn virtio_net_write(
        p: &mut VirtPlatform,
        mem: &mut FlatGuestRam,
        bar: u64,
        offset: u64,
        size: u8,
        value: u64,
    ) {
        assert_eq!(
            p.on_mmio(bar + offset, MmioOp::Write { size, value }, mem),
            MmioOutcome::WriteAck
        );
    }

    struct TestVirtQueue {
        desc: u64,
        avail: u64,
        used: u64,
    }

    fn setup_virtio_net_queue(
        p: &mut VirtPlatform,
        mem: &mut FlatGuestRam,
        bar: u64,
        queue: u16,
        layout: TestVirtQueue,
        vector: u16,
    ) {
        let TestVirtQueue { desc, avail, used } = layout;
        for (offset, size, value) in [
            (NET_COMMON_QUEUE_SELECT, 2, u64::from(queue)),
            (NET_COMMON_QUEUE_SIZE, 2, 8),
            (NET_COMMON_QUEUE_MSIX_VECTOR, 2, u64::from(vector)),
            (NET_COMMON_QUEUE_DESC, 8, desc),
            (NET_COMMON_QUEUE_DRIVER, 8, avail),
            (NET_COMMON_QUEUE_DEVICE, 8, used),
            (NET_COMMON_QUEUE_ENABLE, 2, 1),
        ] {
            virtio_net_write(p, mem, bar, offset, size, value);
        }
    }

    fn enable_virtio_net_msix_vector(
        p: &mut VirtPlatform,
        mem: &mut FlatGuestRam,
        bar1: u64,
        vector: u16,
        address: u64,
        data: u32,
    ) {
        let entry = bar1 + u64::from(vector) * 16;
        assert_eq!(
            p.on_mmio(
                entry,
                MmioOp::Write {
                    size: 8,
                    value: address,
                },
                mem,
            ),
            MmioOutcome::WriteAck
        );
        assert_eq!(
            p.on_mmio(
                entry + 8,
                MmioOp::Write {
                    size: 4,
                    value: u64::from(data),
                },
                mem,
            ),
            MmioOutcome::WriteAck
        );
        assert_eq!(
            p.on_mmio(entry + 12, MmioOp::Write { size: 4, value: 0 }, mem),
            MmioOutcome::WriteAck
        );
        assert_eq!(
            p.on_mmio(
                pcie_cfg_gpa(
                    crate::pcie::VIRTIO_NET_BDF.1,
                    crate::pcie::VIRTIO_NET_BDF.2,
                    u16::from(crate::pcie::VIRTIO_NET_MSIX_CAP_OFFSET) + 2,
                ),
                MmioOp::Write {
                    size: 2,
                    value: 0x8000,
                },
                mem,
            ),
            MmioOutcome::WriteAck
        );
    }

    fn encode_nvme_sqe(
        opcode: u8,
        command_id: u16,
        nsid: u32,
        prp1: u64,
        cdw10: u32,
        cdw11: u32,
        cdw12: u32,
    ) -> [u8; 64] {
        let mut e = [0u8; 64];
        let cdw0 = u32::from(opcode) | (u32::from(command_id) << 16);
        e[0..4].copy_from_slice(&cdw0.to_le_bytes());
        e[4..8].copy_from_slice(&nsid.to_le_bytes());
        e[24..32].copy_from_slice(&prp1.to_le_bytes());
        e[40..44].copy_from_slice(&cdw10.to_le_bytes());
        e[44..48].copy_from_slice(&cdw11.to_le_bytes());
        e[48..52].copy_from_slice(&cdw12.to_le_bytes());
        e
    }

    fn find_fw_cfg_file_entry(p: &mut VirtPlatform, name: &[u8]) -> Option<(u16, usize)> {
        p.fw_cfg.select(KEY_FILE_DIR);
        let dir = p.fw_cfg.read_data(p.fw_cfg.file_dir_bytes().len());
        let count = u32::from_be_bytes([dir[0], dir[1], dir[2], dir[3]]) as usize;
        (0..count).find_map(|index| {
            let record = &dir[4 + index * 64..4 + (index + 1) * 64];
            let name_end = record[8..64]
                .iter()
                .position(|&byte| byte == 0)
                .unwrap_or(56);
            (&record[8..8 + name_end] == name).then(|| {
                let size =
                    u32::from_be_bytes([record[0], record[1], record[2], record[3]]) as usize;
                let select = u16::from_be_bytes([record[4], record[5]]);
                (select, size)
            })
        })
    }

    fn fw_cfg_file_entry(p: &mut VirtPlatform, name: &[u8]) -> (u16, usize) {
        find_fw_cfg_file_entry(p, name).unwrap_or_else(|| {
            panic!(
                "default fw_cfg dir missing {}",
                String::from_utf8_lossy(name)
            )
        })
    }

    fn nvme_mmio_write(
        p: &mut VirtPlatform,
        mem: &mut FlatGuestRam,
        offset: u64,
        size: u8,
        value: u64,
    ) {
        assert_eq!(
            p.on_mmio(
                machine::PCIE_MMIO_32.base + offset,
                MmioOp::Write { size, value },
                mem,
            ),
            MmioOutcome::WriteAck
        );
    }

    fn enable_nvme_controller(p: &mut VirtPlatform, mem: &mut FlatGuestRam, asq: u64, acq: u64) {
        let qdepth = 4u64;
        nvme_mmio_write(
            p,
            mem,
            crate::nvme::REG_AQA,
            4,
            ((qdepth - 1) << 16) | (qdepth - 1),
        );
        nvme_mmio_write(p, mem, crate::nvme::REG_ASQ, 8, asq);
        nvme_mmio_write(p, mem, crate::nvme::REG_ACQ, 8, acq);
        assert_eq!(
            p.on_mmio(
                machine::PCIE_MMIO_32.base + crate::nvme::REG_CC,
                MmioOp::Write { size: 4, value: 1 },
                mem,
            ),
            MmioOutcome::WriteAck
        );
    }

    fn enable_nvme_msix_vector0(
        p: &mut VirtPlatform,
        mem: &mut FlatGuestRam,
        address: u64,
        data: u32,
    ) {
        let table = u64::from(crate::pcie::NVME_MSIX_TABLE_OFFSET);
        nvme_mmio_write(p, mem, table, 8, address);
        nvme_mmio_write(p, mem, table + 8, 4, u64::from(data));
        nvme_mmio_write(p, mem, table + 12, 4, 0);
        assert_eq!(
            p.on_mmio(
                pcie_cfg_gpa(1, 0, u16::from(crate::pcie::NVME_MSIX_CAP_OFFSET) + 2),
                MmioOp::Write {
                    size: 2,
                    value: 0x8000,
                },
                mem,
            ),
            MmioOutcome::WriteAck
        );
    }

    fn submit_admin_sqe(
        p: &mut VirtPlatform,
        mem: &mut FlatGuestRam,
        asq: u64,
        slot: u16,
        sqe: &[u8; 64],
    ) {
        assert!(mem.write_bytes(asq + u64::from(slot) * crate::nvme::SQ_ENTRY_SIZE, sqe));
        assert_eq!(
            p.on_mmio(
                machine::PCIE_MMIO_32.base + crate::nvme::REG_DOORBELL_BASE,
                MmioOp::Write {
                    size: 4,
                    value: u64::from(slot + 1),
                },
                mem,
            ),
            MmioOutcome::WriteAck
        );
    }

    #[test]
    fn dtb_is_generated_and_well_formed() {
        let p = platform();
        let dtb = p.dtb();
        assert_eq!(
            u32::from_be_bytes([dtb[0], dtb[1], dtb[2], dtb[3]]),
            0xd00d_feed
        );
    }

    #[test]
    fn memory_layout_is_consistent() {
        let p = platform();
        let l = p.memory_layout();
        assert_eq!(l.flash_code.base, 0x0);
        assert_eq!(l.flash_vars.base, 0x0400_0000);
        assert_eq!(l.ram.base, machine::RAM_BASE);
        assert_eq!(l.dtb_load, machine::RAM_BASE);
        // Flash and RAM must not overlap.
        assert!(!l.flash_vars.overlaps(&l.ram));
        // The DTB must fit inside RAM.
        assert!(l.ram.contains(l.dtb_load));
        assert!(p.dtb().len() as u64 <= l.ram.size);
    }

    #[test]
    fn mmio_routes_fw_cfg_signature_via_the_platform() {
        let mut p = platform();
        let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0);
        // Select SIGNATURE through the platform's MMIO entry point...
        let ack = p.on_mmio(
            machine::FW_CFG.base + REG_SELECTOR,
            MmioOp::Write {
                size: 2,
                value: u64::from(KEY_SIGNATURE),
            },
            &mut mem,
        );
        assert_eq!(ack, MmioOutcome::WriteAck);
        // ...then a 4-byte data read returns the little-endian CPU value for
        // the byte stream "QEMU".
        let v = p.on_mmio(
            machine::FW_CFG.base + REG_DATA,
            MmioOp::Read { size: 4 },
            &mut mem,
        );
        assert_eq!(v, MmioOutcome::ReadValue(0x554d_4551));
    }

    #[test]
    fn mmio_fw_cfg_dma_transfers_through_guest_ram() {
        let mut p = platform();
        let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0x1000);
        let ctrl = machine::RAM_BASE;
        let dst = machine::RAM_BASE + 0x80;
        // Build a FWCfgDmaAccess (big-endian) that selects SIGNATURE and reads
        // 4 bytes into `dst`.
        let control: u32 = (u32::from(KEY_SIGNATURE) << 16) | DMA_CTL_SELECT | DMA_CTL_READ;
        let mut blob = Vec::new();
        blob.extend_from_slice(&control.to_be_bytes());
        blob.extend_from_slice(&4u32.to_be_bytes());
        blob.extend_from_slice(&dst.to_be_bytes());
        mem.write_bytes(ctrl, &blob);
        // Writing the control-structure address to the DMA register runs it. The
        // register is big-endian, so the firmware stores the byte-swapped address.
        let ack = p.on_mmio(
            machine::FW_CFG.base + REG_DMA,
            MmioOp::Write {
                size: 8,
                value: ctrl.swap_bytes(),
            },
            &mut mem,
        );
        assert_eq!(ack, MmioOutcome::WriteAck);
        assert_eq!(mem.read_bytes(dst, 4).unwrap(), b"QEMU");
    }

    #[test]
    fn mmio_classifies_known_and_unmapped_addresses() {
        let mut p = platform();
        let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0);
        // GIC is mapped in the machine map but not yet modelled.
        assert_eq!(
            p.on_mmio(machine::GIC_DIST.base, MmioOp::Read { size: 4 }, &mut mem),
            MmioOutcome::KnownUnimplemented("gic-dist")
        );
        // A hole between GPIO and the virtio block.
        assert_eq!(
            p.on_mmio(0x0905_0000, MmioOp::Read { size: 4 }, &mut mem),
            MmioOutcome::Unmapped
        );
    }

    #[test]
    fn pcie_host_bridge_and_empty_slots() {
        let mut p = platform();
        let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0);
        // 00:00.0 is the host bridge (vendor 0x1b36 / device 0x0008).
        assert_eq!(
            p.on_mmio(machine::PCIE_ECAM.base, MmioOp::Read { size: 4 }, &mut mem),
            MmioOutcome::ReadValue(0x0008_1b36)
        );
        assert_eq!(
            p.on_mmio(
                machine::PCIE_ECAM.base + (4 << 15),
                MmioOp::Read { size: 4 },
                &mut mem
            ),
            MmioOutcome::ReadValue(0xFFFF_FFFF)
        );
    }

    #[test]
    fn platform_device_disable_omits_xhci_from_pci_and_mmio_surfaces() {
        let mut p = platform_with_devices(VirtPlatformDeviceConfig {
            xhci_present: false,
            ..VirtPlatformDeviceConfig::default()
        });
        let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0);
        let xhci_base = machine::PCIE_MMIO_32.base + 0x2_0000;

        assert_eq!(
            p.on_mmio(
                pcie_cfg_gpa(crate::pcie::XHCI_BDF.1, crate::pcie::XHCI_BDF.2, 0),
                MmioOp::Read { size: 4 },
                &mut mem
            ),
            MmioOutcome::ReadValue(crate::pcie::NO_DEVICE)
        );
        for (reg, value) in [
            (crate::pcie::REG_BAR0, xhci_base),
            (crate::pcie::REG_BAR0 + 4, 0),
            (
                crate::pcie::REG_COMMAND_STATUS,
                u64::from(crate::pcie::CMD_MEMORY_SPACE),
            ),
        ] {
            assert_eq!(
                p.on_mmio(
                    pcie_cfg_gpa(crate::pcie::XHCI_BDF.1, crate::pcie::XHCI_BDF.2, reg),
                    MmioOp::Write {
                        size: if reg == crate::pcie::REG_COMMAND_STATUS {
                            2
                        } else {
                            4
                        },
                        value,
                    },
                    &mut mem,
                ),
                MmioOutcome::WriteAck
            );
        }

        assert_eq!(p.pcie_mmio_target(xhci_base), None);
        assert_eq!(
            p.on_mmio(xhci_base, MmioOp::Read { size: 4 }, &mut mem),
            MmioOutcome::KnownUnimplemented("pcie-mmio-32")
        );
        assert!(!String::from_utf8_lossy(p.dtb()).contains("xhci"));
    }

    #[test]
    fn platform_device_disable_omits_virtio_iso_from_dtb_pci_and_mmio_surfaces() {
        let mut p = platform_with_devices(VirtPlatformDeviceConfig {
            virtio_boot_media_present: false,
            legacy_virtio_mmio_present: false,
            ..VirtPlatformDeviceConfig::default()
        });
        let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0);
        let pci_bar = machine::PCIE_MMIO_32.base + 0x8_0000;
        let legacy_slot = machine::virtio_mmio_slot(INSTALLER_ISO_SLOT);
        let dtb_body = String::from_utf8_lossy(p.dtb());

        assert!(!dtb_body.contains("virtio_mmio@"));
        assert_eq!(find_fw_cfg_file_entry(&mut p, b"bootorder"), None);
        assert_eq!(
            p.on_mmio(
                pcie_cfg_gpa(
                    crate::pcie::VIRTIO_BLK_BDF.1,
                    crate::pcie::VIRTIO_BLK_BDF.2,
                    0
                ),
                MmioOp::Read { size: 4 },
                &mut mem
            ),
            MmioOutcome::ReadValue(crate::pcie::NO_DEVICE)
        );
        program_virtio_blk_bar4(&mut p, &mut mem, pci_bar);
        assert_eq!(p.pcie_mmio_target(pci_bar), None);
        assert_eq!(
            p.on_mmio(legacy_slot.base, MmioOp::Read { size: 4 }, &mut mem),
            MmioOutcome::Unmapped
        );
        assert_eq!(p.virtio_iso_stats(), None);
        assert_eq!(p.pci_boot_media_stats(), None);
    }

    #[test]
    fn pcie_nvme_endpoint_routes_bar0_to_controller_registers() {
        let mut p = platform();
        let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0);

        assert_eq!(
            p.on_mmio(
                pcie_cfg_gpa(1, 0, crate::pcie::REG_VENDOR_DEVICE),
                MmioOp::Read { size: 4 },
                &mut mem
            ),
            MmioOutcome::ReadValue(0x0010_1b36)
        );
        assert!(matches!(
            p.on_mmio(
                pcie_cfg_gpa(1, 0, crate::pcie::REG_REVISION_CLASS),
                MmioOp::Read { size: 4 },
                &mut mem
            ),
            MmioOutcome::ReadValue(v) if v >> 8 == u64::from(crate::pcie::NVME_CLASS_CODE)
        ));

        p.on_mmio(
            pcie_cfg_gpa(1, 0, crate::pcie::REG_BAR0),
            MmioOp::Write {
                size: 4,
                value: 0xFFFF_FFFF,
            },
            &mut mem,
        );
        let MmioOutcome::ReadValue(mask) = p.on_mmio(
            pcie_cfg_gpa(1, 0, crate::pcie::REG_BAR0),
            MmioOp::Read { size: 4 },
            &mut mem,
        ) else {
            panic!("BAR0 sizing read did not return a value");
        };
        let size = (!((mask as u32) & !0xF)).wrapping_add(1);
        assert_eq!(size, crate::pcie::NVME_BAR0_SIZE);

        assert_eq!(
            p.on_mmio(
                machine::PCIE_MMIO_32.base + crate::nvme::REG_VS,
                MmioOp::Read { size: 4 },
                &mut mem
            ),
            MmioOutcome::KnownUnimplemented("pcie-mmio-32")
        );
        program_nvme_bar0(&mut p, &mut mem);
        assert_eq!(
            p.on_mmio(
                machine::PCIE_MMIO_32.base + crate::nvme::REG_VS,
                MmioOp::Read { size: 4 },
                &mut mem
            ),
            MmioOutcome::ReadValue(u64::from(crate::nvme::NVME_VERSION_1_4_0))
        );
        assert_eq!(
            p.on_mmio(
                machine::PCIE_MMIO_32.base + crate::nvme::REG_CC,
                MmioOp::Write { size: 4, value: 1 },
                &mut mem
            ),
            MmioOutcome::WriteAck
        );
        assert_eq!(
            p.on_mmio(
                machine::PCIE_MMIO_32.base + crate::nvme::REG_CSTS,
                MmioOp::Read { size: 4 },
                &mut mem
            ),
            MmioOutcome::ReadValue(1)
        );
    }

    #[test]
    fn pcie_nvme_liveness_separates_bar_command_mmio_cc_and_admin_doorbell() {
        const ASQ: u64 = machine::RAM_BASE + 0x1000;
        const ACQ: u64 = machine::RAM_BASE + 0x2000;
        const DATA: u64 = machine::RAM_BASE + 0x3000;

        let mut p = platform();
        let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0x8000);

        // Given: the NVMe endpoint is advertised but untouched.
        let initial = p.nvme_pcie_liveness();
        assert!(initial.nvme_advertised);
        assert!(!initial.nvme_ecam_touched);
        assert!(!initial.nvme_bar0_assigned);
        assert!(!initial.nvme_command_memory_enabled);
        assert!(!initial.nvme_command_bus_master_enabled);
        assert!(!initial.nvme_mmio_reached);
        assert!(!initial.nvme_cc_enabled);
        assert!(!initial.nvme_admin_doorbell_rung);

        // When: firmware assigns NVMe BAR0 but leaves command memory disabled.
        assert_eq!(
            p.on_mmio(
                pcie_cfg_gpa(1, 0, crate::pcie::REG_BAR0),
                MmioOp::Write {
                    size: 4,
                    value: machine::PCIE_MMIO_32.base,
                },
                &mut mem,
            ),
            MmioOutcome::WriteAck
        );

        // Then: BAR assignment is visible without claiming MMIO reachability.
        let bar_only = p.nvme_pcie_liveness();
        assert!(bar_only.nvme_ecam_touched);
        assert!(bar_only.nvme_bar0_assigned);
        assert!(!bar_only.nvme_command_memory_enabled);
        assert!(!bar_only.nvme_command_bus_master_enabled);
        assert!(!bar_only.nvme_mmio_reached);
        assert_eq!(p.pcie_mmio_target(machine::PCIE_MMIO_32.base), None);

        // When: only NVMe command memory is enabled.
        assert_eq!(
            p.on_mmio(
                pcie_cfg_gpa(1, 0, crate::pcie::REG_COMMAND_STATUS),
                MmioOp::Write {
                    size: 2,
                    value: u64::from(crate::pcie::CMD_MEMORY_SPACE),
                },
                &mut mem,
            ),
            MmioOutcome::WriteAck
        );

        // Then: MMIO decode is enabled while bus-master is still reported apart.
        let memory_only = p.nvme_pcie_liveness();
        assert!(memory_only.nvme_command_memory_enabled);
        assert!(!memory_only.nvme_command_bus_master_enabled);
        assert_eq!(
            p.pcie_mmio_target(machine::PCIE_MMIO_32.base),
            Some(PcieMmioTarget {
                bdf: NVME_BDF,
                bar_index: 0,
                offset: 0,
            })
        );
        assert_eq!(
            p.on_mmio(
                machine::PCIE_MMIO_32.base + crate::nvme::REG_VS,
                MmioOp::Read { size: 4 },
                &mut mem,
            ),
            MmioOutcome::ReadValue(u64::from(crate::nvme::NVME_VERSION_1_4_0))
        );
        let mmio_read = p.nvme_pcie_liveness();
        assert!(mmio_read.nvme_mmio_reached);
        assert!(!mmio_read.nvme_cc_enabled);
        assert!(!mmio_read.nvme_admin_doorbell_rung);

        // When: bus-master is added and the admin queue is used.
        assert_eq!(
            p.on_mmio(
                pcie_cfg_gpa(1, 0, crate::pcie::REG_COMMAND_STATUS),
                MmioOp::Write {
                    size: 2,
                    value: u64::from(crate::pcie::CMD_MEMORY_SPACE | crate::pcie::CMD_BUS_MASTER),
                },
                &mut mem,
            ),
            MmioOutcome::WriteAck
        );
        enable_nvme_controller(&mut p, &mut mem, ASQ, ACQ);
        let identify_controller = encode_nvme_sqe(0x06, 11, 0, DATA, 0x01, 0, 0);
        submit_admin_sqe(&mut p, &mut mem, ASQ, 0, &identify_controller);

        // Then: liveness distinguishes bus-master, CC enable, and doorbell.
        let live = p.nvme_pcie_liveness();
        assert!(live.nvme_command_bus_master_enabled);
        assert!(live.nvme_cc_enabled);
        assert!(live.nvme_admin_doorbell_rung);
    }

    #[test]
    fn pcie_nvme_bar0_doorbell_processes_admin_identify() {
        const ASQ: u64 = machine::RAM_BASE + 0x1000;
        const ACQ: u64 = machine::RAM_BASE + 0x2000;
        const DATA: u64 = machine::RAM_BASE + 0x3000;

        let mut p = platform();
        let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0x8000);
        program_nvme_bar0(&mut p, &mut mem);

        enable_nvme_controller(&mut p, &mut mem, ASQ, ACQ);
        p.nvme_completion_scratch.reserve(4);
        let completion_scratch_capacity = p.nvme_completion_scratch.capacity();
        let completion_scratch_ptr = p.nvme_completion_scratch.as_ptr();

        let identify_controller = encode_nvme_sqe(0x06, 7, 0, DATA, 0x01, 0, 0);
        submit_admin_sqe(&mut p, &mut mem, ASQ, 0, &identify_controller);
        assert!(p.nvme_completion_scratch.is_empty());
        assert_eq!(
            p.nvme_completion_scratch.capacity(),
            completion_scratch_capacity
        );
        assert_eq!(p.nvme_completion_scratch.as_ptr(), completion_scratch_ptr);

        let identify = mem.read_bytes(DATA, 4096).unwrap();
        assert_eq!(u16::from_le_bytes([identify[0], identify[1]]), 0x1b36);
        assert!(identify[24..64].starts_with(b"BridgeVM NVMe"));

        let completion = mem.read_bytes(ACQ, 16).unwrap();
        assert_eq!(u16::from_le_bytes([completion[12], completion[13]]), 7);
        let status = u16::from_le_bytes([completion[14], completion[15]]);
        assert_eq!(status & 0x1, 1, "phase tag must be set");
        assert_eq!(status >> 1, 0, "identify must complete successfully");
    }

    #[test]
    fn pcie_nvme_msix_table_completion_queues_a_message() {
        const ASQ: u64 = machine::RAM_BASE + 0x1000;
        const ACQ: u64 = machine::RAM_BASE + 0x2000;
        const DATA: u64 = machine::RAM_BASE + 0x3000;
        const MSI_ADDRESS: u64 = machine::GIC_ITS.base + 0x40;
        const MSI_DATA: u32 = 35;

        let mut p = platform();
        let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0x8000);
        program_nvme_bar0(&mut p, &mut mem);
        enable_nvme_msix_vector0(&mut p, &mut mem, MSI_ADDRESS, MSI_DATA);
        enable_nvme_controller(&mut p, &mut mem, ASQ, ACQ);

        let identify_controller = encode_nvme_sqe(0x06, 9, 0, DATA, 0x01, 0, 0);
        submit_admin_sqe(&mut p, &mut mem, ASQ, 0, &identify_controller);

        assert_eq!(
            p.take_pending_msix(),
            vec![crate::msix::MsixMessage {
                vector: 0,
                address: MSI_ADDRESS,
                data: MSI_DATA,
            }]
        );
    }

    #[test]
    fn virtio_iso_completion_queues_legacy_spi_level_changes() {
        const REG_GUEST_PAGE_SIZE: u64 = 0x28;
        const REG_QUEUE_NUM: u64 = 0x38;
        const REG_QUEUE_ALIGN: u64 = 0x3c;
        const REG_QUEUE_PFN: u64 = 0x40;
        const REG_QUEUE_NOTIFY: u64 = 0x50;
        const REG_INTERRUPT_ACK: u64 = 0x64;
        const DESC_F_NEXT: u16 = 1;
        const DESC_F_WRITE: u16 = 2;
        const VIRTIO_BLK_T_IN: u32 = 0;
        const VIRTIO_BLK_S_OK: u8 = 0;

        let path = temp_path("virtio-iso");
        let mut media = vec![0u8; 1024];
        media[512..520].copy_from_slice(b"WINSETUP");
        fs::write(&path, media).unwrap();

        let mut p = platform();
        p.attach_virtio_iso(&path).unwrap();
        let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0x10000);
        let slot_base = machine::virtio_mmio_slot(INSTALLER_ISO_SLOT).base;
        let desc = machine::RAM_BASE + 0x1000;
        let avail = desc + 8 * 16;
        let used = (avail + 4 + 8 * 2).div_ceil(4096) * 4096;
        let header = machine::RAM_BASE + 0x4000;
        let data = machine::RAM_BASE + 0x5000;
        let status = machine::RAM_BASE + 0x6000;

        assert!(mem.write_bytes(header, &VIRTIO_BLK_T_IN.to_le_bytes()));
        assert!(mem.write_bytes(header + 8, &1u64.to_le_bytes()));
        write_vring_desc(&mut mem, desc, 0, header, 16, DESC_F_NEXT, 1);
        write_vring_desc(&mut mem, desc, 1, data, 512, DESC_F_NEXT | DESC_F_WRITE, 2);
        write_vring_desc(&mut mem, desc, 2, status, 1, DESC_F_WRITE, 0);
        assert!(mem.write_bytes(avail + 2, &1u16.to_le_bytes()));
        assert!(mem.write_bytes(avail + 4, &0u16.to_le_bytes()));

        for (reg, value) in [
            (REG_QUEUE_NUM, 8),
            (REG_GUEST_PAGE_SIZE, 4096),
            (REG_QUEUE_ALIGN, 4096),
            (REG_QUEUE_PFN, desc >> 12),
        ] {
            assert_eq!(
                p.on_mmio(slot_base + reg, MmioOp::Write { size: 4, value }, &mut mem),
                MmioOutcome::WriteAck
            );
        }
        assert_eq!(
            p.on_mmio(
                slot_base + REG_QUEUE_NOTIFY,
                MmioOp::Write { size: 4, value: 0 },
                &mut mem
            ),
            MmioOutcome::WriteAck
        );

        assert_eq!(mem.read_bytes(data, 8).unwrap(), b"WINSETUP");
        assert_eq!(mem.read_bytes(status, 1).unwrap(), [VIRTIO_BLK_S_OK]);
        assert_eq!(
            u16::from_le_bytes(mem.read_bytes(used + 2, 2).unwrap().try_into().unwrap()),
            1
        );
        assert_eq!(
            p.take_pending_spi_levels(),
            vec![(
                machine::spi_to_intid(machine::virtio_mmio_spi(INSTALLER_ISO_SLOT as u32)),
                true
            )]
        );

        assert_eq!(
            p.on_mmio(
                slot_base + REG_INTERRUPT_ACK,
                MmioOp::Write { size: 4, value: 1 },
                &mut mem
            ),
            MmioOutcome::WriteAck
        );
        assert_eq!(
            p.take_pending_spi_levels(),
            vec![(
                machine::spi_to_intid(machine::virtio_mmio_spi(INSTALLER_ISO_SLOT as u32)),
                false
            )]
        );

        fs::remove_file(path).ok();
    }

    #[test]
    fn pcie_boot_media_reads_from_attached_iso_and_posts_interrupt() {
        const PCI_ISR_CFG_OFFSET: u64 = 0x1000;
        const PCI_NOTIFY_CFG_OFFSET: u64 = 0x3000;
        const REG_QUEUE_NUM: u64 = 0x038;
        const REG_QUEUE_READY: u64 = 0x044;
        const REG_QUEUE_NOTIFY: u64 = 0x050;
        const REG_QUEUE_DESC_LOW: u64 = 0x080;
        const REG_QUEUE_DRIVER_LOW: u64 = 0x090;
        const REG_QUEUE_DEVICE_LOW: u64 = 0x0a0;
        const DESC_F_NEXT: u16 = 1;
        const DESC_F_WRITE: u16 = 2;
        const VIRTIO_BLK_T_IN: u32 = 0;
        const VIRTIO_BLK_S_OK: u8 = 0;

        let path = temp_path("pci-boot-media");
        let mut media = vec![0u8; 1024];
        media[512..520].copy_from_slice(b"WINSETUP");
        fs::write(&path, media).unwrap();

        let mut p = platform();
        p.attach_pci_boot_media(&path).unwrap();
        let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0x10000);
        let bar = machine::PCIE_MMIO_32.base + 0x8000;
        program_virtio_blk_bar4(&mut p, &mut mem, bar);

        let desc = machine::RAM_BASE + 0x1000;
        let avail = machine::RAM_BASE + 0x2000;
        let used = machine::RAM_BASE + 0x3000;
        let header = machine::RAM_BASE + 0x4000;
        let data = machine::RAM_BASE + 0x5000;
        let status = machine::RAM_BASE + 0x6000;

        assert!(mem.write_bytes(header, &VIRTIO_BLK_T_IN.to_le_bytes()));
        assert!(mem.write_bytes(header + 8, &1u64.to_le_bytes()));
        write_vring_desc(&mut mem, desc, 0, header, 16, DESC_F_NEXT, 1);
        write_vring_desc(&mut mem, desc, 1, data, 512, DESC_F_NEXT | DESC_F_WRITE, 2);
        write_vring_desc(&mut mem, desc, 2, status, 1, DESC_F_WRITE, 0);
        assert!(mem.write_bytes(avail + 2, &1u16.to_le_bytes()));
        assert!(mem.write_bytes(avail + 4, &0u16.to_le_bytes()));

        for (reg, value) in [
            (REG_QUEUE_NUM, 8),
            (REG_QUEUE_DESC_LOW, desc),
            (REG_QUEUE_DRIVER_LOW, avail),
            (REG_QUEUE_DEVICE_LOW, used),
            (REG_QUEUE_READY, 1),
        ] {
            assert_eq!(
                p.on_mmio(bar + reg, MmioOp::Write { size: 4, value }, &mut mem),
                MmioOutcome::WriteAck
            );
        }
        assert_eq!(
            p.on_mmio(
                bar + PCI_NOTIFY_CFG_OFFSET + REG_QUEUE_NOTIFY,
                MmioOp::Write { size: 4, value: 0 },
                &mut mem
            ),
            MmioOutcome::WriteAck
        );

        assert_eq!(mem.read_bytes(data, 8).unwrap(), b"WINSETUP");
        assert_eq!(mem.read_bytes(status, 1).unwrap(), [VIRTIO_BLK_S_OK]);
        assert_eq!(
            p.take_pending_spi_levels(),
            vec![(machine::spi_to_intid(machine::SPI_PCIE_INTA), true)]
        );

        assert_eq!(
            p.on_mmio(
                bar + PCI_ISR_CFG_OFFSET,
                MmioOp::Write { size: 4, value: 1 },
                &mut mem
            ),
            MmioOutcome::WriteAck
        );
        assert_eq!(
            p.take_pending_spi_levels(),
            vec![(machine::spi_to_intid(machine::SPI_PCIE_INTA), false)]
        );

        fs::remove_file(path).ok();
    }

    #[test]
    fn pcie_boot_media_msix_bar_decodes_table_and_pba() {
        let path = temp_path("pci-boot-media-msix");
        fs::write(&path, [0u8; 512]).unwrap();

        let mut p = platform();
        p.attach_pci_boot_media(&path).unwrap();
        let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0x1000);
        let bar = machine::PCIE_MMIO_32.base + 0x1_8000;
        program_virtio_blk_bar1(&mut p, &mut mem, bar);

        assert_eq!(
            p.pcie_mmio_target(bar),
            Some(PcieMmioTarget {
                bdf: VIRTIO_BLK_BDF,
                bar_index: 1,
                offset: 0,
            })
        );
        assert_eq!(
            p.on_mmio(bar + 12, MmioOp::Read { size: 4 }, &mut mem),
            MmioOutcome::ReadValue(1)
        );
        assert_eq!(
            p.on_mmio(
                bar,
                MmioOp::Write {
                    size: 4,
                    value: 0xfee0_0000,
                },
                &mut mem
            ),
            MmioOutcome::WriteAck
        );
        assert_eq!(
            p.on_mmio(bar, MmioOp::Read { size: 4 }, &mut mem),
            MmioOutcome::ReadValue(0xfee0_0000)
        );
        assert_eq!(
            p.on_mmio(
                bar + u64::from(crate::pcie::VIRTIO_BLK_MSIX_PBA_OFFSET),
                MmioOp::Read { size: 8 },
                &mut mem
            ),
            MmioOutcome::ReadValue(0)
        );
        assert_eq!(
            p.on_mmio(
                bar + u64::from(crate::pcie::VIRTIO_BLK_MSIX_PBA_OFFSET),
                MmioOp::Write {
                    size: 8,
                    value: u64::MAX,
                },
                &mut mem
            ),
            MmioOutcome::WriteAck
        );
        assert_eq!(
            p.on_mmio(
                bar + u64::from(crate::pcie::VIRTIO_BLK_MSIX_PBA_OFFSET),
                MmioOp::Read { size: 8 },
                &mut mem
            ),
            MmioOutcome::ReadValue(0)
        );

        fs::remove_file(path).ok();
    }

    #[test]
    fn pcie_boot_media_modern_bar_live_offsets_stay_modelled() {
        let path = temp_path("pci-boot-media-modern-offsets");
        fs::write(&path, [0u8; 512]).unwrap();

        let mut p = platform();
        p.attach_pci_boot_media(&path).unwrap();
        let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0x1000);
        let bar = machine::PCIE_MMIO_32.base + 0x1_c000;
        program_virtio_blk_bar4(&mut p, &mut mem, bar);

        for offset in [0x20, 0x28, 0x30, 0x38, 0x40, 0x48, 0x50, 0x58] {
            assert!(matches!(
                p.on_mmio(bar + offset, MmioOp::Read { size: 4 }, &mut mem),
                MmioOutcome::ReadValue(_)
            ));
            assert_eq!(
                p.on_mmio(bar + offset, MmioOp::Write { size: 4, value: 0 }, &mut mem),
                MmioOutcome::WriteAck
            );
        }

        fs::remove_file(path).ok();
    }

    #[test]
    fn pcie_boot_media_legacy_pio_bar_decodes_without_unimplemented() {
        let path = temp_path("pci-boot-media-pio");
        fs::write(&path, [0u8; 512]).unwrap();

        let mut p = platform();
        p.attach_pci_boot_media(&path).unwrap();
        let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0x1000);
        program_virtio_blk_bar0_pio(&mut p, &mut mem, 0);

        assert_eq!(
            p.pcie_pio_target(machine::PCIE_PIO.base),
            Some(PciePioTarget {
                bdf: VIRTIO_BLK_BDF,
                bar_index: 0,
                offset: 0,
            })
        );
        assert_eq!(
            p.on_mmio(machine::PCIE_PIO.base, MmioOp::Read { size: 4 }, &mut mem),
            MmioOutcome::ReadValue(0x60)
        );
        assert_eq!(
            p.on_mmio(
                machine::PCIE_PIO.base + 0x12,
                MmioOp::Write { size: 1, value: 4 },
                &mut mem
            ),
            MmioOutcome::WriteAck
        );
        assert_eq!(p.pci_boot_media_stats().unwrap().status, 4);

        fs::remove_file(path).ok();
    }

    #[test]
    fn pcie_virtio_net_opt_in_routes_tx_msix_and_reset_clears_runtime_state() {
        const MSI_ADDRESS: u64 = machine::GIC_ITS.base + 0x80;
        const MSI_DATA: u32 = 0x61;

        let mut p = platform_with_devices(VirtPlatformDeviceConfig {
            virtio_net_present: true,
            virtio_net_backend: VirtioNetBackendKind::Loopback,
            ..VirtPlatformDeviceConfig::default()
        });
        let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0x20000);
        let bar4 = machine::PCIE_MMIO_32.base + 0x4_0000;
        let bar1 = machine::PCIE_MMIO_32.base + 0x5_0000;

        assert_eq!(
            p.on_mmio(
                pcie_cfg_gpa(
                    crate::pcie::VIRTIO_NET_BDF.1,
                    crate::pcie::VIRTIO_NET_BDF.2,
                    crate::pcie::REG_VENDOR_DEVICE,
                ),
                MmioOp::Read { size: 4 },
                &mut mem,
            ),
            MmioOutcome::ReadValue(0x1041_1af4)
        );
        program_virtio_net_bar4(&mut p, &mut mem, bar4);
        program_virtio_net_bar1(&mut p, &mut mem, bar1);
        assert_eq!(
            p.pcie_mmio_target(bar4),
            Some(PcieMmioTarget {
                bdf: crate::pcie::VIRTIO_NET_BDF,
                bar_index: 4,
                offset: 0,
            })
        );
        assert_eq!(
            p.pcie_mmio_target(bar1),
            Some(PcieMmioTarget {
                bdf: crate::pcie::VIRTIO_NET_BDF,
                bar_index: 1,
                offset: 0,
            })
        );

        let desc = machine::RAM_BASE + 0x10000;
        let avail = machine::RAM_BASE + 0x11000;
        let used = machine::RAM_BASE + 0x12000;
        let hdr = machine::RAM_BASE + 0x13000;
        let payload = machine::RAM_BASE + 0x14000;
        let frame = b"\x02\x00\x00\x00\x00\x01\x52\x54\x00\x42\x56\x01\x08\x00platform";

        setup_virtio_net_queue(
            &mut p,
            &mut mem,
            bar4,
            NET_TX_QUEUE,
            TestVirtQueue { desc, avail, used },
            NET_TX_QUEUE,
        );
        enable_virtio_net_msix_vector(&mut p, &mut mem, bar1, NET_TX_QUEUE, MSI_ADDRESS, MSI_DATA);
        assert!(mem.write_bytes(hdr, &[0; NET_VIRTIO_HDR_LEN]));
        assert!(mem.write_bytes(payload, frame));
        write_vring_desc(
            &mut mem,
            desc,
            0,
            hdr,
            NET_VIRTIO_HDR_LEN as u32,
            NET_DESC_F_NEXT,
            1,
        );
        write_vring_desc(&mut mem, desc, 1, payload, frame.len() as u32, 0, 0);
        assert!(mem.write_bytes(avail + 2, &1u16.to_le_bytes()));
        assert!(mem.write_bytes(avail + 4, &0u16.to_le_bytes()));

        assert_eq!(
            p.on_mmio(
                bar4 + NET_NOTIFY_CFG_OFFSET + u64::from(NET_TX_QUEUE) * 4,
                MmioOp::Write { size: 4, value: 0 },
                &mut mem,
            ),
            MmioOutcome::WriteAck
        );

        let net = p.virtio_net.as_ref().expect("virtio-net device present");
        assert_eq!(
            net.backend().test_transmitted_frames(),
            Some(&[frame.to_vec()][..])
        );
        assert_eq!(
            p.pending_msix,
            vec![crate::msix::MsixMessage {
                vector: NET_TX_QUEUE,
                address: MSI_ADDRESS,
                data: MSI_DATA,
            }]
        );
        let stats = p.virtio_net_stats().unwrap();
        assert_eq!(stats.tx_count, 1);
        assert_eq!(stats.queues[usize::from(NET_TX_QUEUE)].last_avail_idx, 1);

        p.reset();

        assert_eq!(
            p.take_pending_msix(),
            Vec::<crate::msix::MsixMessage>::new()
        );
        assert_eq!(p.pcie_mmio_target(bar4), None);
        let stats = p.virtio_net_stats().unwrap();
        assert_eq!(stats.tx_count, 0);
        assert_eq!(stats.notify_count, 0);
        assert!(!stats.queues[usize::from(NET_TX_QUEUE)].ready);
        assert_eq!(stats.status, 0);

        assert_eq!(
            p.on_mmio(
                pcie_cfg_gpa(
                    crate::pcie::VIRTIO_NET_BDF.1,
                    crate::pcie::VIRTIO_NET_BDF.2,
                    crate::pcie::REG_VENDOR_DEVICE,
                ),
                MmioOp::Read { size: 4 },
                &mut mem,
            ),
            MmioOutcome::ReadValue(0x1041_1af4)
        );
        program_virtio_net_bar1(&mut p, &mut mem, bar1);
        assert_eq!(
            p.on_mmio(bar1 + 12, MmioOp::Read { size: 4 }, &mut mem),
            MmioOutcome::ReadValue(1)
        );
    }

    #[test]
    fn unknown_pcie_bar_still_reports_known_unimplemented() {
        let mut p = platform();
        let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0);

        assert_eq!(
            p.on_mmio(
                machine::PCIE_MMIO_32.base + 0x1_0000,
                MmioOp::Read { size: 4 },
                &mut mem
            ),
            MmioOutcome::KnownUnimplemented("pcie-mmio-32")
        );
    }

    #[test]
    fn xhci_bar_and_command_do_not_enable_nvme_liveness_or_decode() {
        let mut p = platform();
        let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0);
        let xhci_base = machine::PCIE_MMIO_32.base + 0x2_0000;

        // Given: only xHCI BAR0 and command bits are programmed.
        for (reg, value) in [
            (crate::pcie::REG_BAR0, xhci_base),
            (crate::pcie::REG_BAR0 + 4, 0),
            (
                crate::pcie::REG_COMMAND_STATUS,
                u64::from(crate::pcie::CMD_MEMORY_SPACE | crate::pcie::CMD_BUS_MASTER),
            ),
        ] {
            assert_eq!(
                p.on_mmio(
                    pcie_cfg_gpa(crate::pcie::XHCI_BDF.1, crate::pcie::XHCI_BDF.2, reg),
                    MmioOp::Write {
                        size: if reg == crate::pcie::REG_COMMAND_STATUS {
                            2
                        } else {
                            4
                        },
                        value,
                    },
                    &mut mem,
                ),
                MmioOutcome::WriteAck
            );
        }

        // Then: xHCI decode exists, but NVMe liveness and NVMe BAR decode do not.
        let live = p.nvme_pcie_liveness();
        assert!(live.nvme_advertised);
        assert!(!live.nvme_ecam_touched);
        assert!(!live.nvme_bar0_assigned);
        assert!(!live.nvme_command_memory_enabled);
        assert!(!live.nvme_command_bus_master_enabled);
        assert_eq!(p.pcie_mmio_target(machine::PCIE_MMIO_32.base), None);
        assert_eq!(
            p.pcie_mmio_target(xhci_base),
            Some(PcieMmioTarget {
                bdf: XHCI_BDF,
                bar_index: 0,
                offset: 0,
            })
        );
    }

    #[test]
    fn xhci_bar_reports_qemu_capability_registers() {
        let mut p = platform();
        let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0);
        let base = machine::PCIE_MMIO_32.base + 0x2_0000;

        assert_eq!(
            p.on_mmio(
                pcie_cfg_gpa(
                    crate::pcie::XHCI_BDF.1,
                    crate::pcie::XHCI_BDF.2,
                    crate::pcie::REG_BAR0
                ),
                MmioOp::Write {
                    size: 4,
                    value: base,
                },
                &mut mem
            ),
            MmioOutcome::WriteAck
        );
        assert_eq!(
            p.on_mmio(
                pcie_cfg_gpa(
                    crate::pcie::XHCI_BDF.1,
                    crate::pcie::XHCI_BDF.2,
                    crate::pcie::REG_BAR0 + 4
                ),
                MmioOp::Write { size: 4, value: 0 },
                &mut mem
            ),
            MmioOutcome::WriteAck
        );
        assert_eq!(
            p.on_mmio(
                pcie_cfg_gpa(
                    crate::pcie::XHCI_BDF.1,
                    crate::pcie::XHCI_BDF.2,
                    crate::pcie::REG_COMMAND_STATUS
                ),
                MmioOp::Write {
                    size: 2,
                    value: u64::from(crate::pcie::CMD_MEMORY_SPACE | crate::pcie::CMD_BUS_MASTER),
                },
                &mut mem
            ),
            MmioOutcome::WriteAck
        );
        assert_eq!(
            p.on_mmio(base, MmioOp::Read { size: 1 }, &mut mem),
            MmioOutcome::ReadValue(0x40)
        );
        assert_eq!(
            p.on_mmio(base, MmioOp::Read { size: 4 }, &mut mem),
            MmioOutcome::ReadValue(0x0100_0040)
        );
        assert_eq!(
            p.on_mmio(base + 0x04, MmioOp::Read { size: 4 }, &mut mem),
            MmioOutcome::ReadValue(0x0800_1040)
        );
        assert_eq!(
            p.on_mmio(base + 0x08, MmioOp::Read { size: 4 }, &mut mem),
            MmioOutcome::ReadValue(0x0000_000f)
        );
        assert_eq!(
            p.on_mmio(base + 0x10, MmioOp::Read { size: 4 }, &mut mem),
            MmioOutcome::ReadValue(0x0008_7001)
        );
        assert_eq!(
            p.on_mmio(base + 0x14, MmioOp::Read { size: 4 }, &mut mem),
            MmioOutcome::ReadValue(0x0000_2000)
        );
        assert_eq!(
            p.on_mmio(base + 0x18, MmioOp::Read { size: 4 }, &mut mem),
            MmioOutcome::ReadValue(0x0000_1000)
        );
    }

    #[test]
    fn xhci_bar_routes_from_64bit_mmio_window() {
        let mut p = platform();
        let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0);
        let base = machine::PCIE_MMIO_64.base + 0x2_0000;

        assert_eq!(
            p.on_mmio(
                pcie_cfg_gpa(
                    crate::pcie::XHCI_BDF.1,
                    crate::pcie::XHCI_BDF.2,
                    crate::pcie::REG_BAR0
                ),
                MmioOp::Write {
                    size: 4,
                    value: base & 0xffff_ffff,
                },
                &mut mem
            ),
            MmioOutcome::WriteAck
        );
        assert_eq!(
            p.on_mmio(
                pcie_cfg_gpa(
                    crate::pcie::XHCI_BDF.1,
                    crate::pcie::XHCI_BDF.2,
                    crate::pcie::REG_BAR0 + 4
                ),
                MmioOp::Write {
                    size: 4,
                    value: base >> 32,
                },
                &mut mem
            ),
            MmioOutcome::WriteAck
        );
        assert_eq!(
            p.on_mmio(
                pcie_cfg_gpa(
                    crate::pcie::XHCI_BDF.1,
                    crate::pcie::XHCI_BDF.2,
                    crate::pcie::REG_COMMAND_STATUS
                ),
                MmioOp::Write {
                    size: 2,
                    value: u64::from(crate::pcie::CMD_MEMORY_SPACE | crate::pcie::CMD_BUS_MASTER),
                },
                &mut mem
            ),
            MmioOutcome::WriteAck
        );

        assert_eq!(
            p.on_mmio(base, MmioOp::Read { size: 4 }, &mut mem),
            MmioOutcome::ReadValue(0x0100_0040)
        );
        assert_eq!(
            p.on_mmio(base + 0x04, MmioOp::Read { size: 4 }, &mut mem),
            MmioOutcome::ReadValue(0x0800_1040)
        );
    }

    #[test]
    fn pcie_nvme_reads_and_writes_preloaded_disk_media() {
        const ASQ: u64 = machine::RAM_BASE + 0x1000;
        const ACQ: u64 = machine::RAM_BASE + 0x2000;
        const IO_SQ: u64 = machine::RAM_BASE + 0x3000;
        const IO_CQ: u64 = machine::RAM_BASE + 0x4000;
        const DATA: u64 = machine::RAM_BASE + 0x5000;
        const SLBA: u64 = 7;

        let mut p = platform();
        let mut disk = vec![0u8; crate::nvme::LBA_SIZE * 16];
        let pattern: Vec<u8> = (0..crate::nvme::LBA_SIZE)
            .map(|i| 0x80 | ((i % 0x40) as u8))
            .collect();
        let start = SLBA as usize * crate::nvme::LBA_SIZE;
        disk[start..start + crate::nvme::LBA_SIZE].copy_from_slice(&pattern);
        p.load_nvme_disk(disk);

        let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0x9000);
        program_nvme_bar0(&mut p, &mut mem);
        enable_nvme_controller(&mut p, &mut mem, ASQ, ACQ);

        let cdw10 = (3u32 << 16) | 1;
        let create_cq = encode_nvme_sqe(0x05, 1, 0, IO_CQ, cdw10, 1, 0);
        submit_admin_sqe(&mut p, &mut mem, ASQ, 0, &create_cq);
        let create_sq = encode_nvme_sqe(0x01, 2, 0, IO_SQ, cdw10, 1u32 << 16, 0);
        submit_admin_sqe(&mut p, &mut mem, ASQ, 1, &create_sq);

        let read = encode_nvme_sqe(
            0x02,
            0x10,
            crate::nvme::NSID,
            DATA,
            SLBA as u32,
            (SLBA >> 32) as u32,
            0,
        );
        assert!(mem.write_bytes(IO_SQ, &read));
        assert_eq!(
            p.on_mmio(
                machine::PCIE_MMIO_32.base + crate::nvme::REG_DOORBELL_BASE + 2 * 4,
                MmioOp::Write { size: 4, value: 1 },
                &mut mem,
            ),
            MmioOutcome::WriteAck
        );
        assert_eq!(
            mem.read_bytes(DATA, crate::nvme::LBA_SIZE).unwrap(),
            pattern
        );

        let replacement: Vec<u8> = (0..crate::nvme::LBA_SIZE)
            .map(|i| 0x40 | ((i % 0x20) as u8))
            .collect();
        assert!(mem.write_bytes(DATA, &replacement));
        let write = encode_nvme_sqe(
            0x01,
            0x11,
            crate::nvme::NSID,
            DATA,
            SLBA as u32,
            (SLBA >> 32) as u32,
            0,
        );
        assert!(mem.write_bytes(IO_SQ + crate::nvme::SQ_ENTRY_SIZE, &write));
        assert_eq!(
            p.on_mmio(
                machine::PCIE_MMIO_32.base + crate::nvme::REG_DOORBELL_BASE + 2 * 4,
                MmioOp::Write { size: 4, value: 2 },
                &mut mem,
            ),
            MmioOutcome::WriteAck
        );
        assert_eq!(
            &p.nvme_disk()[start..start + crate::nvme::LBA_SIZE],
            replacement.as_slice()
        );
    }

    #[test]
    fn platform_reset_preserving_media_and_vars_clears_runtime_state() {
        const ASQ: u64 = machine::RAM_BASE + 0x1000;
        const ACQ: u64 = machine::RAM_BASE + 0x2000;
        const IO_SQ: u64 = machine::RAM_BASE + 0x3000;
        const IO_CQ: u64 = machine::RAM_BASE + 0x4000;
        const DATA: u64 = machine::RAM_BASE + 0x5000;
        const MSI_ADDRESS: u64 = machine::GIC_ITS.base + 0x40;
        const MSI_DATA: u32 = 35;

        // Given: persistent media, virtio installer media, RAMFB fw_cfg bytes,
        // and UEFI vars have guest-visible writes, while device runtime state
        // and pending interrupts are dirty.
        let virtio_iso_path = temp_path("reset-virtio-iso");
        let pci_boot_media_path = temp_path("reset-pci-boot-media");
        let mut installer_media = vec![0u8; 1024];
        installer_media[512..520].copy_from_slice(b"WINSETUP");
        fs::write(&virtio_iso_path, &installer_media).unwrap();
        fs::write(&pci_boot_media_path, &installer_media).unwrap();

        let mut p = platform_with_ramfb();
        let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0x18000);
        p.attach_virtio_iso(&virtio_iso_path).unwrap();
        p.attach_pci_boot_media(&pci_boot_media_path).unwrap();
        write_valid_ramfb_config(&mut p, &mut mem);
        assert!(p.ramfb_config().is_some());
        p.load_nvme_disk(vec![0u8; crate::nvme::LBA_SIZE * 16]);
        p.attach_nvme_second_namespace(crate::nvme::LBA_SIZE * 8);
        program_nvme_bar0(&mut p, &mut mem);
        enable_nvme_msix_vector0(&mut p, &mut mem, MSI_ADDRESS, MSI_DATA);
        enable_nvme_controller(&mut p, &mut mem, ASQ, ACQ);

        let cdw10 = (3u32 << 16) | 1;
        let create_cq = encode_nvme_sqe(0x05, 1, 0, IO_CQ, cdw10, 1, 0);
        submit_admin_sqe(&mut p, &mut mem, ASQ, 0, &create_cq);
        let create_sq = encode_nvme_sqe(0x01, 2, 0, IO_SQ, cdw10, 1u32 << 16, 0);
        submit_admin_sqe(&mut p, &mut mem, ASQ, 1, &create_sq);

        let ns1_pattern: Vec<u8> = (0..crate::nvme::LBA_SIZE)
            .map(|i| 0x20 | ((i % 0x20) as u8))
            .collect();
        assert!(mem.write_bytes(DATA, &ns1_pattern));
        let ns1_write = encode_nvme_sqe(0x01, 0x31, crate::nvme::NSID, DATA, 2, 0, 0);
        assert!(mem.write_bytes(IO_SQ, &ns1_write));
        assert_eq!(
            p.on_mmio(
                machine::PCIE_MMIO_32.base + crate::nvme::REG_DOORBELL_BASE + 2 * 4,
                MmioOp::Write { size: 4, value: 1 },
                &mut mem,
            ),
            MmioOutcome::WriteAck
        );

        let ns2_pattern: Vec<u8> = (0..crate::nvme::LBA_SIZE)
            .map(|i| 0x80 | ((i % 0x40) as u8))
            .collect();
        assert!(mem.write_bytes(DATA, &ns2_pattern));
        let ns2_write = encode_nvme_sqe(0x01, 0x32, crate::nvme::NSID2, DATA, 0, 0, 0);
        assert!(mem.write_bytes(IO_SQ + crate::nvme::SQ_ENTRY_SIZE, &ns2_write));
        assert_eq!(
            p.on_mmio(
                machine::PCIE_MMIO_32.base + crate::nvme::REG_DOORBELL_BASE + 2 * 4,
                MmioOp::Write { size: 4, value: 2 },
                &mut mem,
            ),
            MmioOutcome::WriteAck
        );

        let identify_controller = encode_nvme_sqe(0x06, 0x33, 0, DATA, 0x01, 0, 0);
        submit_admin_sqe(&mut p, &mut mem, ASQ, 2, &identify_controller);
        assert!(!p.pending_msix.is_empty());
        assert!(p.nvme_pcie_liveness().nvme_admin_doorbell_rung);
        assert!(!p.nvme_command_trace().is_empty());

        assert_eq!(read_virtio_iso_sector(&mut p, &mut mem, 1, 8), b"WINSETUP");
        let pci_boot_media_bar = machine::PCIE_MMIO_32.base + 0x8000;
        let pci_boot_media_msix_bar = machine::PCIE_MMIO_32.base + 0x1_8000;
        program_virtio_blk_bar4(&mut p, &mut mem, pci_boot_media_bar);
        assert_eq!(
            read_pci_boot_media_sector(&mut p, &mut mem, pci_boot_media_bar, 1, 8),
            b"WINSETUP"
        );
        program_virtio_blk_bar1(&mut p, &mut mem, pci_boot_media_msix_bar);
        assert_eq!(
            p.on_mmio(
                pci_boot_media_msix_bar,
                MmioOp::Write {
                    size: 4,
                    value: 0xfee0_0000,
                },
                &mut mem,
            ),
            MmioOutcome::WriteAck
        );
        assert_eq!(
            p.on_mmio(
                pci_boot_media_msix_bar + 8,
                MmioOp::Write {
                    size: 4,
                    value: 0x45,
                },
                &mut mem,
            ),
            MmioOutcome::WriteAck
        );
        assert_eq!(
            p.on_mmio(pci_boot_media_msix_bar, MmioOp::Read { size: 4 }, &mut mem,),
            MmioOutcome::ReadValue(0xfee0_0000)
        );
        assert_eq!(
            p.on_mmio(
                pci_boot_media_msix_bar + 8,
                MmioOp::Read { size: 4 },
                &mut mem,
            ),
            MmioOutcome::ReadValue(0x45)
        );
        assert!(!p.virtio_iso_request_trace().unwrap().is_empty());
        assert!(!p.pci_boot_media_request_trace().unwrap().is_empty());

        p.load_flash_vars(&[0xff; 8]);
        assert_eq!(
            p.on_mmio(
                machine::FLASH_VARS.base,
                MmioOp::Write {
                    size: 4,
                    value: 0x0040_0040,
                },
                &mut mem,
            ),
            MmioOutcome::WriteAck
        );
        assert_eq!(
            p.on_mmio(
                machine::FLASH_VARS.base,
                MmioOp::Write {
                    size: 4,
                    value: 0x1234_5678,
                },
                &mut mem,
            ),
            MmioOutcome::WriteAck
        );
        let flash_after_program = p.flash_vars_image()[0..8].to_vec();

        // When: the probe reboot loop asks the platform to reset runtime state.
        p.reset();

        // Then: persistent media and vars survive, while PCIe/NVMe runtime state
        // no longer carries over to the next boot.
        let ns1_start = 2 * crate::nvme::LBA_SIZE;
        assert_eq!(
            &p.nvme_disk()[ns1_start..ns1_start + crate::nvme::LBA_SIZE],
            ns1_pattern.as_slice()
        );
        assert_eq!(&p.flash_vars_image()[0..8], flash_after_program.as_slice());
        assert_eq!(
            p.take_pending_msix(),
            Vec::<crate::msix::MsixMessage>::new()
        );
        assert_eq!(
            p.take_pending_spi_levels(),
            vec![
                (
                    machine::spi_to_intid(machine::virtio_mmio_spi(INSTALLER_ISO_SLOT as u32)),
                    false,
                ),
                (machine::spi_to_intid(machine::SPI_PCIE_INTA), false),
            ]
        );
        assert_eq!(p.take_pending_spi_levels(), Vec::<(u32, bool)>::new());
        assert_eq!(p.fw_cfg.read_data(4), b"QEMU");
        let (_, ramfb_size_after_reset) = fw_cfg_file_entry(&mut p, b"etc/ramfb");
        assert_eq!(ramfb_size_after_reset, RAMFB_CONFIG_SIZE);
        p.refresh_ramfb();
        assert_eq!(p.ramfb_config(), None);
        let virtio_iso_stats = p.virtio_iso_stats().unwrap();
        assert_eq!(virtio_iso_stats.request_count, 0);
        assert_eq!(virtio_iso_stats.read_count, 0);
        assert_eq!(virtio_iso_stats.notify_count, 0);
        assert!(!virtio_iso_stats.queue_ready);
        assert_eq!(virtio_iso_stats.status, 0);
        assert!(p.virtio_iso_request_trace().unwrap().is_empty());
        let pci_boot_media_stats = p.pci_boot_media_stats().unwrap();
        assert_eq!(pci_boot_media_stats.request_count, 0);
        assert_eq!(pci_boot_media_stats.read_count, 0);
        assert_eq!(pci_boot_media_stats.notify_count, 0);
        assert!(!pci_boot_media_stats.queue_ready);
        assert_eq!(pci_boot_media_stats.status, 0);
        assert!(p.pci_boot_media_request_trace().unwrap().is_empty());
        program_virtio_blk_bar1(&mut p, &mut mem, pci_boot_media_msix_bar);
        assert_eq!(
            p.on_mmio(pci_boot_media_msix_bar, MmioOp::Read { size: 4 }, &mut mem,),
            MmioOutcome::ReadValue(0)
        );
        assert_eq!(
            p.on_mmio(
                pci_boot_media_msix_bar + 8,
                MmioOp::Read { size: 4 },
                &mut mem,
            ),
            MmioOutcome::ReadValue(0)
        );
        assert_eq!(
            p.on_mmio(
                pci_boot_media_msix_bar + 12,
                MmioOp::Read { size: 4 },
                &mut mem,
            ),
            MmioOutcome::ReadValue(1)
        );
        let reset_liveness = p.nvme_pcie_liveness();
        assert!(reset_liveness.nvme_advertised);
        assert!(!reset_liveness.nvme_ecam_touched);
        assert!(!reset_liveness.nvme_command_memory_enabled);
        assert!(!reset_liveness.nvme_command_bus_master_enabled);
        assert!(!reset_liveness.nvme_bar0_assigned);
        assert!(!reset_liveness.nvme_mmio_reached);
        assert!(!reset_liveness.nvme_cc_enabled);
        assert!(!reset_liveness.nvme_admin_doorbell_rung);
        assert_eq!(p.pcie_mmio_target(machine::PCIE_MMIO_32.base), None);
        assert!(p.nvme_command_trace().is_empty());

        program_nvme_bar0(&mut p, &mut mem);
        enable_nvme_controller(&mut p, &mut mem, ASQ, ACQ);
        let cdw10 = (3u32 << 16) | 1;
        let create_cq = encode_nvme_sqe(0x05, 3, 0, IO_CQ, cdw10, 1, 0);
        submit_admin_sqe(&mut p, &mut mem, ASQ, 0, &create_cq);
        let create_sq = encode_nvme_sqe(0x01, 4, 0, IO_SQ, cdw10, 1u32 << 16, 0);
        submit_admin_sqe(&mut p, &mut mem, ASQ, 1, &create_sq);
        assert!(mem.write_bytes(DATA, &[0u8; crate::nvme::LBA_SIZE]));
        let ns2_read = encode_nvme_sqe(0x02, 0x34, crate::nvme::NSID2, DATA, 0, 0, 0);
        assert!(mem.write_bytes(IO_SQ, &ns2_read));
        assert_eq!(
            p.on_mmio(
                machine::PCIE_MMIO_32.base + crate::nvme::REG_DOORBELL_BASE + 2 * 4,
                MmioOp::Write { size: 4, value: 1 },
                &mut mem,
            ),
            MmioOutcome::WriteAck
        );
        assert_eq!(
            mem.read_bytes(DATA, crate::nvme::LBA_SIZE).unwrap(),
            ns2_pattern
        );
        assert_eq!(read_virtio_iso_sector(&mut p, &mut mem, 1, 8), b"WINSETUP");
        let pci_boot_media_bar = machine::PCIE_MMIO_32.base + 0x8000;
        program_virtio_blk_bar4(&mut p, &mut mem, pci_boot_media_bar);
        assert_eq!(
            read_pci_boot_media_sector(&mut p, &mut mem, pci_boot_media_bar, 1, 8),
            b"WINSETUP"
        );

        fs::remove_file(virtio_iso_path).ok();
        fs::remove_file(pci_boot_media_path).ok();
    }

    #[test]
    fn uart_writes_are_captured_via_mmio() {
        let mut p = platform();
        let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0);
        for b in b"HI\n" {
            assert_eq!(
                p.on_mmio(
                    machine::UART.base,
                    MmioOp::Write {
                        size: 1,
                        value: u64::from(*b)
                    },
                    &mut mem
                ),
                MmioOutcome::WriteAck
            );
        }
        assert_eq!(p.uart_output(), b"HI\n");
        // UARTFR (offset 0x18) reports idle FIFOs: TXFE and RXFE set.
        assert!(matches!(
            p.on_mmio(machine::UART.base + 0x18, MmioOp::Read { size: 4 }, &mut mem),
            MmioOutcome::ReadValue(v) if v & ((1 << 7) | (1 << 4)) == ((1 << 7) | (1 << 4))
        ));
    }

    #[test]
    fn uart_reads_consume_preloaded_input_via_mmio() {
        let mut p = platform();
        let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0);
        p.push_uart_input(b" ");
        assert_eq!(p.uart_input_len(), 1);
        assert!(matches!(
            p.on_mmio(machine::UART.base + 0x18, MmioOp::Read { size: 4 }, &mut mem),
            MmioOutcome::ReadValue(v) if v & (1 << 4) == 0
        ));
        assert_eq!(
            p.on_mmio(machine::UART.base, MmioOp::Read { size: 1 }, &mut mem),
            MmioOutcome::ReadValue(u64::from(b' '))
        );
        assert_eq!(p.uart_input_len(), 0);
        assert!(matches!(
            p.on_mmio(machine::UART.base + 0x18, MmioOp::Read { size: 4 }, &mut mem),
            MmioOutcome::ReadValue(v) if v & (1 << 4) != 0
        ));
    }

    #[test]
    fn rtc_data_and_id_registers_are_modelled() {
        let mut p = platform();
        let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0);
        assert_eq!(
            p.on_mmio(
                machine::RTC.base + 0xfe0,
                MmioOp::Read { size: 4 },
                &mut mem
            ),
            MmioOutcome::ReadValue(0x31)
        );
        match p.on_mmio(machine::RTC.base, MmioOp::Read { size: 4 }, &mut mem) {
            MmioOutcome::ReadValue(value) => assert!(value > 1_600_000_000),
            other => panic!("unexpected RTC read outcome: {other:?}"),
        }
        assert_eq!(
            p.on_mmio(
                machine::RTC.base + 0x008,
                MmioOp::Write {
                    size: 4,
                    value: 0x2026_0619,
                },
                &mut mem
            ),
            MmioOutcome::WriteAck
        );
        assert_eq!(
            p.on_mmio(machine::RTC.base, MmioOp::Read { size: 4 }, &mut mem),
            MmioOutcome::ReadValue(0x2026_0619)
        );
    }

    #[test]
    fn flash_vars_routes_nor_status_protocol() {
        let mut p = platform();
        let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0);
        p.load_flash_vars(&[0x78, 0x56, 0x34, 0x12]);
        assert_eq!(
            p.on_mmio(machine::FLASH_VARS.base, MmioOp::Read { size: 4 }, &mut mem),
            MmioOutcome::ReadValue(0x1234_5678)
        );
        assert_eq!(
            p.on_mmio(
                machine::FLASH_VARS.base,
                MmioOp::Write {
                    size: 4,
                    value: 0x0070_0070,
                },
                &mut mem
            ),
            MmioOutcome::WriteAck
        );
        assert_eq!(
            p.on_mmio(machine::FLASH_VARS.base, MmioOp::Read { size: 4 }, &mut mem),
            MmioOutcome::ReadValue(0x0080_0080)
        );
    }

    #[test]
    fn flash_vars_snapshot_reflects_guest_programming() {
        let mut p = platform();
        let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0);
        p.load_flash_vars(&[0xff; 8]);
        assert_eq!(p.flash_vars_image()[0], 0xff);

        assert_eq!(
            p.on_mmio(
                machine::FLASH_VARS.base,
                MmioOp::Write {
                    size: 4,
                    value: 0x0040_0040,
                },
                &mut mem,
            ),
            MmioOutcome::WriteAck
        );
        assert_eq!(
            p.on_mmio(
                machine::FLASH_VARS.base,
                MmioOp::Write {
                    size: 4,
                    value: 0x1234_5678,
                },
                &mut mem,
            ),
            MmioOutcome::WriteAck
        );
        assert_eq!(&p.flash_vars_image()[0..4], &[0x78, 0x56, 0x34, 0x12]);
    }

    #[test]
    fn generated_acpi_tables_are_registered_by_default() {
        let mut p = platform();
        p.fw_cfg.select(crate::fwcfg::KEY_FILE_DIR);
        let dir = p.fw_cfg.read_data(p.fw_cfg.file_dir_bytes().len());
        let blob = String::from_utf8_lossy(&dir);
        for name in [ACPI_RSDP_FILE, ACPI_TABLE_FILE, ACPI_LOADER_FILE] {
            assert!(blob.contains(name), "default fw_cfg dir missing {name}");
        }
    }

    #[test]
    fn generated_smbios_tables_are_registered_by_default() {
        let mut p = platform();
        p.fw_cfg.select(crate::fwcfg::KEY_FILE_DIR);
        let dir = p.fw_cfg.read_data(p.fw_cfg.file_dir_bytes().len());
        let blob = String::from_utf8_lossy(&dir);
        for name in [SMBIOS_ANCHOR_FILE, SMBIOS_TABLE_FILE] {
            assert!(blob.contains(name), "default fw_cfg dir missing {name}");
        }
    }

    #[test]
    fn default_fw_cfg_matches_qemu_display_none_without_ramfb_file() {
        let mut p = platform();

        assert_eq!(find_fw_cfg_file_entry(&mut p, b"etc/ramfb"), None);
        assert_eq!(p.ramfb_config(), None);
    }

    #[test]
    fn ramfb_opt_in_registers_qemu_ramfb_file() {
        let mut p = platform_with_ramfb();
        let (_, size) = fw_cfg_file_entry(&mut p, b"etc/ramfb");

        assert_eq!(size, RAMFB_CONFIG_SIZE);
        assert_eq!(p.ramfb_config(), None);
    }

    #[test]
    fn platform_device_disable_omits_ramfb_fw_cfg_surface() {
        let mut p = platform_with_devices(VirtPlatformDeviceConfig {
            ramfb_present: false,
            ..VirtPlatformDeviceConfig::default()
        });
        let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0x1000);
        let ctrl = machine::RAM_BASE + 0x200;
        let config = [0u8; RAMFB_CONFIG_SIZE];

        assert_eq!(find_fw_cfg_file_entry(&mut p, b"etc/ramfb"), None);
        assert!(mem.write_bytes(ctrl, &config));
        assert_eq!(
            p.on_mmio(
                machine::FW_CFG.base + REG_DMA,
                MmioOp::Write {
                    size: 8,
                    value: ctrl.swap_bytes()
                },
                &mut mem
            ),
            MmioOutcome::WriteAck
        );
        assert_eq!(p.ramfb_config(), None);
    }

    #[test]
    fn flat_guest_ram_rejects_ranges_that_overflow_host_offset() {
        let mut ram = FlatGuestRam::new(0, 16);
        let overflowing_gpa = usize::MAX as u64;

        assert_eq!(ram.read_bytes(overflowing_gpa, 2), None);
        assert!(!ram.write_bytes(overflowing_gpa, &[1, 2]));
    }

    #[test]
    fn fw_cfg_dma_write_updates_ramfb_config() {
        let mut p = platform_with_ramfb();
        let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0x1000);
        let (selector, size) = fw_cfg_file_entry(&mut p, b"etc/ramfb");
        let src = machine::RAM_BASE + 0x100;
        let ctrl = machine::RAM_BASE + 0x200;
        let mut config = [0u8; RAMFB_CONFIG_SIZE];
        config[0..8].copy_from_slice(&0x4010_0000u64.to_be_bytes());
        config[8..12].copy_from_slice(&DRM_FORMAT_XRGB8888.to_be_bytes());
        config[12..16].copy_from_slice(&0u32.to_be_bytes());
        config[16..20].copy_from_slice(&1024u32.to_be_bytes());
        config[20..24].copy_from_slice(&768u32.to_be_bytes());
        config[24..28].copy_from_slice(&(1024u32 * 4).to_be_bytes());
        let control = (u32::from(selector) << 16) | DMA_CTL_SELECT | DMA_CTL_WRITE;
        let mut dma = Vec::new();
        dma.extend_from_slice(&control.to_be_bytes());
        dma.extend_from_slice(&(size as u32).to_be_bytes());
        dma.extend_from_slice(&src.to_be_bytes());
        assert!(mem.write_bytes(src, &config));
        assert!(mem.write_bytes(ctrl, &dma));

        let outcome = p.on_mmio(
            machine::FW_CFG.base + REG_DMA,
            MmioOp::Write {
                size: 8,
                value: ctrl.swap_bytes(),
            },
            &mut mem,
        );

        assert_eq!(outcome, MmioOutcome::WriteAck);
        assert_eq!(
            p.ramfb_config(),
            Some(RamfbConfig {
                addr: 0x4010_0000,
                fourcc: DRM_FORMAT_XRGB8888,
                flags: 0,
                width: 1024,
                height: 768,
                stride: 4096,
            })
        );
    }

    #[test]
    fn default_fw_cfg_bootorder_targets_qemu_virtio_blk_pci_installer() {
        let mut p = platform();
        let bootorder = fw_cfg_file_entry(&mut p, b"bootorder");
        assert_eq!(bootorder.1, bootorder::QEMU_VIRTIO_BLK_PCI_BOOTORDER.len());

        p.fw_cfg.select(bootorder.0);
        assert_eq!(
            p.fw_cfg.read_data(bootorder.1),
            bootorder::QEMU_VIRTIO_BLK_PCI_BOOTORDER
        );
    }

    #[test]
    fn linux_boot_blobs_register_qemu_numeric_fw_cfg_items() {
        let mut p = platform();
        p.set_linux_boot_blobs(
            b"kernel-image".to_vec(),
            Some(b"initrd-image".to_vec()),
            b"console=ttyAMA0\0".to_vec(),
        );

        p.fw_cfg.select(KEY_KERNEL_SIZE);
        assert_eq!(
            p.fw_cfg.mmio_read(0, 4),
            12,
            "QemuKernelLoaderFsDxe reads KERNEL_SIZE with QemuFwCfgRead32"
        );
        p.fw_cfg.select(KEY_KERNEL_DATA);
        assert_eq!(p.fw_cfg.read_data(12), b"kernel-image");

        p.fw_cfg.select(KEY_INITRD_SIZE);
        assert_eq!(p.fw_cfg.mmio_read(0, 4), 12);
        p.fw_cfg.select(KEY_INITRD_DATA);
        assert_eq!(p.fw_cfg.read_data(12), b"initrd-image");

        p.fw_cfg.select(KEY_CMDLINE_SIZE);
        assert_eq!(p.fw_cfg.mmio_read(0, 4), 16);
        p.fw_cfg.select(KEY_CMDLINE_DATA);
        assert_eq!(p.fw_cfg.read_data(16), b"console=ttyAMA0\0");

        p.fw_cfg.select(crate::fwcfg::KEY_FILE_DIR);
        let dir = p.fw_cfg.read_data(p.fw_cfg.file_dir_bytes().len());
        let blob = String::from_utf8_lossy(&dir);
        assert!(!blob.contains("kernel-image"));
        assert!(!blob.contains("initrd-image"));
    }

    #[test]
    fn acpi_and_smbios_tables_register_into_fw_cfg() {
        let mut p = platform();
        p.set_acpi_tables(vec![0xAA; 36], vec![0xBB; 100], vec![0xCC; 40]);
        p.set_smbios(vec![0x5F; 24], vec![0x01; 80]);
        let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0);
        // Read the FILE_DIR through fw_cfg and confirm the names are present.
        p.fw_cfg.select(crate::fwcfg::KEY_FILE_DIR);
        let dir = p.fw_cfg.read_data(p.fw_cfg.file_dir_bytes().len());
        let blob = String::from_utf8_lossy(&dir);
        for name in [
            "etc/acpi/rsdp",
            "etc/acpi/tables",
            "etc/table-loader",
            "etc/smbios/smbios-anchor",
            "etc/smbios/smbios-tables",
            "bootorder",
        ] {
            assert!(blob.contains(name), "fw_cfg dir missing {name}");
        }
        // Suppress unused-variable warning for `mem` in this assertion-only test.
        let _ = &mut mem;
    }

    #[test]
    fn report_pacing_zero_interval_is_unpaced() {
        let base = Instant::now();
        assert!(report_pacing_allows_emission(Duration::ZERO, None, base));
        assert!(report_pacing_allows_emission(
            Duration::ZERO,
            Some(base),
            base
        ));
        assert!(report_pacing_allows_emission(
            Duration::ZERO,
            Some(base + Duration::from_millis(1)),
            base
        ));
    }

    #[test]
    fn report_pacing_first_emission_allowed_then_gated_until_interval_elapses() {
        let base = Instant::now();
        let interval = Duration::from_millis(30);
        // Nothing emitted yet: the first report is always allowed.
        assert!(report_pacing_allows_emission(interval, None, base));
        // Just emitted at `base`: held off until the full interval passes.
        assert!(!report_pacing_allows_emission(interval, Some(base), base));
        assert!(!report_pacing_allows_emission(
            interval,
            Some(base),
            base + Duration::from_millis(29)
        ));
        assert!(report_pacing_allows_emission(
            interval,
            Some(base),
            base + Duration::from_millis(30)
        ));
        assert!(report_pacing_allows_emission(
            interval,
            Some(base),
            base + Duration::from_millis(31)
        ));
    }

    #[test]
    fn report_pacing_tolerates_now_before_last_emission() {
        // A non-monotonic clock (now earlier than the last emission) must not
        // underflow into "allowed"; saturating_duration_since yields zero.
        let base = Instant::now();
        let interval = Duration::from_millis(30);
        let last = base + Duration::from_millis(100);
        assert!(!report_pacing_allows_emission(interval, Some(last), base));
    }

    #[test]
    fn three_d_scanout_readback_defaults_to_display_cadence() {
        assert_eq!(
            virtio_gpu_3d_scanout_readback_interval_from_value(None),
            Duration::from_millis(DEFAULT_VIRTIO_GPU_3D_SCANOUT_READBACK_MS)
        );
        assert_eq!(
            virtio_gpu_3d_scanout_readback_interval_from_value(Some("invalid")),
            Duration::from_millis(DEFAULT_VIRTIO_GPU_3D_SCANOUT_READBACK_MS)
        );
    }

    #[test]
    fn host_vblank_pacing_is_opt_in_with_a_120_hz_configured_default() {
        assert_eq!(virtio_gpu_vblank_interval_from_value(None), Duration::ZERO);
        assert_eq!(
            virtio_gpu_vblank_interval_from_value(Some("0")),
            Duration::ZERO
        );
        assert_eq!(
            virtio_gpu_vblank_interval_from_value(Some("120")),
            Duration::from_nanos(8_333_333)
        );
        assert_eq!(
            virtio_gpu_vblank_interval_from_value(Some("invalid")),
            Duration::from_nanos(8_333_333)
        );
    }

    #[test]
    fn three_d_scanout_readback_allows_explicit_pacing_and_unpaced_debugging() {
        assert_eq!(
            virtio_gpu_3d_scanout_readback_interval_from_value(Some("33")),
            Duration::from_millis(33)
        );
        assert_eq!(
            virtio_gpu_3d_scanout_readback_interval_from_value(Some("0")),
            Duration::ZERO
        );
    }

    #[test]
    fn opt_in_hda_pci_bar_routes_controller_mmio() {
        let mut p = platform_with_devices(VirtPlatformDeviceConfig {
            hda_present: true,
            ..VirtPlatformDeviceConfig::default()
        });
        let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0x10000);
        let bar = machine::PCIE_MMIO_32.base + 0x70_000;
        let cfg = |reg| pcie_cfg_gpa(crate::pcie::HDA_BDF.1, crate::pcie::HDA_BDF.2, reg);

        assert_eq!(
            p.on_mmio(
                cfg(crate::pcie::REG_VENDOR_DEVICE),
                MmioOp::Read { size: 4 },
                &mut mem
            ),
            MmioOutcome::ReadValue(0x2668_8086)
        );
        assert_eq!(
            p.on_mmio(
                cfg(crate::pcie::REG_BAR0),
                MmioOp::Write {
                    size: 4,
                    value: bar
                },
                &mut mem,
            ),
            MmioOutcome::WriteAck
        );
        assert_eq!(
            p.on_mmio(
                cfg(crate::pcie::REG_COMMAND_STATUS),
                MmioOp::Write {
                    size: 2,
                    value: u64::from(crate::pcie::CMD_MEMORY_SPACE | crate::pcie::CMD_BUS_MASTER),
                },
                &mut mem,
            ),
            MmioOutcome::WriteAck
        );
        assert_eq!(
            p.on_mmio(
                bar + crate::hda::REG_GCAP,
                MmioOp::Read { size: 2 },
                &mut mem
            ),
            MmioOutcome::ReadValue(0x1001)
        );
        assert_eq!(
            p.on_mmio(
                bar + crate::hda::REG_GCTL,
                MmioOp::Write { size: 4, value: 1 },
                &mut mem,
            ),
            MmioOutcome::WriteAck
        );
        assert_eq!(
            p.on_mmio(
                bar + crate::hda::REG_STATESTS,
                MmioOp::Read { size: 2 },
                &mut mem
            ),
            MmioOutcome::ReadValue(1)
        );
    }

    #[test]
    fn poll_hda_routes_stream_ioc_through_standard_msi_aggregation() {
        let mut p = platform_with_devices(VirtPlatformDeviceConfig {
            hda_present: true,
            ..VirtPlatformDeviceConfig::default()
        });
        let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0x10000);
        let hda_bar = machine::PCIE_MMIO_32.base + 0x70_000;
        let cfg = |reg| pcie_cfg_gpa(crate::pcie::HDA_BDF.1, crate::pcie::HDA_BDF.2, reg);

        assert_eq!(
            p.on_mmio(
                cfg(crate::pcie::REG_BAR0),
                MmioOp::Write {
                    size: 4,
                    value: hda_bar,
                },
                &mut mem,
            ),
            MmioOutcome::WriteAck
        );
        assert_eq!(
            p.on_mmio(
                cfg(crate::pcie::REG_COMMAND_STATUS),
                MmioOp::Write {
                    size: 2,
                    value: u64::from(crate::pcie::CMD_MEMORY_SPACE | crate::pcie::CMD_BUS_MASTER),
                },
                &mut mem,
            ),
            MmioOutcome::WriteAck
        );

        let msi = u16::from(crate::pcie::HDA_MSI_CAP_OFFSET);
        let message_address = 0x0000_0001_0808_4000u64;
        assert_eq!(
            p.on_mmio(
                cfg(msi + 4),
                MmioOp::Write {
                    size: 4,
                    value: message_address as u32 as u64,
                },
                &mut mem,
            ),
            MmioOutcome::WriteAck
        );
        assert_eq!(
            p.on_mmio(
                cfg(msi + 8),
                MmioOp::Write {
                    size: 4,
                    value: message_address >> 32,
                },
                &mut mem,
            ),
            MmioOutcome::WriteAck
        );
        assert_eq!(
            p.on_mmio(
                cfg(msi + 12),
                MmioOp::Write {
                    size: 2,
                    value: 0x61,
                },
                &mut mem,
            ),
            MmioOutcome::WriteAck
        );
        assert_eq!(
            p.on_mmio(
                cfg(msi + 2),
                MmioOp::Write {
                    size: 2,
                    value: 0x0001,
                },
                &mut mem,
            ),
            MmioOutcome::WriteAck
        );

        let bdl = machine::RAM_BASE + 0x1000;
        let pcm = machine::RAM_BASE + 0x2000;
        let pcm_bytes = vec![0x5a; 192];
        assert!(mem.write_bytes(pcm, &pcm_bytes));
        let mut descriptor = [0u8; 16];
        descriptor[..8].copy_from_slice(&pcm.to_le_bytes());
        descriptor[8..12].copy_from_slice(&(pcm_bytes.len() as u32).to_le_bytes());
        descriptor[12..16].copy_from_slice(&1u32.to_le_bytes());
        assert!(mem.write_bytes(bdl, &descriptor));

        let hda = p.hda.as_mut().expect("opt-in HDA controller");
        hda.mmio_write(crate::hda::REG_GCTL, 4, 1, &mut mem);
        hda.mmio_write(crate::hda::REG_SD_BDPL, 4, bdl, &mut mem);
        hda.mmio_write(crate::hda::REG_SD_CBL, 4, pcm_bytes.len() as u64, &mut mem);
        hda.mmio_write(crate::hda::REG_SD_LVI, 2, 0, &mut mem);
        hda.mmio_write(crate::hda::REG_SD_FMT, 2, 0x0011, &mut mem);
        hda.mmio_write(
            crate::hda::REG_INTCTL,
            4,
            u64::from((1u32 << 31) | 1),
            &mut mem,
        );
        hda.mmio_write(crate::hda::REG_SD_CTL, 1, 0x06, &mut mem);

        p.poll_hda(&mut mem);
        let mut messages = Vec::new();
        p.drain_pending_msix_into(&mut messages);
        assert_eq!(
            messages,
            vec![MsixMessage {
                vector: 0,
                address: message_address,
                data: 0x61,
            }]
        );
        assert!(
            p.take_pending_spi_levels().is_empty(),
            "enabled HDA MSI must not touch legacy PCI INTA"
        );
    }
}
