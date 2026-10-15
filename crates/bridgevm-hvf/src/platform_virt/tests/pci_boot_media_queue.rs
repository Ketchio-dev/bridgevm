//! Ready queue setup through the model's PCI BAR MMIO-register aliases.

use super::super::*;

pub(super) fn ready_pci_boot_media_queue(
    platform: &mut VirtPlatform,
    mem: &mut FlatGuestRam,
    bar: u64,
    desc: u64,
    avail: u64,
    used: u64,
) {
    const REG_QUEUE_NUM: u64 = 0x038;
    const REG_QUEUE_READY: u64 = 0x044;
    const REG_STATUS: u64 = 0x070;
    const REG_QUEUE_DESC_LOW: u64 = 0x080;
    const REG_QUEUE_DRIVER_LOW: u64 = 0x090;
    const REG_QUEUE_DEVICE_LOW: u64 = 0x0a0;

    for (reg, value) in [
        (REG_QUEUE_NUM, 8),
        (REG_QUEUE_DESC_LOW, desc),
        (REG_QUEUE_DRIVER_LOW, avail),
        (REG_QUEUE_DEVICE_LOW, used),
        (REG_QUEUE_READY, 1),
        (REG_STATUS, 0x0f),
    ] {
        assert_eq!(
            platform.on_mmio(bar + reg, MmioOp::Write { size: 4, value }, mem),
            MmioOutcome::WriteAck
        );
    }
}
