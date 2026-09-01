use super::*;
use crate::nvme::{NVME_VERSION_1_4_0, REG_CAP, REG_VS};
use crate::pcie::{CMD_BUS_MASTER, CMD_MEMORY_SPACE, REG_BAR0, REG_COMMAND_STATUS};

fn ecam(register: u16) -> u64 {
    board::PCIE_ECAM.base + (1 << 15) + u64::from(register)
}

fn assign_nvme_bar0(platform: &mut BridgeVmPcPlatform, base: u64) {
    for (register, value) in [(REG_BAR0, base as u32), (REG_BAR0 + 4, (base >> 32) as u32)] {
        assert_eq!(
            platform.on_mmio_without_dma(
                ecam(register),
                MmioOp::Write {
                    size: 4,
                    value: u64::from(value),
                },
            ),
            MmioOutcome::WriteAck
        );
    }
    assert_eq!(
        platform.on_mmio_without_dma(
            ecam(REG_COMMAND_STATUS),
            MmioOp::Write {
                size: 2,
                value: u64::from(CMD_MEMORY_SPACE | CMD_BUS_MASTER),
            },
        ),
        MmioOutcome::WriteAck
    );
}

#[test]
fn assigned_nvme_bar0_routes_real_controller_registers() {
    let mut platform = BridgeVmPcPlatform::new();
    let base = board::PCIE_MMIO_32.base;
    assert_eq!(
        platform.on_mmio_without_dma(base + REG_VS, MmioOp::Read { size: 4 }),
        MmioOutcome::KnownUnimplemented("pcie-mmio-32")
    );
    assign_nvme_bar0(&mut platform, base);
    assert_eq!(
        platform.on_mmio_without_dma(base + REG_CAP, MmioOp::Read { size: 8 }),
        MmioOutcome::ReadValue(0x20_0201_03ff)
    );
    assert_eq!(
        platform.on_mmio_without_dma(base + REG_VS, MmioOp::Read { size: 4 }),
        MmioOutcome::ReadValue(u64::from(NVME_VERSION_1_4_0))
    );
    platform.reset_runtime_state();
    assert_eq!(
        platform.on_mmio_without_dma(base + REG_VS, MmioOp::Read { size: 4 }),
        MmioOutcome::KnownUnimplemented("pcie-mmio-32")
    );
}

#[test]
fn assigned_high_nvme_bar0_uses_the_high_aperture() {
    let mut platform = BridgeVmPcPlatform::new();
    let base = board::PCIE_MMIO_64.base;
    assign_nvme_bar0(&mut platform, base);
    assert_eq!(
        platform.on_mmio_without_dma(base + REG_VS, MmioOp::Read { size: 4 }),
        MmioOutcome::ReadValue(u64::from(NVME_VERSION_1_4_0))
    );
}
