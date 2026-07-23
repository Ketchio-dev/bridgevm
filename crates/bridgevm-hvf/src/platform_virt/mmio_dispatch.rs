//! Machine-map address decode and routing of every guest MMIO/PIO access to the owning device.

use super::*;
use crate::fwcfg::GuestMemoryMut;
use crate::machine;
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
use crate::virtio_blk::VirtioMmioBlockResult;
use crate::virtio_blk::INSTALLER_ISO_SLOT;

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

impl VirtPlatform {
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
}
