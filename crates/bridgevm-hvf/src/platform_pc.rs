//! Minimal runtime for the experimental BridgeVM Virtual ARM PC.
//!
//! This intentionally remains separate from the shipping platform. It proves
//! that BridgeVM-owned device models can execute at the new board addresses;
//! GIC setup, firmware tables and guest boot are later live-gated boundaries.
//! Construction is explicit opt-in and must never become the default silently.

#[path = "platform_pc_firmware.rs"]
mod firmware;
#[path = "platform_pc_layout.rs"]
mod layout;
#[path = "platform_pc_nvme.rs"]
mod nvme;
#[path = "platform_pc_pcie.rs"]
mod pcie;
#[path = "platform_pc_storage.rs"]
mod storage;

use crate::fwcfg::GuestMemoryMut;
use crate::machine::bridgevm_pc as board;
use crate::nvme::{NvmeCompletionEvent, NvmeController};
use crate::pcie::{PcieEcam, PcieEcamConfig};
use crate::pflash::P30NorFlash;
use crate::pl011::Pl011;
use crate::pl031::Pl031;
use crate::platform_virt::{MmioOp, MmioOutcome};
pub use firmware::*;
pub use layout::*;

#[derive(Debug)]
pub struct BridgeVmPcPlatform {
    uart: Pl011,
    rtc: Pl031,
    pcie: PcieEcam,
    pcie_config: PcieEcamConfig,
    nvme: NvmeController,
    nvme_completion_scratch: Vec<NvmeCompletionEvent>,
    flash_vars: P30NorFlash,
}

impl BridgeVmPcPlatform {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        assert_eq!(board::first_overlap(), None);
        let pcie_config = PcieEcamConfig {
            xhci_present: true,
            hda_present: true,
            virtio_blk_present: true,
            virtio_net_present: true,
            virtio_gpu_present: true,
            virtio_console_present: true,
            virtio_gpu_pci_device_id: crate::pcie::VIRTIO_GPU_DEVICE_ID,
            virtio_gpu_3d_enabled: false,
        };
        Self {
            uart: Pl011::new(),
            rtc: Pl031::new(),
            pcie: PcieEcam::new_with_config(pcie_config),
            pcie_config,
            nvme: NvmeController::new(0),
            nvme_completion_scratch: Vec::new(),
            flash_vars: P30NorFlash::new(
                board::FLASH_VARS.base,
                board::FLASH_VARS.size as usize,
                0x40000,
            ),
        }
    }

    pub fn on_mmio(&mut self, gpa: u64, op: MmioOp, mem: &mut dyn GuestMemoryMut) -> MmioOutcome {
        let Some((name, region)) = board::fixed_regions()
            .into_iter()
            .find(|(_, region)| region.contains(gpa))
        else {
            return MmioOutcome::Unmapped;
        };
        let offset = gpa - region.base;
        match name {
            "uart" => match op {
                MmioOp::Read { size } => MmioOutcome::ReadValue(self.uart.mmio_read(offset, size)),
                MmioOp::Write { size, value } => {
                    self.uart.mmio_write(offset, size, value);
                    MmioOutcome::WriteAck
                }
            },
            "rtc" => match op {
                MmioOp::Read { size } => MmioOutcome::ReadValue(self.rtc.mmio_read(offset, size)),
                MmioOp::Write { size, value } => {
                    self.rtc.mmio_write(offset, size, value);
                    MmioOutcome::WriteAck
                }
            },
            "flash-vars" => self.flash_vars.access(gpa, op),
            "pcie-ecam" => match op {
                MmioOp::Read { size } => MmioOutcome::ReadValue(self.pcie.cfg_read(offset, size)),
                MmioOp::Write { size, value } => {
                    self.pcie.cfg_write(offset, size, value);
                    MmioOutcome::WriteAck
                }
            },
            other @ ("pcie-mmio-32" | "pcie-mmio-64") => self.pcie_mmio_access(other, gpa, op, mem),
            other => MmioOutcome::KnownUnimplemented(other),
        }
    }

    pub fn load_vars(&mut self, image: &[u8]) {
        self.flash_vars.load(image);
    }

    pub fn vars_image(&self) -> &[u8] {
        self.flash_vars.image()
    }

    pub fn uart_output(&self) -> &[u8] {
        self.uart.output()
    }

    pub fn reset_runtime_state(&mut self) {
        self.uart = Pl011::new();
        self.rtc = Pl031::new();
        self.pcie = PcieEcam::new_with_config(self.pcie_config);
        self.nvme.reset_registers_keep_disks();
        self.nvme_completion_scratch.clear();
        self.flash_vars.reset_runtime_state();
    }
}

#[cfg(test)]
#[path = "platform_pc_nvme_tests.rs"]
mod nvme_tests;
#[cfg(test)]
#[path = "platform_pc_storage_tests.rs"]
mod storage_tests;
#[cfg(test)]
#[path = "platform_pc_test_support.rs"]
mod test_support;
#[cfg(test)]
#[path = "platform_pc_tests.rs"]
mod tests;
