//! Split for the 1000-line rule.

use super::*;
use crate::pcie::VIRTIO_BLK_MSIX_PBA_OFFSET;
use crate::pcie::VIRTIO_BLK_MSIX_TABLE_OFFSET;
use std::fs;

#[test]
fn pci_read_only_block_media_reports_capacity_and_ro_features() {
    let path = temp_path("pci-identity");
    fs::write(&path, vec![0u8; 1500]).unwrap();
    let mut dev = VirtioPciBlock::open_read_only(&path).unwrap();
    let mut mem = TestMem::new(0x4000_0000, 0x1000);

    assert_eq!(
        dev.access(
            PCI_COMMON_CFG_OFFSET + REG_DEVICE_FEATURES,
            VirtioPciBlockOp::Read { size: 4 },
            &mut mem
        ),
        VirtioMmioBlockResult::ReadValue(u64::from(VIRTIO_BLK_F_RO | VIRTIO_BLK_F_BLK_SIZE))
    );
    assert_eq!(
        dev.access(
            PCI_DEVICE_CFG_OFFSET,
            VirtioPciBlockOp::Read { size: 8 },
            &mut mem
        ),
        VirtioMmioBlockResult::ReadValue(3)
    );

    fs::remove_file(path).ok();
}

#[test]
fn pci_msix_bar_table_round_trips_and_pba_is_read_only() {
    let path = temp_path("pci-msix");
    fs::write(&path, vec![0u8; 512]).unwrap();
    let mut dev = VirtioPciBlock::open_read_only(&path).unwrap();

    assert_eq!(
        dev.msix_bar_access(
            u64::from(VIRTIO_BLK_MSIX_TABLE_OFFSET) + 12,
            VirtioPciBlockOp::Read { size: 4 },
        ),
        VirtioMmioBlockResult::ReadValue(1)
    );
    assert_eq!(
        dev.msix_bar_access(
            u64::from(VIRTIO_BLK_MSIX_TABLE_OFFSET),
            VirtioPciBlockOp::Write {
                size: 8,
                value: 0x0000_0001_fee0_0000,
            },
        ),
        VirtioMmioBlockResult::WriteAck
    );
    assert_eq!(
        dev.msix_bar_access(
            u64::from(VIRTIO_BLK_MSIX_TABLE_OFFSET),
            VirtioPciBlockOp::Read { size: 8 },
        ),
        VirtioMmioBlockResult::ReadValue(0x0000_0001_fee0_0000)
    );
    assert_eq!(
        dev.msix_bar_access(
            u64::from(VIRTIO_BLK_MSIX_PBA_OFFSET),
            VirtioPciBlockOp::Write {
                size: 8,
                value: u64::MAX,
            },
        ),
        VirtioMmioBlockResult::WriteAck
    );
    assert_eq!(
        dev.msix_bar_access(
            u64::from(VIRTIO_BLK_MSIX_PBA_OFFSET),
            VirtioPciBlockOp::Read { size: 8 },
        ),
        VirtioMmioBlockResult::ReadValue(0)
    );

    fs::remove_file(path).ok();
}

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

    pci_write(&mut dev, REG_QUEUE_NUM, 8, &mut mem);
    pci_write(&mut dev, REG_QUEUE_DESC_LOW, desc, &mut mem);
    pci_write(&mut dev, REG_QUEUE_DRIVER_LOW, avail, &mut mem);
    pci_write(&mut dev, REG_QUEUE_DEVICE_LOW, used, &mut mem);
    pci_write(&mut dev, REG_QUEUE_READY, 1, &mut mem);
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
fn pci_legacy_pio_queue_pfn_notify_processes_edk2_read_request() {
    const LEGACY_DEVICE_FEATURES: u64 = 0x00;
    const LEGACY_DRIVER_FEATURES: u64 = 0x04;
    const LEGACY_QUEUE_PFN: u64 = 0x08;
    const LEGACY_QUEUE_NUM: u64 = 0x0c;
    const LEGACY_QUEUE_SEL: u64 = 0x0e;
    const LEGACY_QUEUE_NOTIFY: u64 = 0x10;
    const LEGACY_DEVICE_STATUS: u64 = 0x12;

    let path = temp_path("pci-legacy-pio-read");
    let mut media = vec![0u8; 1024];
    media[512..520].copy_from_slice(b"WINSETUP");
    fs::write(&path, media).unwrap();
    let mut dev = VirtioPciBlock::open_read_only(&path).unwrap();
    let mut mem = TestMem::new(0x4000_0000, 0x10000);

    let desc = 0x4000_1000;
    let avail = desc + u64::from(QUEUE_MAX) * DESC_SIZE;
    let used = align_up(avail + 4 + u64::from(QUEUE_MAX) * 2, 4096);
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

    // Given: EDK2 sees the transitional virtio-blk legacy PIO surface.
    assert_eq!(
        dev.legacy_io_access(
            LEGACY_DEVICE_FEATURES,
            VirtioPciBlockOp::Read { size: 4 },
            &mut mem
        ),
        VirtioMmioBlockResult::ReadValue(u64::from(VIRTIO_BLK_F_RO | VIRTIO_BLK_F_BLK_SIZE))
    );
    assert_eq!(
        dev.legacy_io_access(
            LEGACY_QUEUE_NUM,
            VirtioPciBlockOp::Read { size: 2 },
            &mut mem
        ),
        VirtioMmioBlockResult::ReadValue(u64::from(QUEUE_MAX))
    );

    // When: firmware performs the legacy queue-select, PFN, status, and notify sequence.
    dev.legacy_io_access(
        LEGACY_DEVICE_STATUS,
        VirtioPciBlockOp::Write { size: 1, value: 0 },
        &mut mem,
    );
    dev.legacy_io_access(
        LEGACY_DEVICE_STATUS,
        VirtioPciBlockOp::Write { size: 1, value: 1 },
        &mut mem,
    );
    dev.legacy_io_access(
        LEGACY_DEVICE_STATUS,
        VirtioPciBlockOp::Write { size: 1, value: 3 },
        &mut mem,
    );
    dev.legacy_io_access(
        LEGACY_QUEUE_SEL,
        VirtioPciBlockOp::Write { size: 2, value: 0 },
        &mut mem,
    );
    dev.legacy_io_access(
        LEGACY_DRIVER_FEATURES,
        VirtioPciBlockOp::Write {
            size: 4,
            value: u64::from(VIRTIO_BLK_F_RO | VIRTIO_BLK_F_BLK_SIZE),
        },
        &mut mem,
    );
    dev.legacy_io_access(
        LEGACY_QUEUE_PFN,
        VirtioPciBlockOp::Write {
            size: 4,
            value: desc >> 12,
        },
        &mut mem,
    );
    dev.legacy_io_access(
        LEGACY_DEVICE_STATUS,
        VirtioPciBlockOp::Write { size: 1, value: 7 },
        &mut mem,
    );
    dev.legacy_io_access(
        LEGACY_QUEUE_NOTIFY,
        VirtioPciBlockOp::Write { size: 2, value: 0 },
        &mut mem,
    );

    // Then: the queue is usable without a modern queue-num write and the request completes.
    let stats = dev.stats();
    assert_eq!(
        (
            stats.queue_ready,
            stats.queue_num,
            stats.queue_desc,
            stats.queue_driver,
            stats.queue_device,
            stats.request_count,
            stats.read_count,
            mem.read(data, 8),
            mem.read(status, 1),
            u16::from_le_bytes(mem.read(used + 2, 2).try_into().unwrap()),
            u32::from_le_bytes(mem.read(used + 4, 4).try_into().unwrap()),
            u32::from_le_bytes(mem.read(used + 8, 4).try_into().unwrap()),
        ),
        (
            true,
            QUEUE_MAX,
            desc,
            avail,
            used,
            1,
            1,
            b"WINSETUP".to_vec(),
            vec![VIRTIO_BLK_S_OK],
            1,
            0,
            513,
        )
    );

    fs::remove_file(path).ok();
}

