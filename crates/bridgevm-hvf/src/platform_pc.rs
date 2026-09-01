//! Minimal runtime for the experimental BridgeVM Virtual ARM PC.
//!
//! This intentionally remains separate from the shipping platform. It proves
//! that BridgeVM-owned device models can execute at the new board addresses;
//! GIC setup, firmware tables and guest boot are later live-gated boundaries.
//! Construction is explicit opt-in and must never become the default silently.

#[path = "platform_pc_firmware.rs"]
mod firmware;
#[path = "platform_pc_interrupts.rs"]
mod interrupts;
#[path = "platform_pc_layout.rs"]
mod layout;
#[path = "platform_pc_nvme.rs"]
mod nvme;
#[path = "platform_pc_pcie.rs"]
mod pcie;
#[path = "platform_pc_runtime.rs"]
mod runtime;
#[path = "platform_pc_storage.rs"]
mod storage;
#[path = "platform_pc_xhci.rs"]
mod xhci;

use crate::fwcfg::GuestMemoryMut;
use crate::machine::bridgevm_pc as board;
use crate::msix::MsixMessage;
use crate::nvme::{NvmeCompletionEvent, NvmeController};
use crate::pcie::{PcieEcam, PcieEcamConfig};
use crate::pflash::P30NorFlash;
use crate::pl011::Pl011;
use crate::pl031::Pl031;
use crate::platform_virt::{MmioOp, MmioOutcome};
use crate::xhci::XhciController;
pub use firmware::*;
pub use layout::*;

#[derive(Debug)]
pub struct BridgeVmPcPlatform {
    uart: Pl011,
    rtc: Pl031,
    pcie: PcieEcam,
    pcie_config: PcieEcamConfig,
    nvme: NvmeController,
    xhci: XhciController,
    pending_msix: Vec<MsixMessage>,
    nvme_completion_scratch: Vec<NvmeCompletionEvent>,
    flash_vars: P30NorFlash,
}

impl BridgeVmPcPlatform {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        assert_eq!(board::first_overlap(), None);
        let pcie_config = PcieEcamConfig {
            xhci_present: true,
            hda_present: false,
            virtio_blk_present: false,
            virtio_net_present: false,
            virtio_gpu_present: false,
            virtio_console_present: false,
            virtio_gpu_pci_device_id: crate::pcie::VIRTIO_GPU_DEVICE_ID,
            virtio_gpu_3d_enabled: false,
        };
        Self {
            uart: Pl011::new(),
            rtc: Pl031::new(),
            pcie: PcieEcam::new_with_config(pcie_config),
            pcie_config,
            nvme: NvmeController::new(0),
            xhci: XhciController::new(),
            pending_msix: Vec::new(),
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
                    self.flush_nvme_pending_msix();
                    self.flush_xhci_pending_msix();
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
}

#[cfg(test)]
#[path = "platform_pc_inventory_tests.rs"]
mod inventory_tests;
#[cfg(test)]
#[path = "platform_pc_nvme_tests.rs"]
mod nvme_tests;
#[cfg(test)]
#[path = "platform_pc_pcie_tests.rs"]
mod pcie_tests;
#[cfg(test)]
#[path = "platform_pc_storage_tests.rs"]
mod storage_tests;
#[cfg(test)]
#[path = "platform_pc_test_support.rs"]
mod test_support;
#[cfg(test)]
#[path = "platform_pc_tests.rs"]
mod tests;
