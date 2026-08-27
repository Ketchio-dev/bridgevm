use super::super::*;
use super::helpers::*;

#[test]
fn net_tx_queue_caps_malformed_avail_delta_at_queue_size() {
    let mut dev = VirtioPciNet::new_loopback();
    let mut mem = TestMem::new(0x4000_0000, 0x10000);
    let desc = 0x4000_1000;
    let avail = 0x4000_2000;
    let used = 0x4000_3000;
    let queue_size = 8u16;

    setup_queue(&mut dev, &mut mem, QUEUE_TX as u16, desc, avail, used, 1);
    mem.write(avail + 2, &queue_size.wrapping_add(1).to_le_bytes());

    pci_write(&mut dev, PCI_NOTIFY_CFG_OFFSET + 4, 4, 0, &mut mem);

    assert_eq!(dev.stats().queues[QUEUE_TX].last_avail_idx, queue_size);
    assert_eq!(
        u16::from_le_bytes(mem.read(used + 2, 2).try_into().unwrap()),
        queue_size
    );
    assert_eq!(dev.stats().notify_count, 1);
}
