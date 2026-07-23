use super::*;
use crate::fwcfg::GuestMemoryMut;
use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;

mod part_2;

#[derive(Debug)]
pub(super) struct TestMem {
    pub(super) base: u64,
    pub(super) bytes: Vec<u8>,
}

impl TestMem {
    pub(super) fn new(base: u64, len: usize) -> Self {
        Self {
            base,
            bytes: vec![0; len],
        }
    }

    pub(super) fn write(&mut self, gpa: u64, data: &[u8]) {
        assert!(self.write_bytes(gpa, data));
    }

    pub(super) fn read(&self, gpa: u64, len: usize) -> Vec<u8> {
        self.read_bytes(gpa, len).unwrap()
    }
}

impl GuestMemoryMut for TestMem {
    fn write_bytes(&mut self, gpa: u64, data: &[u8]) -> bool {
        let Some(off) = gpa.checked_sub(self.base).map(|v| v as usize) else {
            return false;
        };
        if off + data.len() > self.bytes.len() {
            return false;
        }
        self.bytes[off..off + data.len()].copy_from_slice(data);
        true
    }

    fn read_bytes(&self, gpa: u64, len: usize) -> Option<Vec<u8>> {
        let off = gpa.checked_sub(self.base)? as usize;
        (off + len <= self.bytes.len()).then(|| self.bytes[off..off + len].to_vec())
    }

    fn read_into(&self, gpa: u64, dst: &mut [u8]) -> bool {
        let Some(off) = gpa.checked_sub(self.base).map(|v| v as usize) else {
            return false;
        };
        let Some(end) = off.checked_add(dst.len()) else {
            return false;
        };
        if end > self.bytes.len() {
            return false;
        }
        dst.copy_from_slice(&self.bytes[off..end]);
        true
    }
}

pub(super) fn temp_path(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "bridgevm-hvf-virtio-blk-{name}-{}-{nanos}",
        std::process::id()
    ))
}

pub(super) fn write_desc(mem: &mut TestMem, table: u64, index: u16, desc: Descriptor) {
    let gpa = table + u64::from(index) * DESC_SIZE;
    mem.write(gpa, &desc.addr.to_le_bytes());
    mem.write(gpa + 8, &desc.len.to_le_bytes());
    mem.write(gpa + 12, &desc.flags.to_le_bytes());
    mem.write(gpa + 14, &desc.next.to_le_bytes());
}

#[test]
pub(super) fn identity_and_capacity_registers_are_exposed() {
    let path = temp_path("identity");
    fs::write(&path, vec![0u8; 1500]).unwrap();
    let mut dev = VirtioMmioBlock::open_read_only(&path).unwrap();
    let mut mem = TestMem::new(0x4000_0000, 0x1000);

    assert_eq!(
        dev.access(REG_MAGIC, false, 4, 0, &mut mem),
        VirtioMmioBlockResult::ReadValue(u64::from(MAGIC_VALUE))
    );
    assert_eq!(
        dev.access(REG_VERSION, false, 4, 0, &mut mem),
        VirtioMmioBlockResult::ReadValue(u64::from(VERSION_LEGACY))
    );
    assert_eq!(
        dev.access(REG_DEVICE_ID, false, 4, 0, &mut mem),
        VirtioMmioBlockResult::ReadValue(u64::from(DEVICE_ID_BLOCK))
    );
    assert_eq!(
        dev.access(REG_DEVICE_FEATURES, false, 4, 0, &mut mem),
        VirtioMmioBlockResult::ReadValue(u64::from(VIRTIO_BLK_F_RO | VIRTIO_BLK_F_BLK_SIZE))
    );
    assert_eq!(
        dev.access(REG_CONFIG, false, 8, 0, &mut mem),
        VirtioMmioBlockResult::ReadValue(3)
    );
    assert_eq!(
        dev.access(REG_CONFIG + 0x14, false, 4, 0, &mut mem),
        VirtioMmioBlockResult::ReadValue(SECTOR_SIZE)
    );

    fs::remove_file(path).ok();
}

