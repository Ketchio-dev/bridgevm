//! Minimal runtime for the experimental BridgeVM Virtual ARM PC.
//!
//! This intentionally remains separate from the shipping platform. It proves
//! that BridgeVM-owned device models can execute at the new board addresses;
//! GIC setup, firmware tables and guest boot are later live-gated boundaries.

use crate::machine::bridgevm_pc as board;
use crate::machine::Region;
use crate::pcie::{PcieEcam, PcieEcamConfig};
use crate::pflash::P30NorFlash;
use crate::pl011::Pl011;
use crate::pl031::Pl031;
use crate::platform_virt::{MmioOp, MmioOutcome};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BridgeVmPcMemoryLayout {
    pub flash_code: Region,
    pub flash_vars: Region,
    pub boot_info: Region,
    pub ram: Region,
}

#[derive(Debug)]
pub struct BridgeVmPcPlatform {
    uart: Pl011,
    rtc: Pl031,
    pcie: PcieEcam,
    pcie_config: PcieEcamConfig,
    flash_vars: P30NorFlash,
}

impl BridgeVmPcPlatform {
    /// Explicit opt-in: this experimental board must never be selected by default.
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
            flash_vars: P30NorFlash::new(
                board::FLASH_VARS.base,
                board::FLASH_VARS.size as usize,
                0x40000,
            ),
        }
    }

    pub fn memory_layout(ram_size: u64) -> Option<BridgeVmPcMemoryLayout> {
        Some(BridgeVmPcMemoryLayout {
            flash_code: board::FLASH_CODE,
            flash_vars: board::FLASH_VARS,
            boot_info: board::BOOT_INFO,
            ram: board::ram_region(ram_size)?,
        })
    }

    pub fn on_mmio(&mut self, gpa: u64, op: MmioOp) -> MmioOutcome {
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
        self.flash_vars.reset_runtime_state();
    }
}

#[cfg(test)]
#[path = "platform_pc_tests.rs"]
mod tests;
