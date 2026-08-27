use super::super::*;
use super::helpers::*;

#[test]
fn gpu_process_queue_caps_malformed_avail_delta_at_queue_size() {
    let mut dev = VirtioPciGpu::new(1280, 800);
    let mut mem = TestMem::new(0x4000_0000, 0x10000);
    let desc = 0x4000_1000;
    let avail = 0x4000_2000;
    let used = 0x4000_3000;
    let queue_size = 16u16;

    setup_queue(&mut dev, &mut mem, 0, desc, avail, used, 0);
    mem.write(avail + 2, &queue_size.wrapping_add(1).to_le_bytes());

    pci_write(&mut dev, PCI_NOTIFY_CFG_OFFSET, 4, 0, &mut mem);

    assert_eq!(dev.stats().queues[0].last_avail_idx, queue_size);
    assert_eq!(
        u16::from_le_bytes(mem.read(used + 2, 2).try_into().unwrap()),
        queue_size
    );
}
