//! Split out of platform_virt.rs to keep files under 850 lines.

use super::*;
use crate::acpi::build_acpi_with_devices;
use crate::acpi::AcpiDeviceConfig;
use crate::acpi::ACPI_LOADER_FILE;
use crate::acpi::ACPI_RSDP_FILE;
use crate::acpi::ACPI_TABLE_FILE;
use crate::acpi::ACPI_TPM_LOG_FILE;
use crate::dtb::build_virt_fdt_with_devices;
use crate::dtb::VirtFdtConfig;
use crate::dtb::VirtFdtDeviceConfig;
use crate::fwcfg::FwCfg;
use crate::hda::HdaController;
use crate::machine;
use crate::machine::Region;
use crate::msix::MsixMessage;
use crate::net_nat::HostSocketOutboundIpv4Handler;
use crate::net_nat::NatBackend;
use crate::net_nat::NatStats;
use crate::nvme::NvmeCompletionEvent;
use crate::nvme::NvmeController;
use crate::pcie::PcieEcam;
use crate::pcie::PcieEcamConfig;
use crate::pcie::VIRTIO_GPU_DEVICE_ID;
use crate::pflash::P30NorFlash;
use crate::pl011::Pl011;
use crate::pl031::Pl031;
use crate::ramfb::Ramfb;
use crate::ramfb::RAMFB_CONFIG_SIZE;
use crate::ramfb::RAMFB_FW_CFG_FILE;
use crate::smbios::build_smbios;
use crate::smbios::SMBIOS_ANCHOR_FILE;
use crate::smbios::SMBIOS_TABLE_FILE;
use crate::tpm_ppi::build_qemu_fw_cfg_tpm_config;
use crate::tpm_ppi::TpmPpi;
use crate::tpm_ppi::TpmPpiStats;
use crate::tpm_ppi::TPM_PPI_FW_CFG_FILE;
use crate::tpm_tis::Tpm2Backend;
use crate::tpm_tis::TpmTis;
use crate::tpm_tis::TpmTisStats;
use crate::virtio_blk::VirtioBlockRequestTrace;
use crate::virtio_blk::VirtioMmioBlock;
use crate::virtio_blk::VirtioMmioBlockStats;
use crate::virtio_blk::VirtioPciBlock;
use crate::virtio_blk::INSTALLER_ISO_SLOT;
use crate::virtio_console::VirtioConsoleStats;
use crate::virtio_console::VirtioPciConsole;
use crate::virtio_gpu::VblankWakeState;
use crate::virtio_gpu::VirtioGpuScanout;
use crate::virtio_gpu::VirtioGpuStats;
use crate::virtio_gpu::VirtioPciGpu;
use crate::virtio_net::LoopbackTestBackend;
use crate::virtio_net::NetBackend;
use crate::virtio_net::VirtioNetStats;
use crate::virtio_net::VirtioPciNet;
use crate::xhci::XhciController;
use std::io;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

pub(crate) const DEFAULT_NVME_DISK_BYTES: usize = 16 * 1024 * 1024;
pub(crate) const HID_BOOT_KEYBOARD_USAGE_SPACE: u8 = 0x2c;
pub(crate) const MAX_XHCI_SETUP_INPUT_DRAIN_ATTEMPTS: usize = 16;
#[cfg(any(feature = "venus", test))]
pub(crate) const DEFAULT_VIRTIO_GPU_3D_SCANOUT_READBACK_MS: u64 = 16;
pub(crate) const DEFAULT_VIRTIO_GPU_VBLANK_HZ: u64 = 120;

