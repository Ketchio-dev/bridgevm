//! Packet-processing fixtures explicitly enable DRIVER_OK after queue setup.
use super::super::*;
use super::helpers::*;

pub(super) fn setup_ready_queue(
    dev: &mut VirtioPciNet,
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
