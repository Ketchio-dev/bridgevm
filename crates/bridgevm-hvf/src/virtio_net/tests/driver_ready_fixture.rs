//! A modern PCI driver paused after queue population, before DRIVER_OK.
use super::super::*;
use super::driver_ready_status::initialize_status;
use super::helpers::*;

pub(super) const RX_USED: u64 = 0x4000_3000;
pub(super) const RX_DATA: u64 = 0x4000_4000;
pub(super) const TX_USED: u64 = 0x4000_7000;
pub(super) const FRAME: &[u8] = b"\x52\x54\x00\x42\x56\x01\x02\x00\x00\x00\x00\x01\x08\x00ready";

pub(super) fn initialize(dev: &mut VirtioPciNet, mem: &mut TestMem) {
    initialize_status(dev, mem);
    for (queue, desc, avail, used, data, flags) in [
        (0, 0x4000_1000, 0x4000_2000, RX_USED, RX_DATA, DESC_F_WRITE),
        (1, 0x4000_5000, 0x4000_6000, TX_USED, 0x4000_8000, 0),
    ] {
        pci_write(dev, COMMON_QUEUE_SELECT, 2, queue, mem);
        pci_write(dev, COMMON_QUEUE_SIZE, 2, 8, mem);
        pci_write_split_u64(dev, COMMON_QUEUE_DESC, desc, mem);
        pci_write_split_u64(dev, COMMON_QUEUE_DRIVER, avail, mem);
        pci_write_split_u64(dev, COMMON_QUEUE_DEVICE, used, mem);
        pci_write(dev, COMMON_QUEUE_MSIX_VECTOR, 2, queue, mem);
        assert_eq!(pci_read(dev, COMMON_QUEUE_MSIX_VECTOR, 2, mem), queue);
        pci_write(dev, COMMON_QUEUE_ENABLE, 2, 1, mem);
        let len = if queue == 0 {
            128
        } else {
            (VIRTIO_NET_HDR_LEN + FRAME.len()) as u32
        };
        write_desc(mem, desc, 0, data, len, flags, 0);
        mem.write(avail + 2, &1u16.to_le_bytes());
        mem.write(avail + 4, &0u16.to_le_bytes());
        program_msix_vector(dev, queue as u16, 0xfee0_0000, 0x50 + queue as u32);
    }
    mem.write(RX_DATA, &[0xa5; 128]);
    mem.write(0x4000_8000 + VIRTIO_NET_HDR_LEN as u64, FRAME);
}

pub(super) fn fixture() -> (VirtioPciNet, TestMem) {
    let mut dev = VirtioPciNet::new_loopback();
    let mut mem = TestMem::new(0x4000_0000, 0x10000);
    initialize(&mut dev, &mut mem);
    (dev, mem)
}

pub(super) fn ready(dev: &mut VirtioPciNet, mem: &mut TestMem) {
    pci_write(dev, COMMON_DEVICE_STATUS, 1, 0x0f, mem);
}

pub(super) fn tx_notify(dev: &mut VirtioPciNet, mem: &mut TestMem) {
    pci_write(dev, PCI_NOTIFY_CFG_OFFSET + 4, 2, 1, mem);
}

pub(super) fn used_index(mem: &TestMem, base: u64) -> u16 {
    u16::from_le_bytes(mem.read(base + 2, 2).try_into().unwrap())
}

pub(super) fn assert_quiet(dev: &mut VirtioPciNet, mem: &TestMem) {
    assert_eq!(used_index(mem, RX_USED), 0);
    assert_eq!(used_index(mem, TX_USED), 0);
    assert_eq!((dev.stats().rx_count, dev.stats().tx_count), (0, 0));
    assert!(dev
        .stats()
        .queues
        .iter()
        .all(|queue| queue.last_avail_idx == 0 && !queue.pending_msix));
    assert!(!dev.interrupt_line_level());
    assert!(dev.drain_pending_msix(true, false).is_empty());
}
