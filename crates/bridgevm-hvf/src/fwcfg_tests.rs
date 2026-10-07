//! `fw_cfg` selector, data and DMA register ABI tests.

use super::*;

/// A flat span of guest RAM for exercising the DMA path in tests.
struct FakeMem {
    base: u64,
    bytes: Vec<u8>,
}

impl FakeMem {
    fn new(base: u64, len: usize) -> Self {
        Self {
            base,
            bytes: vec![0u8; len],
        }
    }
    fn at(&self, gpa: u64) -> usize {
        (gpa - self.base) as usize
    }
}

impl GuestMemoryMut for FakeMem {
    fn write_bytes(&mut self, gpa: u64, data: &[u8]) -> bool {
        let start = self.at(gpa);
        let end = start + data.len();
        if end > self.bytes.len() {
            return false;
        }
        self.bytes[start..end].copy_from_slice(data);
        true
    }
    fn read_bytes(&self, gpa: u64, len: usize) -> Option<Vec<u8>> {
        let start = self.at(gpa);
        let end = start + len;
        if end > self.bytes.len() {
            return None;
        }
        Some(self.bytes[start..end].to_vec())
    }
}

#[test]
fn signature_reads_qemu() {
    let mut fw = FwCfg::new();
    fw.select(KEY_SIGNATURE);
    assert_eq!(fw.read_data(4), b"QEMU");
}

#[test]
fn id_advertises_traditional_and_dma() {
    let mut fw = FwCfg::new();
    fw.select(KEY_ID);
    let bytes = fw.read_data(4);
    let id = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    assert_eq!(id & ID_TRADITIONAL, ID_TRADITIONAL);
    assert_eq!(id & ID_DMA, ID_DMA);
}

#[test]
fn reading_past_end_returns_zero() {
    let mut fw = FwCfg::new();
    fw.select(KEY_SIGNATURE); // 4 bytes
    assert_eq!(fw.read_data(4), b"QEMU");
    assert_eq!(fw.read_data_byte(), 0);
}

#[test]
fn reselect_resets_cursor() {
    let mut fw = FwCfg::new();
    fw.select(KEY_SIGNATURE);
    assert_eq!(fw.read_data_byte(), b'Q');
    fw.select(KEY_SIGNATURE);
    assert_eq!(fw.read_data_byte(), b'Q');
}

#[test]
fn file_registration_assigns_sequential_selectors() {
    let mut fw = FwCfg::new();
    let a = fw.add_file("etc/acpi/rsdp", vec![1, 2, 3]);
    let b = fw.add_file("etc/acpi/tables", vec![4, 5]);
    assert_eq!(a, KEY_FILE_FIRST);
    assert_eq!(b, KEY_FILE_FIRST + 1);
    // The blob is reachable through its selector.
    fw.select(b);
    assert_eq!(fw.read_data(2), vec![4, 5]);
}

#[test]
fn registering_the_same_file_replaces_without_duplicate_directory_entry() {
    let mut fw = FwCfg::new();
    let first = fw.add_file("etc/acpi/tables", vec![1, 2]);
    let second = fw.add_file("etc/acpi/tables", vec![3, 4, 5]);
    assert_eq!(first, second);

    fw.select(first);
    assert_eq!(fw.read_data(3), vec![3, 4, 5]);

    fw.select(KEY_FILE_DIR);
    let dir = fw.read_data(fw.file_dir_bytes().len());
    let count = u32::from_be_bytes([dir[0], dir[1], dir[2], dir[3]]);
    assert_eq!(count, 1);
    let size = u32::from_be_bytes([dir[4], dir[5], dir[6], dir[7]]);
    assert_eq!(size, 3);
}

#[test]
fn fixed_items_are_readable_without_file_dir_entries() {
    let mut fw = FwCfg::new();
    fw.add_item(KEY_KERNEL_SIZE, 4u32.to_le_bytes().to_vec());
    fw.add_item(KEY_KERNEL_DATA, b"boot".to_vec());

    fw.select(KEY_KERNEL_SIZE);
    assert_eq!(fw.mmio_read(REG_DATA, 4), 4);
    fw.select(KEY_KERNEL_DATA);
    assert_eq!(fw.read_data(4), b"boot");

    fw.select(KEY_FILE_DIR);
    let dir = fw.read_data(fw.file_dir_bytes().len());
    let count = u32::from_be_bytes([dir[0], dir[1], dir[2], dir[3]]);
    assert_eq!(count, 0, "fixed selector items stay out of FILE_DIR");
}

