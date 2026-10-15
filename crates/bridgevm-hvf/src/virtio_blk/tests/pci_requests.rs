//! Request fixtures for the modeled PCI BAR MMIO-register aliases.

use super::*;

#[test]
fn pci_read_request_copies_iso_sector_to_guest() {
    let path = temp_path("pci-read");
    let mut media = vec![0u8; 1024];
    media[512..520].copy_from_slice(b"WINSETUP");
    fs::write(&path, media).unwrap();
    let mut dev = VirtioPciBlock::open_read_only(&path).unwrap();
    let mut mem = TestMem::new(0x4000_0000, 0x10000);

    let desc = 0x4000_1000;
    let avail = 0x4000_2000;
    let used = 0x4000_3000;
    let header = 0x4000_4000;
    let data = 0x4000_5000;
    let status = 0x4000_6000;

    mem.write(header, &VIRTIO_BLK_T_IN.to_le_bytes());
    mem.write(header + 8, &1u64.to_le_bytes());
    write_desc(
        &mut mem,
        desc,
        0,
        Descriptor {
            addr: header,
            len: 16,
            flags: DESC_F_NEXT,
            next: 1,
        },
    );
    write_desc(
        &mut mem,
        desc,
        1,
        Descriptor {
            addr: data,
            len: 512,
            flags: DESC_F_NEXT | DESC_F_WRITE,
            next: 2,
        },
    );
    write_desc(
        &mut mem,
        desc,
        2,
        Descriptor {
            addr: status,
            len: 1,
            flags: DESC_F_WRITE,
            next: 0,
        },
    );
    mem.write(avail + 2, &1u16.to_le_bytes());
    mem.write(avail + 4, &0u16.to_le_bytes());

    pci_write(&mut dev, REG_QUEUE_NUM, 0x1_0000, &mut mem);
    pci_write(&mut dev, REG_QUEUE_DESC_LOW, desc, &mut mem);
    pci_write(&mut dev, REG_QUEUE_DRIVER_LOW, avail, &mut mem);
    pci_write(&mut dev, REG_QUEUE_DEVICE_LOW, used, &mut mem);
    pci_write(&mut dev, REG_QUEUE_READY, 1, &mut mem);
    pci_write(&mut dev, REG_STATUS, 0x0f, &mut mem);
    dev.access(
        PCI_NOTIFY_CFG_OFFSET,
        VirtioPciBlockOp::Write { size: 4, value: 0 },
        &mut mem,
    );

    assert_eq!(&mem.read(data, 8), b"WINSETUP");
    assert_eq!(mem.read(status, 1), [VIRTIO_BLK_S_OK]);
    assert!(dev.interrupt_line_level());

    fs::remove_file(path).ok();
}

#[test]
fn pci_write_request_is_rejected_for_read_only_iso() {
    let path = temp_path("pci-write-reject");
    fs::write(&path, vec![0u8; 1024]).unwrap();
    let mut dev = VirtioPciBlock::open_read_only(&path).unwrap();
    let mut mem = TestMem::new(0x4000_0000, 0x10000);

    let desc = 0x4000_1000;
    let avail = 0x4000_2000;
    let used = 0x4000_3000;
    let header = 0x4000_4000;
    let data = 0x4000_5000;
    let status = 0x4000_6000;

    mem.write(header, &1u32.to_le_bytes());
    mem.write(header + 8, &1u64.to_le_bytes());
    mem.write(data, b"NOWRITE!");
    write_desc(
        &mut mem,
        desc,
        0,
        Descriptor {
            addr: header,
            len: 16,
            flags: DESC_F_NEXT,
            next: 1,
        },
    );
    write_desc(
        &mut mem,
        desc,
        1,
        Descriptor {
            addr: data,
            len: 512,
            flags: DESC_F_NEXT,
            next: 2,
        },
    );
    write_desc(
        &mut mem,
        desc,
        2,
        Descriptor {
            addr: status,
            len: 1,
            flags: DESC_F_WRITE,
            next: 0,
        },
    );
    mem.write(avail + 2, &1u16.to_le_bytes());
    mem.write(avail + 4, &0u16.to_le_bytes());

    pci_write(&mut dev, REG_QUEUE_NUM, 8, &mut mem);
    pci_write(&mut dev, REG_QUEUE_DESC_LOW, desc, &mut mem);
    pci_write(&mut dev, REG_QUEUE_DRIVER_LOW, avail, &mut mem);
    pci_write(&mut dev, REG_QUEUE_DEVICE_LOW, used, &mut mem);
    pci_write(&mut dev, REG_QUEUE_READY, 1, &mut mem);
    pci_write(&mut dev, REG_STATUS, 0x0f, &mut mem);
    dev.access(
        PCI_NOTIFY_CFG_OFFSET,
        VirtioPciBlockOp::Write { size: 4, value: 0 },
        &mut mem,
    );

    assert_ne!(mem.read(status, 1), [VIRTIO_BLK_S_OK]);
    assert_eq!(mem.read(status, 1), [VIRTIO_BLK_S_IOERR]);

    fs::remove_file(path).ok();
}

fn pci_write(dev: &mut VirtioPciBlock, reg: u64, value: u64, mem: &mut TestMem) {
    dev.access(
        PCI_COMMON_CFG_OFFSET + reg,
        VirtioPciBlockOp::Write { size: 4, value },
        mem,
    );
}
