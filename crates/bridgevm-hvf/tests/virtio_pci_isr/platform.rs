//! The advertised ISR capability must also acknowledge through the platform bus.
use bridgevm_hvf::{
    dtb::VirtFdtConfig,
    machine, pcie,
    platform_virt::{
        FlatGuestRam, MmioOp, MmioOutcome, VirtPlatform, VirtPlatformConfig,
        VirtPlatformDeviceConfig,
    },
};

#[test]
fn pci_bar_isr_read_clears_gpu_config_change() {
    let mut platform = VirtPlatform::new_with_config(VirtPlatformConfig {
        fdt: VirtFdtConfig::default(),
        devices: VirtPlatformDeviceConfig {
            virtio_gpu_present: true,
            ..Default::default()
        },
    });
    let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0x1000);
    let bar = machine::PCIE_MMIO_32.base + 0x80_000;
    let bdf = pcie::VIRTIO_GPU_BDF;
    let config = machine::PCIE_ECAM.base + (u64::from(bdf.1) << 15) + (u64::from(bdf.2) << 12);
    for (reg, size, value) in [
        (pcie::REG_BAR0 + 4 * 4, 4, bar),
        (pcie::REG_BAR0 + 5 * 4, 4, 0),
        (
            pcie::REG_COMMAND_STATUS,
            2,
            u64::from(pcie::CMD_MEMORY_SPACE),
        ),
    ] {
        assert_eq!(
            platform.on_mmio(
                config + u64::from(reg),
                MmioOp::Write { size, value },
                &mut mem
            ),
            MmioOutcome::WriteAck
        );
    }
    assert!(platform.request_virtio_gpu_resolution(1920, 1080));
    for (offset, size) in [(0x1001, 1), (0x1002, 2), (0x1004, 4)] {
        assert_eq!(
            platform.on_mmio(bar + offset, MmioOp::Read { size }, &mut mem),
            MmioOutcome::ReadValue(0)
        );
        assert_eq!(platform.virtio_gpu_stats().unwrap().interrupt_status, 2);
    }
    assert_eq!(
        platform.on_mmio(bar + 0x1000, MmioOp::Read { size: 1 }, &mut mem),
        MmioOutcome::ReadValue(2)
    );
    assert_eq!(platform.virtio_gpu_stats().unwrap().interrupt_status, 0);
    assert_eq!(
        platform.on_mmio(bar + 0x1000, MmioOp::Read { size: 1 }, &mut mem),
        MmioOutcome::ReadValue(0)
    );
}
