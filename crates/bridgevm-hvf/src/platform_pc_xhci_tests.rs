use super::*;
use crate::pcie::{CMD_BUS_MASTER, CMD_MEMORY_SPACE, REG_BAR0, REG_COMMAND_STATUS, XHCI_BDF};
use crate::platform_virt::FlatGuestRam;

fn ecam(register: u16) -> u64 {
    board::PCIE_ECAM.base
        + (u64::from(XHCI_BDF.1) << 15)
        + (u64::from(XHCI_BDF.2) << 12)
        + u64::from(register)
}

fn write(platform: &mut BridgeVmPcPlatform, mem: &mut FlatGuestRam, gpa: u64, value: u64) {
    assert_eq!(
        platform.on_mmio(gpa, MmioOp::Write { size: 4, value }, mem),
        MmioOutcome::WriteAck
    );
}

#[test]
fn windows_style_high_xhci_bar_routes_controller_and_msix_registers() {
    let mut platform = BridgeVmPcPlatform::new();
    let mut mem = FlatGuestRam::new(board::RAM_BASE, 0x10000);
    let bar = board::PCIE_MMIO_64_NON_PREFETCH.end() - u64::from(crate::pcie::XHCI_BAR0_SIZE);
    write(&mut platform, &mut mem, ecam(REG_BAR0), bar as u32 as u64);
    write(&mut platform, &mut mem, ecam(REG_BAR0 + 4), bar >> 32);
    write(
        &mut platform,
        &mut mem,
        ecam(REG_COMMAND_STATUS),
        u64::from(CMD_MEMORY_SPACE | CMD_BUS_MASTER),
    );
    assert_eq!(
        platform.on_mmio(bar, MmioOp::Read { size: 1 }, &mut mem),
        MmioOutcome::ReadValue(u64::from(crate::xhci::XHCI_CAP_LENGTH))
    );
    assert_eq!(
        platform.on_mmio(bar + 0x3000, MmioOp::Read { size: 4 }, &mut mem),
        MmioOutcome::ReadValue(0)
    );
}