#[test]
fn pci_legacy_pio_read_records_recent_request_trace() {
    const LEGACY_QUEUE_PFN: u64 = 0x08;
    const LEGACY_QUEUE_NOTIFY: u64 = 0x10;

    let path = temp_path("pci-legacy-pio-trace");
    let mut media = vec![0u8; 4096];
    media[1024..1032].copy_from_slice(b"BOOTSECT");
    fs::write(&path, media).unwrap();
    let mut dev = VirtioPciBlock::open_read_only(&path).unwrap();
    let mut mem = TestMem::new(0x4000_0000, 0x10000);

    let desc = 0x4000_1000;
    let avail = desc + u64::from(QUEUE_MAX) * DESC_SIZE;
    let header = 0x4000_4000;
    let data = 0x4000_5000;
    let status = 0x4000_6000;

    mem.write(header, &VIRTIO_BLK_T_IN.to_le_bytes());
    mem.write(header + 8, &2u64.to_le_bytes());
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
            len: 1024,
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

    dev.legacy_io_access(
        LEGACY_QUEUE_PFN,
        VirtioPciBlockOp::Write {
            size: 4,
            value: desc >> 12,
        },
        &mut mem,
    );
    dev.legacy_io_access(
        LEGACY_QUEUE_NOTIFY,
        VirtioPciBlockOp::Write { size: 2, value: 0 },
        &mut mem,
    );

    assert_eq!(
        dev.recent_request_trace().last().copied(),
        Some(VirtioBlockRequestTrace {
            sequence: 1,
            request_type: VIRTIO_BLK_T_IN,
            sector: 2,
            data_len: 1024,
            status: VIRTIO_BLK_S_OK,
        })
    );

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
    dev.access(
        PCI_NOTIFY_CFG_OFFSET,
        VirtioPciBlockOp::Write { size: 4, value: 0 },
        &mut mem,
    );

    assert_ne!(mem.read(status, 1), [VIRTIO_BLK_S_OK]);
    assert_eq!(mem.read(status, 1), [VIRTIO_BLK_S_UNSUPP]);

    fs::remove_file(path).ok();
}

fn pci_write(dev: &mut VirtioPciBlock, reg: u64, value: u64, mem: &mut TestMem) {
    dev.access(
        PCI_COMMON_CFG_OFFSET + reg,
        VirtioPciBlockOp::Write { size: 4, value },
        mem,
    );
}
