//! Continuation of the `default_nvme_disk_bytes` impl block, split for the 1000-line rule.

use super::*;

use crate::acpi::ACPI_LOADER_FILE;
use crate::acpi::ACPI_RSDP_FILE;
use crate::acpi::ACPI_TABLE_FILE;
use crate::fwcfg::GuestMemoryMut;
use crate::fwcfg::KEY_CMDLINE_DATA;
use crate::fwcfg::KEY_CMDLINE_SIZE;
use crate::fwcfg::KEY_INITRD_DATA;
use crate::fwcfg::KEY_INITRD_SIZE;
use crate::fwcfg::KEY_KERNEL_DATA;
use crate::fwcfg::KEY_KERNEL_SIZE;
use crate::machine;
use crate::machine::Region;
use crate::msix::MsixMessage;
use crate::nvme::NvmeCommandTrace;
use crate::nvme::REG_CC;
use crate::nvme::REG_DOORBELL_BASE;
use crate::pcie::CfgAddr;
use crate::pcie::PcieMmioTarget;
use crate::pcie::PciePioTarget;
use crate::pcie::HDA_BDF;
use crate::pcie::NVME_BDF;
use crate::pcie::VIRTIO_BLK_BDF;
use crate::pcie::VIRTIO_CONSOLE_BDF;
use crate::pcie::VIRTIO_GPU_BDF;
use crate::pcie::VIRTIO_NET_BDF;
use crate::pcie::XHCI_BDF;
use crate::smbios::SMBIOS_ANCHOR_FILE;
use crate::smbios::SMBIOS_TABLE_FILE;
use crate::virtio_blk::VirtioMmioBlockResult;
use crate::virtio_blk::VirtioPciBlockOp;
use crate::virtio_blk::INSTALLER_ISO_SLOT;
use crate::virtio_gpu::VirtioGpuResult;
use crate::virtio_gpu::VirtioPciGpuOp;
use crate::virtio_net::VirtioNetResult;
use crate::virtio_net::VirtioPciNetOp;
use std::io;

impl VirtPlatform {
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
    pub(crate) fn virtio_mmio_access(
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
    pub(crate) fn pcie_access(&mut self, ecam_offset: u64, op: MmioOp) -> MmioOutcome {
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

    pub(crate) fn pcie_pio_access(
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

    pub(crate) fn pcie_mmio_access(
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

    pub(crate) fn pci_boot_media_msix_access(&mut self, offset: u64, op: MmioOp) -> MmioOutcome {
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

    pub(crate) fn pci_boot_media_access(
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

    pub(crate) fn virtio_net_msix_access(&mut self, offset: u64, op: MmioOp) -> MmioOutcome {
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

    pub(crate) fn virtio_net_access(
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

    pub(crate) fn virtio_gpu_msix_access(&mut self, offset: u64, op: MmioOp) -> MmioOutcome {
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

    pub(crate) fn virtio_gpu_access(
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
}
