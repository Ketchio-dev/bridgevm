use super::message;
use bridgevm_hvf::{
    dtb::VirtFdtConfig,
    machine, pcie,
    platform_virt::{
        FlatGuestRam, MmioOp, MmioOutcome, VirtPlatform, VirtPlatformConfig,
        VirtPlatformDeviceConfig,
    },
};

#[test]
fn platform_bar_and_ecam_unmask_deliver_one_pending_resize() {
    for function_masked in [false, true] {
        let mut platform = VirtPlatform::new_with_config(VirtPlatformConfig {
            fdt: VirtFdtConfig::default(),
            devices: VirtPlatformDeviceConfig {
                virtio_gpu_present: true,
                ..Default::default()
            },
        });
        let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0x1000);
        let bar = machine::PCIE_MMIO_32.base + 0x80_000;
        let msix_bar = bar + 0x10_000;
        let bdf = pcie::VIRTIO_GPU_BDF;
        let config = machine::PCIE_ECAM.base + (u64::from(bdf.1) << 15) + (u64::from(bdf.2) << 12);
        let control = config + u64::from(pcie::VIRTIO_GPU_MSIX_CAP_OFFSET) + 2;
        let table = msix_bar + u64::from(pcie::VIRTIO_GPU_MSIX_TABLE_OFFSET);
        let pba = msix_bar + u64::from(pcie::VIRTIO_GPU_MSIX_PBA_OFFSET);
        let write = |platform: &mut VirtPlatform, mem: &mut FlatGuestRam, address, size, value| {
            assert_eq!(
                platform.on_mmio(address, MmioOp::Write { size, value }, mem),
                MmioOutcome::WriteAck
            );
        };
        for (address, size, value) in [
            (config + u64::from(pcie::REG_BAR0) + 4, 4, msix_bar),
            (config + u64::from(pcie::REG_BAR0) + 16, 4, bar),
            (config + u64::from(pcie::REG_BAR0) + 20, 4, 0),
            (
                config + u64::from(pcie::REG_COMMAND_STATUS),
                2,
                u64::from(pcie::CMD_MEMORY_SPACE),
            ),
            (bar + 0x10, 2, 0),
            (table, 8, message(0).address),
            (table + 8, 4, u64::from(message(0).data)),
            (table + 12, 4, u64::from(!function_masked)),
            (control, 2, if function_masked { 0xc000 } else { 0x8000 }),
        ] {
            write(&mut platform, &mut mem, address, size, value);
        }
        assert!(platform.take_pending_msix().is_empty());
        assert!(platform.request_virtio_gpu_resolution(1920, 1080));
        platform.poll_virtio_gpu_fences(&mut mem);
        assert!(platform.take_pending_msix().is_empty());
        assert_eq!(
            platform.on_mmio(pba, MmioOp::Read { size: 8 }, &mut mem),
            MmioOutcome::ReadValue(1)
        );
        if function_masked {
            write(&mut platform, &mut mem, control, 2, 0x8000);
        } else {
            write(&mut platform, &mut mem, table + 12, 4, 0);
        }
        assert_eq!(platform.take_pending_msix(), vec![message(0)]);
        assert_eq!(
            platform.on_mmio(pba, MmioOp::Read { size: 8 }, &mut mem),
            MmioOutcome::ReadValue(0)
        );
        platform.poll_virtio_gpu_fences(&mut mem);
        assert!(platform.take_pending_msix().is_empty());
    }
}
