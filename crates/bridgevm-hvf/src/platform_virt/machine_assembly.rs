//! The VirtPlatform struct, its construction and reset: fw_cfg control files, ACPI/SMBIOS/DTB, PCIe and device instantiation.

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
use crate::msix::MsixMessage;
use crate::nvme::NvmeCompletionEvent;
use crate::nvme::NvmeController;
use crate::pcie::PcieEcam;
use crate::pcie::PcieEcamConfig;
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
use crate::tpm_ppi::TPM_PPI_FW_CFG_FILE;
use crate::tpm_tis::Tpm2Backend;
use crate::tpm_tis::TpmTis;
use crate::virtio_blk::VirtioMmioBlock;
use crate::virtio_blk::VirtioPciBlock;
use crate::virtio_blk::INSTALLER_ISO_SLOT;
use crate::virtio_console::VirtioPciConsole;
use crate::virtio_gpu::VirtioPciGpu;
use crate::virtio_net::VirtioPciNet;
use crate::xhci::XhciController;
use std::time::Duration;
use std::time::Instant;

pub(crate) const DEFAULT_NVME_DISK_BYTES: usize = 16 * 1024 * 1024;

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

    /// Feed the current host time to the platform once per run-loop iteration.
    /// The report-pacing gate reads this; the `Instant::now()` call itself stays
    /// in the probe so this crate holds no clock and unit tests stay
    /// deterministic (they never call this, so pacing is inert).
    pub fn set_host_now(&mut self, now: Instant) {
        self.host_now = Some(now);
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
}
