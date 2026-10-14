//! Console control queues populated before the final DRIVER_OK transition.
use super::super::*;
use super::helpers::*;

pub(super) const RX_USED: u64 = 0x4000_3000;
pub(super) const TX_USED: u64 = 0x4000_7000;
pub(super) const RX_DATA: u64 = 0x4000_4000;

pub(super) fn initialize(dev: &mut VirtioPciConsole, mem: &mut TestMem) {
    pci_write(dev, COMMON_DEVICE_STATUS, 1, 0, mem);
    assert_eq!(pci_read(dev, COMMON_DEVICE_STATUS, 1, mem), 0);
    mem.bytes.fill(0);
    pci_write(dev, COMMON_DEVICE_STATUS, 1, 1, mem);
    pci_write(dev, COMMON_DEVICE_STATUS, 1, 3, mem);
    for (selector, accepted) in [(0, 2), (1, 1)] {
        pci_write(dev, COMMON_DEVICE_FEATURE_SELECT, 4, selector, mem);
        let offered = pci_read(dev, COMMON_DEVICE_FEATURE, 4, mem);
        assert_eq!(offered & accepted, accepted);
        pci_write(dev, COMMON_DRIVER_FEATURE_SELECT, 4, selector, mem);
        pci_write(dev, COMMON_DRIVER_FEATURE, 4, accepted, mem);
    }
    pci_write(dev, COMMON_DEVICE_STATUS, 1, 0x0b, mem);
    assert_eq!(pci_read(dev, COMMON_DEVICE_STATUS, 1, mem), 0x0b);
    assert_eq!(pci_read(dev, COMMON_NUM_QUEUES, 2, mem), 6);
    assert_eq!(pci_read(dev, PCI_DEVICE_CFG_OFFSET + 4, 4, mem), 2);
    for (queue, desc, avail, used) in [
        (2, 0x4000_1000, 0x4000_2000, RX_USED),
        (3, 0x4000_5000, 0x4000_6000, TX_USED),
    ] {
        pci_write(dev, COMMON_QUEUE_SELECT, 2, queue, mem);
        assert!(pci_read(dev, COMMON_QUEUE_SIZE, 2, mem) >= 8);
        pci_write(dev, COMMON_QUEUE_SIZE, 2, 8, mem);
        for (register, address) in [
            (COMMON_QUEUE_DESC, desc),
            (COMMON_QUEUE_DRIVER, avail),
            (COMMON_QUEUE_DEVICE, used),
        ] {
            pci_write(dev, register, 4, address & 0xffff_ffff, mem);
            pci_write(dev, register + 4, 4, address >> 32, mem);
        }
        assert_eq!(pci_read(dev, COMMON_QUEUE_NOTIFY_OFF, 2, mem), queue);
        pci_write(dev, COMMON_QUEUE_ENABLE, 2, 1, mem);
    }
    for index in 0..2u16 {
        let data = RX_DATA + u64::from(index) * 0x100;
        mem.write(data, &[0xa5; 64]);
        post_rx(mem, 0x4000_1000, 0x4000_2000, data, 64, index);
    }
    let ready_message = control_bytes(u32::MAX, VIRTIO_CONSOLE_DEVICE_READY, 1);
    mem.write(0x4000_8000, &ready_message);
    write_desc(mem, 0x4000_5000, 0, 0x4000_8000, 8, 0, 0);
    mem.write(0x4000_6002, &1u16.to_le_bytes());
}

pub(super) fn fixture() -> (VirtioPciConsole, TestMem) {
    let mut dev = VirtioPciConsole::new();
    let mut mem = TestMem::new(0x4000_0000, 0x10000);
    initialize(&mut dev, &mut mem);
    (dev, mem)
}

pub(super) fn ready(dev: &mut VirtioPciConsole, mem: &mut TestMem) {
    pci_write(dev, COMMON_DEVICE_STATUS, 1, 0x0f, mem);
    assert_eq!(pci_read(dev, COMMON_DEVICE_STATUS, 1, mem), 0x0f);
}

pub(super) fn notify(dev: &mut VirtioPciConsole, mem: &mut TestMem) {
    pci_write(dev, PCI_NOTIFY_CFG_OFFSET + 12, 2, 3, mem);
}

pub(super) fn used_index(mem: &TestMem, base: u64) -> u16 {
    u16::from_le_bytes(mem.read(base + 2, 2).try_into().unwrap())
}