pub(crate) fn make_virtio_gpu() -> VirtioPciGpu {
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

pub(crate) fn configure_virtio_gpu_vblank(gpu: &mut VirtioPciGpu) {
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

pub(crate) fn virtio_gpu_vblank_interval_from_value(value: Option<&str>) -> Duration {
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

pub(crate) fn virtio_gpu_3d_enabled_for_pcie() -> bool {
    cfg!(feature = "venus") && env_flag("BRIDGEVM_VIRTIO_GPU_3D")
}

#[cfg(any(feature = "venus", test))]
pub(crate) fn virtio_gpu_3d_scanout_readback_interval_from_value(value: Option<&str>) -> Duration {
    let interval_ms = value
        .and_then(|value| value.trim().parse::<u64>().ok())
        .unwrap_or(DEFAULT_VIRTIO_GPU_3D_SCANOUT_READBACK_MS);
    Duration::from_millis(interval_ms)
}

pub(crate) fn virtio_gpu_resolution_from_env() -> (u32, u32) {
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

pub(crate) fn env_flag(name: &str) -> bool {
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
pub(crate) enum PlatformNetBackend {
    Nat(Box<NatBackend<HostSocketOutboundIpv4Handler>>),
    Loopback(LoopbackTestBackend),
}

impl PlatformNetBackend {
    pub(crate) fn new(kind: VirtioNetBackendKind) -> Self {
        match kind {
            VirtioNetBackendKind::Nat => Self::Nat(Box::new(NatBackend::new_host_socket())),
            VirtioNetBackendKind::Loopback => Self::Loopback(LoopbackTestBackend::default()),
        }
    }

    pub(crate) fn nat_stats(&self) -> Option<NatStats> {
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

pub(crate) fn make_virtio_net_backend(kind: VirtioNetBackendKind) -> PlatformNetBackend {
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
pub(crate) fn venus_start_trace_gpu_bar_access(bar_index: usize, offset: u64, op: &MmioOp) {
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
    pub(crate) xhci_setup_input_attempted: bool,
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
    pub(crate) cfg: VirtFdtConfig,
    pub(crate) devices: VirtPlatformDeviceConfig,
    pub(crate) fw_cfg: FwCfg,
    pub(crate) uart: Pl011,
    pub(crate) rtc: Pl031,
    pub(crate) pcie: PcieEcam,
    pub(crate) nvme: NvmeController,
    pub(crate) xhci: XhciController,
    pub(crate) hda: Option<HdaController>,
    pub(crate) virtio_iso: Option<VirtioMmioBlock>,
    pub(crate) pci_boot_media: Option<VirtioPciBlock>,
    pub(crate) virtio_net: Option<VirtioPciNet<PlatformNetBackend>>,
    pub(crate) virtio_gpu: Option<VirtioPciGpu>,
    pub(crate) virtio_console: Option<VirtioPciConsole>,
    pub(crate) tpm_tis: Option<TpmTis>,
    pub(crate) tpm_ppi: Option<TpmPpi>,
    pub(crate) ramfb: Ramfb,
    pub(crate) flash_vars: P30NorFlash,
    pub(crate) pending_msix: Vec<MsixMessage>,
    pub(crate) pending_spi_levels: Vec<(u32, bool)>,
    pub(crate) nvme_completion_scratch: Vec<NvmeCompletionEvent>,
    pub(crate) xhci_hid_boot_key_report_stats: XhciHidBootKeyReportStats,
    pub(crate) nvme_ecam_touched: bool,
    pub(crate) nvme_mmio_reached: bool,
    pub(crate) nvme_cc_enabled: bool,
    pub(crate) nvme_admin_doorbell_rung: bool,
    pub(crate) dtb: Vec<u8>,
    // Minimum host-time spacing between consecutive HID interrupt-IN report
    // emissions. Windows drops keystrokes when many reports land microseconds
    // apart (the guest coalesces the interrupt-IN completions), so live runs
    // throttle emission; `Duration::ZERO` means no pacing (the default, so unit
    // tests drain a queued sequence in one call). The clock read stays in the
    // probe: it pushes `Instant::now()` in via `set_host_now`, and while
    // `host_now` is `None` (the unit-test default) the drain paths are unpaced.
    pub(crate) xhci_report_interval: Duration,
    pub(crate) host_now: Option<Instant>,
    pub(crate) xhci_dci3_last_emission: Option<Instant>,
    pub(crate) xhci_dci5_last_emission: Option<Instant>,
}

pub(crate) const _: fn() = || {
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
}