#[test]
fn directory_is_sorted_by_name_with_be_fields() {
    let mut fw = FwCfg::new();
    // Insert out of lexical order; directory must come back sorted.
    let tables = fw.add_file("etc/table-loader", vec![0; 7]);
    let rsdp = fw.add_file("etc/acpi/rsdp", vec![0; 36]);

    fw.select(KEY_FILE_DIR);
    let dir = fw.read_data(fw.file_dir_bytes().len());

    let count = u32::from_be_bytes([dir[0], dir[1], dir[2], dir[3]]);
    assert_eq!(count, 2);

    // First record (offset 4) must be the lexically-smallest name.
    let rec0 = &dir[4..68];
    let size0 = u32::from_be_bytes([rec0[0], rec0[1], rec0[2], rec0[3]]);
    let select0 = u16::from_be_bytes([rec0[4], rec0[5]]);
    let name0_end = rec0[8..64].iter().position(|&b| b == 0).unwrap_or(56);
    let name0 = std::str::from_utf8(&rec0[8..8 + name0_end]).unwrap();
    assert_eq!(name0, "etc/acpi/rsdp");
    assert_eq!(size0, 36);
    assert_eq!(select0, rsdp);

    let rec1 = &dir[68..132];
    let select1 = u16::from_be_bytes([rec1[4], rec1[5]]);
    let name1_end = rec1[8..64].iter().position(|&b| b == 0).unwrap_or(56);
    let name1 = std::str::from_utf8(&rec1[8..8 + name1_end]).unwrap();
    assert_eq!(name1, "etc/table-loader");
    assert_eq!(select1, tables);
}

#[test]
fn mmio_data_read_is_little_endian_cpu_load_from_stream() {
    let mut fw = FwCfg::new();
    fw.select(KEY_SIGNATURE);
    // AArch64 firmware does `MmioRead32(DATA)` and compares against
    // SIGNATURE_32('Q','E','M','U') == 0x554d4551.
    assert_eq!(fw.mmio_read(REG_DATA, 4), 0x554d_4551);
}

#[test]
fn mmio_dma_register_reads_signature() {
    let mut fw = FwCfg::new();
    assert_eq!(fw.mmio_read(REG_DMA, 8), DMA_REG_SIGNATURE);
}

#[test]
fn mmio_selector_write_then_read() {
    let mut fw = FwCfg::new();
    let mut mem = FakeMem::new(0x4000_0000, 0);
    fw.mmio_write(REG_SELECTOR, 2, u64::from(KEY_SIGNATURE), &mut mem);
    assert_eq!(fw.mmio_read(REG_DATA, 1), u64::from(b'Q'));
}

#[test]
fn mmio_selector_register_is_big_endian() {
    // Guest firmware stores SwapBytes16(selector); selecting FILE_DIR (0x0019)
    // arrives on the bus as 0x1900. The device must swap it back, not read a
    // non-existent item.
    let mut fw = FwCfg::new();
    fw.add_file("etc/x", vec![1, 2, 3]);
    let mut mem = FakeMem::new(0x4000_0000, 0);
    fw.mmio_write(
        REG_SELECTOR,
        2,
        u64::from(KEY_FILE_DIR.swap_bytes()),
        &mut mem,
    );
    // FILE_DIR begins with a big-endian u32 file count == 1.
    let raw_count = fw.mmio_read(REG_DATA, 4);
    assert_eq!(
        u32::from_be_bytes((raw_count as u32).to_le_bytes()),
        1,
        "selector must resolve to FILE_DIR, not a bogus item"
    );
}

#[test]
fn dma_read_moves_entry_into_guest_memory() {
    let mut fw = FwCfg::new();
    let mut mem = FakeMem::new(0x4000_0000, 0x1000);
    let dst = 0x4000_0100;
    let access = FwCfgDmaAccess {
        control: (u32::from(KEY_SIGNATURE) << 16) | DMA_CTL_SELECT | DMA_CTL_READ,
        length: 4,
        address: dst,
    };
    let result = fw.dma_execute(access, &mut mem);
    assert_eq!(result, 0, "successful DMA clears all control bits");
    assert_eq!(mem.read_bytes(dst, 4).unwrap(), b"QEMU");
}

#[test]
fn dma_read_unbacked_address_reports_error() {
    let mut fw = FwCfg::new();
    let mut mem = FakeMem::new(0x4000_0000, 0x10);
    let access = FwCfgDmaAccess {
        control: (u32::from(KEY_SIGNATURE) << 16) | DMA_CTL_SELECT | DMA_CTL_READ,
        length: 4,
        address: 0x9999_0000, // outside the fake span
    };
    assert_eq!(fw.dma_execute(access, &mut mem), DMA_CTL_ERROR);
}

#[test]
fn dma_write_into_readonly_entry_is_rejected() {
    let mut fw = FwCfg::new();
    let mut mem = FakeMem::new(0x4000_0000, 0x1000);
    mem.write_bytes(0x4000_0000, &[0xaa, 0xbb])
        .then_some(())
        .unwrap();
    fw.select(KEY_SIGNATURE);
    let access = FwCfgDmaAccess {
        control: DMA_CTL_WRITE,
        length: 2,
        address: 0x4000_0000,
    };
    assert_eq!(fw.dma_execute(access, &mut mem), DMA_CTL_ERROR);
}

#[test]
fn dma_write_updates_writable_entry() {
    let mut fw = FwCfg::new();
    let sel = fw.add_writable_file("etc/system-states", vec![0, 0, 0, 0]);
    let mut mem = FakeMem::new(0x4000_0000, 0x1000);
    mem.write_bytes(0x4000_0000, &[1, 2, 3, 4]);
    let access = FwCfgDmaAccess {
        control: (u32::from(sel) << 16) | DMA_CTL_SELECT | DMA_CTL_WRITE,
        length: 4,
        address: 0x4000_0000,
    };
    assert_eq!(fw.dma_execute(access, &mut mem), 0);
    fw.select(sel);
    assert_eq!(fw.read_data(4), vec![1, 2, 3, 4]);
}