#[test]
pub(super) fn raw_file_backend_zero_fills_reads_past_file_len_within_capacity() {
    let path = temp_path("raw-tail-zero");
    fs::write(&path, b"abc").unwrap();
    let mut backend = RawFileBackend::open(&path).unwrap();

    let mut partial = [0xaa; 8];
    backend.read_at_into(0, &mut partial).unwrap();
    assert_eq!(partial, [b'a', b'b', b'c', 0, 0, 0, 0, 0]);

    let mut tail = [0xaa; 4];
    backend.read_at_into(8, &mut tail).unwrap();
    assert_eq!(tail, [0, 0, 0, 0]);

    let mut past_media = [0xaa; 1];
    let err = backend
        .read_at_into(SECTOR_SIZE, &mut past_media)
        .unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::UnexpectedEof);

    fs::remove_file(path).ok();
}

#[test]
pub(super) fn oversized_read_descriptor_is_rejected_before_growing_scratch() {
    let path = temp_path("oversized-read");
    fs::write(&path, vec![0u8; SECTOR_SIZE as usize]).unwrap();
    let mut dev = VirtioMmioBlock::open_read_only(&path).unwrap();
    let mut mem = TestMem::new(0x4000_0000, 0x10000);
    let desc = 0x4000_1000;
    let header = 0x4000_4000;
    let status = 0x4000_6000;

    mem.write(header, &VIRTIO_BLK_T_IN.to_le_bytes());
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
            addr: 0x4000_5000,
            len: u32::MAX,
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
    dev.queue_num = 8;
    dev.queue_desc = desc;
    let mut descs = Vec::new();
    let mut scratch = Vec::with_capacity(32);
    let capacity = scratch.capacity();

    let completion = dev.process_descriptor_chain(&mut mem, 0, &mut descs, &mut scratch);

    assert_eq!(completion.written_len, 1);
    assert_eq!(mem.read(status, 1), [VIRTIO_BLK_S_IOERR]);
    assert!(scratch.is_empty());
    assert_eq!(scratch.capacity(), capacity);
    fs::remove_file(path).ok();
}

#[test]
pub(super) fn legacy_read_request_copies_media_to_guest_and_posts_used_element() {
    let path = temp_path("read");
    let mut media = vec![0u8; 1024];
    media[512..520].copy_from_slice(b"WINSETUP");
    fs::write(&path, media).unwrap();
    let mut dev = VirtioMmioBlock::open_read_only(&path).unwrap();
    let mut mem = TestMem::new(0x4000_0000, 0x10000);

    let desc = 0x4000_1000;
    let avail = desc + 8 * DESC_SIZE;
    let used = align_up(avail + 4 + 8 * 2, 4096);
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
    dev.access(REG_GUEST_PAGE_SIZE, true, 4, 4096, &mut mem);
    dev.access(REG_QUEUE_ALIGN, true, 4, 4096, &mut mem);
    dev.access(REG_QUEUE_PFN, true, 4, desc >> 12, &mut mem);
    dev.access(REG_QUEUE_NOTIFY, true, 4, 0, &mut mem);

    assert_eq!(&mem.read(data, 8), b"WINSETUP");
    assert_eq!(mem.read(status, 1), [VIRTIO_BLK_S_OK]);
    assert_eq!(
        u16::from_le_bytes(mem.read(used + 2, 2).try_into().unwrap()),
        1
    );
    assert_eq!(
        u32::from_le_bytes(mem.read(used + 4, 4).try_into().unwrap()),
        0
    );
    assert_eq!(
        u32::from_le_bytes(mem.read(used + 8, 4).try_into().unwrap()),
        513
    );
    assert_eq!(
        dev.access(REG_INTERRUPT_STATUS, false, 4, 0, &mut mem),
        VirtioMmioBlockResult::ReadValue(1)
    );

    fs::remove_file(path).ok();
}

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
