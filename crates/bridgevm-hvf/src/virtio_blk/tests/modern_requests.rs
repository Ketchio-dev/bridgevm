//! Modern MMIO request fixtures.

use super::*;

#[test]
pub(super) fn modern_read_request_copies_media_to_guest_and_posts_used_element() {
    let path = temp_path("modern-read");
    let mut media = vec![0u8; 1024];
    media[512..520].copy_from_slice(b"WINSETUP");
    fs::write(&path, media).unwrap();
    let mut dev = VirtioMmioBlock::open_read_only_modern(&path).unwrap();
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

    dev.access(REG_QUEUE_NUM, true, 4, 8, &mut mem);
    dev.access(REG_QUEUE_DESC_LOW, true, 4, desc, &mut mem);
    dev.access(REG_QUEUE_DRIVER_LOW, true, 4, avail, &mut mem);
    dev.access(REG_QUEUE_DEVICE_LOW, true, 4, used, &mut mem);
    dev.access(REG_QUEUE_READY, true, 4, 1, &mut mem);
    dev.access(REG_STATUS, true, 4, 0x0f, &mut mem);
    dev.access(REG_QUEUE_NOTIFY, true, 4, 0, &mut mem);

    assert_eq!(&mem.read(data, 8), b"WINSETUP");
    assert_eq!(mem.read(status, 1), [VIRTIO_BLK_S_OK]);
    assert_eq!(
        u16::from_le_bytes(mem.read(used + 2, 2).try_into().unwrap()),
        1
    );
    assert_eq!(
        dev.access(REG_INTERRUPT_STATUS, false, 4, 0, &mut mem),
        VirtioMmioBlockResult::ReadValue(1)
    );

    fs::remove_file(path).ok();
}

#[test]
pub(super) fn modern_read_request_reuses_descriptor_and_read_scratch_across_notifies() {
    let path = temp_path("modern-read-scratch");
    let mut media = vec![0u8; 1536];
    media[512..520].copy_from_slice(b"WINSETUP");
    media[1024..1032].copy_from_slice(b"NEXTBOOT");
    fs::write(&path, media).unwrap();
    let mut dev = VirtioMmioBlock::open_read_only_modern(&path).unwrap();
    let mut mem = TestMem::new(0x4000_0000, 0x10000);

    let desc = 0x4000_1000;
    let avail = 0x4000_2000;
    let used = 0x4000_3000;
    let header = 0x4000_4000;
    let data = 0x4000_5000;
    let status = 0x4000_6000;

    dev.access(REG_QUEUE_NUM, true, 4, 8, &mut mem);
    dev.access(REG_QUEUE_DESC_LOW, true, 4, desc, &mut mem);
    dev.access(REG_QUEUE_DRIVER_LOW, true, 4, avail, &mut mem);
    dev.access(REG_QUEUE_DEVICE_LOW, true, 4, used, &mut mem);
    dev.access(REG_QUEUE_READY, true, 4, 1, &mut mem);
    dev.access(REG_STATUS, true, 4, 0x0f, &mut mem);

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

    dev.access(REG_QUEUE_NOTIFY, true, 4, 0, &mut mem);

    assert_eq!(&mem.read(data, 8), b"WINSETUP");
    let desc_cap = dev.descriptor_scratch.capacity();
    let desc_ptr = dev.descriptor_scratch.as_ptr();
    let read_cap = dev.read_scratch.capacity();
    let read_ptr = dev.read_scratch.as_ptr();
    assert!(desc_cap >= 3);
    assert!(read_cap >= 512);

    mem.write(data, &[0; 8]);
    mem.write(status, &[0]);
    mem.write(header, &VIRTIO_BLK_T_IN.to_le_bytes());
    mem.write(header + 8, &2u64.to_le_bytes());
    write_desc(
        &mut mem,
        desc,
        3,
        Descriptor {
            addr: header,
            len: 16,
            flags: DESC_F_NEXT,
            next: 4,
        },
    );
    write_desc(
        &mut mem,
        desc,
        4,
        Descriptor {
            addr: data,
            len: 512,
            flags: DESC_F_NEXT | DESC_F_WRITE,
            next: 5,
        },
    );
    write_desc(
        &mut mem,
        desc,
        5,
        Descriptor {
            addr: status,
            len: 1,
            flags: DESC_F_WRITE,
            next: 0,
        },
    );
    mem.write(avail + 2, &2u16.to_le_bytes());
    mem.write(avail + 6, &3u16.to_le_bytes());

    dev.access(REG_QUEUE_NOTIFY, true, 4, 0, &mut mem);

    assert_eq!(&mem.read(data, 8), b"NEXTBOOT");
    assert_eq!(mem.read(status, 1), [VIRTIO_BLK_S_OK]);
    assert_eq!(dev.descriptor_scratch.capacity(), desc_cap);
    assert_eq!(dev.descriptor_scratch.as_ptr(), desc_ptr);
    assert_eq!(dev.read_scratch.capacity(), read_cap);
    assert_eq!(dev.read_scratch.as_ptr(), read_ptr);

    fs::remove_file(path).ok();
}