#[test]
fn file_bytes_observes_named_entry_without_moving_cursor() {
    let mut fw = FwCfg::new();
    fw.add_file("etc/ramfb", vec![0xaa; 4]);
    fw.select(KEY_SIGNATURE);

    assert_eq!(fw.read_data_byte(), b'Q');
    assert_eq!(
        fw.file_bytes("etc/ramfb"),
        Some(&[0xaa, 0xaa, 0xaa, 0xaa][..])
    );
    assert_eq!(fw.read_data_byte(), b'E');
}

#[test]
fn file_bytes_reflects_dma_write_to_writable_entry() {
    let mut fw = FwCfg::new();
    let sel = fw.add_writable_file("etc/ramfb", vec![0; 4]);
    let mut mem = FakeMem::new(0x4000_0000, 0x1000);
    mem.write_bytes(0x4000_0000, &[5, 6, 7, 8]);
    let access = FwCfgDmaAccess {
        control: (u32::from(sel) << 16) | DMA_CTL_SELECT | DMA_CTL_WRITE,
        length: 4,
        address: 0x4000_0000,
    };

    assert_eq!(fw.dma_execute(access, &mut mem), 0);

    assert_eq!(fw.file_bytes("etc/ramfb"), Some(&[5, 6, 7, 8][..]));
}

#[test]
fn dma_skip_advances_cursor() {
    let mut fw = FwCfg::new();
    let mut mem = FakeMem::new(0x4000_0000, 0x10);
    fw.select(KEY_SIGNATURE);
    let access = FwCfgDmaAccess {
        control: DMA_CTL_SKIP,
        length: 2,
        address: 0,
    };
    assert_eq!(fw.dma_execute(access, &mut mem), 0);
    // Skipped "QE", next byte is 'M'.
    assert_eq!(fw.read_data_byte(), b'M');
}

/// Records each DMA write the device issues, backing only `[base, base+len)`.
struct WriteRecorder {
    inner: FakeMem,
    writes: Vec<(u64, usize)>,
}

impl GuestMemoryMut for WriteRecorder {
    fn write_bytes(&mut self, gpa: u64, data: &[u8]) -> bool {
        self.writes.push((gpa, data.len()));
        gpa >= self.inner.base && self.inner.write_bytes(gpa, data)
    }
    fn read_bytes(&self, gpa: u64, len: usize) -> Option<Vec<u8>> {
        self.inner.read_bytes(gpa, len)
    }
}

fn signature_read(length: u32, address: u64) -> FwCfgDmaAccess {
    FwCfgDmaAccess {
        control: (u32::from(KEY_SIGNATURE) << 16) | DMA_CTL_SELECT | DMA_CTL_READ,
        length,
        address,
    }
}

#[test]
fn oversized_dma_read_to_unbacked_memory_stops_at_first_chunk() {
    let mut fw = FwCfg::new();
    let mut mem = WriteRecorder {
        inner: FakeMem::new(0x4000_0000, 0x1000),
        writes: Vec::new(),
    };
    let access = signature_read(64 << 20, 0x9999_0000);
    assert_eq!(fw.dma_execute(access, &mut mem), DMA_CTL_ERROR);
    assert_eq!(
        mem.writes,
        [(0x9999_0000, 4)],
        "no 64 MiB host staging buffer"
    );
}

#[test]
fn dma_read_past_entry_end_zero_fills_in_bounded_chunks() {
    let mut fw = FwCfg::new();
    let base = 0x4000_0000;
    let mut mem = WriteRecorder {
        inner: FakeMem::new(base, 0x4000),
        writes: Vec::new(),
    };
    mem.inner.bytes.fill(0xaa);
    assert_eq!(fw.dma_execute(signature_read(0x3000, base), &mut mem), 0);
    assert_eq!(&mem.inner.bytes[..4], b"QEMU");
    assert!(mem.inner.bytes[4..0x3000].iter().all(|byte| *byte == 0));
    assert_eq!(
        mem.inner.bytes[0x3000], 0xaa,
        "nothing past the requested length"
    );
    assert!(mem.writes.iter().all(|(_, len)| *len <= 4096));
    assert_eq!(
        fw.offset, 0x3000,
        "cursor advances by the transferred length"
    );

    // A transfer that runs off the end of guest RAM reports an error after
    // writing only the backed prefix.
    let mut fw = FwCfg::new();
    mem.writes.clear();
    mem.inner.bytes.fill(0xaa);
    assert_eq!(
        fw.dma_execute(signature_read(0x2000, base + 0x3000), &mut mem),
        DMA_CTL_ERROR
    );
    assert_eq!(&mem.inner.bytes[0x3000..0x3004], b"QEMU");
    assert!(mem.writes.iter().all(|(_, len)| *len <= 4096));
}
