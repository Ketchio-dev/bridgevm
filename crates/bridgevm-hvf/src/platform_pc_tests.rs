use super::*;
use crate::machine;
use crate::machine::Region;
use crate::pcie::{HDA_BDF, NVME_BDF, VIRTIO_CONSOLE_BDF, VIRTIO_GPU_BDF, XHCI_BDF};

fn ecam_offset(bdf: (u8, u8, u8)) -> u64 {
    (u64::from(bdf.0) << 20) | (u64::from(bdf.1) << 15) | (u64::from(bdf.2) << 12)
}

#[test]
fn memory_layout_uses_only_the_new_board_contract() {
    let layout = BridgeVmPcPlatform::memory_layout(8 << 30).expect("8 GiB layout");
    assert_eq!(layout.ram, Region::new(board::RAM_BASE, 8 << 30));
    assert_eq!(layout.flash_vars, board::FLASH_VARS);
    assert_eq!(BridgeVmPcPlatform::memory_layout(0), None);
}

#[test]
fn uart_and_rtc_route_at_new_addresses_only() {
    let mut platform = BridgeVmPcPlatform::new();
    assert_eq!(
        platform.on_mmio_without_dma(
            board::UART.base,
            MmioOp::Write {
                size: 1,
                value: b'B' as u64
            },
        ),
        MmioOutcome::WriteAck
    );
    assert_eq!(platform.uart_output(), b"B");
    assert_eq!(
        platform.on_mmio_without_dma(board::RTC.base + 0xfe0, MmioOp::Read { size: 4 }),
        MmioOutcome::ReadValue(0x31)
    );
    assert_eq!(
        platform.on_mmio_without_dma(machine::UART.base, MmioOp::Read { size: 4 }),
        MmioOutcome::Unmapped
    );
}

#[test]
fn new_ecam_exposes_the_reused_pcie_topology() {
    let mut platform = BridgeVmPcPlatform::new();
    for bdf in [
        NVME_BDF,
        XHCI_BDF,
        VIRTIO_GPU_BDF,
        VIRTIO_CONSOLE_BDF,
        HDA_BDF,
    ] {
        let result = platform.on_mmio_without_dma(
            board::PCIE_ECAM.base + ecam_offset(bdf),
            MmioOp::Read { size: 2 },
        );
        assert_ne!(result, MmioOutcome::ReadValue(0xffff), "missing {bdf:?}");
    }
}

#[test]
fn unfinished_devices_fail_as_known_board_boundaries() {
    let mut platform = BridgeVmPcPlatform::new();
    assert_eq!(
        platform.on_mmio_without_dma(board::GIC_DIST.base, MmioOp::Read { size: 4 }),
        MmioOutcome::KnownUnimplemented("gic-dist")
    );
    assert_eq!(
        platform.on_mmio_without_dma(board::BOOT_INFO.base, MmioOp::Read { size: 4 }),
        MmioOutcome::KnownUnimplemented("boot-info")
    );
}
