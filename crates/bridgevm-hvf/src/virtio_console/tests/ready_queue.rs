//! Existing processing fixtures explicitly opt into initialized queue use.

use super::super::*;
use super::helpers::*;

pub(super) fn setup_ready_queue(
    dev: &mut VirtioPciConsole,
    mem: &mut TestMem,
    queue: u16,
    desc: u64,
    avail: u64,
    used: u64,
    vector: u16,
) {
    setup_queue(dev, mem, queue, desc, avail, used, vector);
    pci_write(dev, COMMON_DEVICE_STATUS, 1, 0x0f, mem);
}

pub(super) fn setup_ready_queue_sized(
    dev: &mut VirtioPciConsole,
    mem: &mut TestMem,
    queue: u16,
    ring: TestRing,
    used: u64,
    vector: u16,
    size: u16,
) {
    setup_queue_sized(dev, mem, queue, ring, used, vector, size);
    pci_write(dev, COMMON_DEVICE_STATUS, 1, 0x0f, mem);
}
