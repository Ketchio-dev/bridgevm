//! Guest ISR-capability accesses on the modern virtio PCI functions.
//! Offset 0x1000 sits below the device-config window; decoding it must not
//! subtract that window's base (release builds keep overflow checks).

use super::super::*;
use super::helpers::{pcie_cfg_gpa, platform_with_devices};
use crate::machine;
use crate::pcie;

const BAR: u64 = machine::PCIE_MMIO_32.base + 0x80_000;
const ISR: u64 = 0x1000;

fn platform_with_bar4(bdf: (u8, u8, u8)) -> (VirtPlatform, FlatGuestRam) {
    let mut platform = platform_with_devices(VirtPlatformDeviceConfig {
        virtio_net_present: true,
        virtio_gpu_present: true,
        virtio_console_present: true,
        ..VirtPlatformDeviceConfig::default()
    });
    let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0x1000);
    for (reg, size, value) in [
        (pcie::REG_BAR0 + 4 * 4, 4, BAR),
        (pcie::REG_BAR0 + 5 * 4, 4, 0),
        (
            pcie::REG_COMMAND_STATUS,
            2,
            u64::from(pcie::CMD_MEMORY_SPACE),
        ),
    ] {
        let op = MmioOp::Write { size, value };
        assert_eq!(
            platform.on_mmio(pcie_cfg_gpa(bdf.1, bdf.2, reg), op, &mut mem),
            MmioOutcome::WriteAck
        );
    }
    (platform, mem)
}

fn isr(platform: &mut VirtPlatform, mem: &mut FlatGuestRam, op: MmioOp) -> MmioOutcome {
    platform.on_mmio(BAR + ISR, op, mem)
}

#[test]
fn isr_read_and_ack_complete_on_net_and_console() {
    for bdf in [pcie::VIRTIO_NET_BDF, pcie::VIRTIO_CONSOLE_BDF] {
        let (mut platform, mut mem) = platform_with_bar4(bdf);
        for size in [1, 4] {
            let read = isr(&mut platform, &mut mem, MmioOp::Read { size });
            assert_eq!(read, MmioOutcome::ReadValue(0), "{bdf:?}");
        }
        let ack = isr(&mut platform, &mut mem, MmioOp::Write { size: 1, value: 3 });
        assert_eq!(ack, MmioOutcome::WriteAck, "{bdf:?}");
    }
}

#[test]
fn gpu_isr_reports_and_acknowledges_a_config_change() {
    let (mut platform, mut mem) = platform_with_bar4(pcie::VIRTIO_GPU_BDF);
    assert_eq!(
        isr(&mut platform, &mut mem, MmioOp::Read { size: 1 }),
        MmioOutcome::ReadValue(0)
    );
    assert!(platform.request_virtio_gpu_resolution(1920, 1080));
    assert_eq!(
        isr(&mut platform, &mut mem, MmioOp::Read { size: 1 }),
        MmioOutcome::ReadValue(2)
    );
    let ack = MmioOp::Write { size: 1, value: 2 };
    assert_eq!(isr(&mut platform, &mut mem, ack), MmioOutcome::WriteAck);
    assert_eq!(platform.virtio_gpu_stats().unwrap().interrupt_status, 0);
}
