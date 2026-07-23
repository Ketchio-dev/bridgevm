//! Split test module.

use super::super::*;
use crate::fwcfg::GuestMemoryMut;
use std::io;
use std::path::Path;
use std::path::PathBuf;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

/// A flat span of guest RAM for exercising the queue/DMA path in tests,
/// mirroring `fwcfg.rs`'s `FakeMem`.
pub(super) struct FakeMem {
    pub(super) base: u64,
    pub(super) bytes: Vec<u8>,
    /// When set, [`GuestMemoryMut::host_ptr`] resolves spans to real pointers
    /// into `bytes`, so the NVMe data path takes its zero-copy direct-DMA
    /// branch instead of the reusable-scratch fallback. Off by default so the
    /// existing suite keeps covering the buffered path.
    pub(super) expose_host_ptr: bool,
}

impl FakeMem {
    pub(super) fn new(base: u64, len: usize) -> Self {
        Self {
            base,
            bytes: vec![0u8; len],
            expose_host_ptr: false,
        }
    }
    pub(super) fn at(&self, gpa: u64) -> usize {
        (gpa - self.base) as usize
    }
    /// Expose stable host pointers so IO takes the direct-DMA branch.
    pub(super) fn enable_host_ptr(&mut self) {
        self.expose_host_ptr = true;
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
    fn read_into(&self, gpa: u64, dst: &mut [u8]) -> bool {
        let start = self.at(gpa);
        let Some(end) = start.checked_add(dst.len()) else {
            return false;
        };
        if end > self.bytes.len() {
            return false;
        }
        dst.copy_from_slice(&self.bytes[start..end]);
        true
    }
    fn host_ptr(&self, gpa: u64, len: usize) -> Option<*mut u8> {
        if !self.expose_host_ptr {
            return None;
        }
        let start = self.at(gpa);
        let end = start.checked_add(len)?;
        if end > self.bytes.len() {
            return None;
        }
        Some(self.bytes.as_ptr().wrapping_add(start) as *mut u8)
    }
}

/// Build a 64-byte submission-queue entry from its decoded fields.
pub(super) fn encode_sqe(
    opcode: u8,
    command_id: u16,
    nsid: u32,
    prp1: u64,
    cdw10: u32,
    cdw11: u32,
    cdw12: u32,
) -> [u8; 64] {
    encode_sqe_with_prps(opcode, command_id, nsid, prp1, 0, [cdw10, cdw11, cdw12])
}

pub(super) fn encode_sqe_with_prps(
    opcode: u8,
    command_id: u16,
    nsid: u32,
    prp1: u64,
    prp2: u64,
    command_dwords: [u32; 3],
) -> [u8; 64] {
    let [cdw10, cdw11, cdw12] = command_dwords;
    let mut e = [0u8; 64];
    let cdw0 = u32::from(opcode) | (u32::from(command_id) << 16);
    e[0..4].copy_from_slice(&cdw0.to_le_bytes());
    e[4..8].copy_from_slice(&nsid.to_le_bytes());
    e[24..32].copy_from_slice(&prp1.to_le_bytes());
    e[32..40].copy_from_slice(&prp2.to_le_bytes());
    e[40..44].copy_from_slice(&cdw10.to_le_bytes());
    e[44..48].copy_from_slice(&cdw11.to_le_bytes());
    e[48..52].copy_from_slice(&cdw12.to_le_bytes());
    e
}

// Guest-memory layout used by the admin/IO tests.
pub(super) const MEM_BASE: u64 = 0x4000_0000;
pub(super) const ASQ_BASE: u64 = 0x4000_1000; // admin submission queue
pub(super) const ACQ_BASE: u64 = 0x4000_2000; // admin completion queue
pub(super) const IO_SQ_BASE: u64 = 0x4000_3000;
pub(super) const IO_CQ_BASE: u64 = 0x4000_4000;
pub(super) const DATA_BASE: u64 = 0x4000_5000; // PRP data buffer
pub(super) const QDEPTH: u16 = 8;

pub(super) fn temp_path(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "bridgevm-hvf-nvme-{name}-{}-{nanos}",
        std::process::id()
    ))
}

pub(super) fn raw_file_sync_attempts(backend: &DiskBackend) -> usize {
    match backend {
        DiskBackend::RawFile(disk) => disk.sync_attempts,
        DiskBackend::Memory(_) => panic!("expected raw-file disk backend"),
    }
}

pub(super) fn set_raw_file_sync_failure(backend: &mut DiskBackend, failure: Option<io::ErrorKind>) {
    match backend {
        DiskBackend::RawFile(disk) => disk.sync_failure = failure,
        DiskBackend::Memory(_) => panic!("expected raw-file disk backend"),
    }
}

/// Enable a fresh controller with admin queues installed.
pub(super) fn enabled_controller_with_disk_and_mem_len(
    disk: Vec<u8>,
    mem_len: usize,
) -> (NvmeController, FakeMem) {
    let mut ctrl = NvmeController::with_disk_image(disk);
    let mem = FakeMem::new(MEM_BASE, mem_len);
    // Program AQA (0-based sizes), ASQ, ACQ, then set CC.EN.
    let aqa = (u32::from(QDEPTH - 1) << 16) | u32::from(QDEPTH - 1);
    ctrl.mmio_write(REG_AQA, 4, u64::from(aqa));
    ctrl.mmio_write(REG_ASQ, 8, ASQ_BASE);
    ctrl.mmio_write(REG_ACQ, 8, ACQ_BASE);
    ctrl.mmio_write(REG_CC, 4, u64::from(CC_EN_BIT));
    (ctrl, mem)
}

pub(super) fn enabled_controller_with_mem_len(mem_len: usize) -> (NvmeController, FakeMem) {
    enabled_controller_with_disk_and_mem_len(vec![0u8; 1 << 20], mem_len)
}

pub(super) fn enabled_controller() -> (NvmeController, FakeMem) {
    enabled_controller_with_mem_len(0x8000)
}

pub(super) fn enabled_controller_with_raw_file(
    path: &Path,
    write_back: bool,
    mem_len: usize,
) -> (NvmeController, FakeMem) {
    let mut ctrl = NvmeController::with_raw_file(path, write_back).unwrap();
    let mem = FakeMem::new(MEM_BASE, mem_len);
    let aqa = (u32::from(QDEPTH - 1) << 16) | u32::from(QDEPTH - 1);
    ctrl.mmio_write(REG_AQA, 4, u64::from(aqa));
    ctrl.mmio_write(REG_ASQ, 8, ASQ_BASE);
    ctrl.mmio_write(REG_ACQ, 8, ACQ_BASE);
    ctrl.mmio_write(REG_CC, 4, u64::from(CC_EN_BIT));
    (ctrl, mem)
}

/// Submit one admin command at SQ slot `slot` and ring the doorbell.
pub(super) fn submit_admin(
    ctrl: &mut NvmeController,
    mem: &mut FakeMem,
    slot: u16,
    sqe: &[u8; 64],
) {
    let gpa = ASQ_BASE + u64::from(slot) * SQ_ENTRY_SIZE;
    assert!(mem.write_bytes(gpa, sqe));
    // Ring SQ0 tail doorbell (offset 0x1000) with new tail = slot + 1.
    ctrl.mmio_write(REG_DOORBELL_BASE, 4, u64::from(slot + 1));
    ctrl.process(mem);
}

pub(super) fn submit_io(
    ctrl: &mut NvmeController,
    mem: &mut FakeMem,
    slot: u16,
    sqe: &[u8; 64],
) -> u16 {
    let gpa = IO_SQ_BASE + u64::from(slot) * SQ_ENTRY_SIZE;
    assert!(mem.write_bytes(gpa, sqe));
    ctrl.mmio_write(REG_DOORBELL_BASE + 2 * 4, 4, u64::from(slot + 1));
    ctrl.process(mem);
    completion_status(&read_completion(mem, IO_CQ_BASE, slot))
}

pub(super) fn read_completion(mem: &FakeMem, cq_base: u64, slot: u16) -> [u8; 16] {
    let gpa = cq_base + u64::from(slot) * CQ_ENTRY_SIZE;
    let raw = mem.read_bytes(gpa, 16).unwrap();
    let mut e = [0u8; 16];
    e.copy_from_slice(&raw);
    e
}

pub(super) fn completion_status(entry: &[u8; 16]) -> u16 {
    u16::from_le_bytes([entry[14], entry[15]]) >> 1
}

pub(super) fn completion_dw0(entry: &[u8; 16]) -> u32 {
    u32::from_le_bytes([entry[0], entry[1], entry[2], entry[3]])
}

pub(super) fn create_io_queue_pair(
    ctrl: &mut NvmeController,
    mem: &mut FakeMem,
    first_admin_slot: u16,
    cq_cdw11: u32,
) {
    let cdw10 = (u32::from(QDEPTH - 1) << 16) | 1;
    let cq_cmd = encode_sqe(ADMIN_OP_CREATE_IO_CQ, 1, 0, IO_CQ_BASE, cdw10, cq_cdw11, 0);
    submit_admin(ctrl, mem, first_admin_slot, &cq_cmd);
    assert_eq!(
        completion_status(&read_completion(mem, ACQ_BASE, first_admin_slot)),
        SC_SUCCESS
    );

    let sq_cmd = encode_sqe(
        ADMIN_OP_CREATE_IO_SQ,
        2,
        0,
        IO_SQ_BASE,
        cdw10,
        1u32 << 16,
        0,
    );
    submit_admin(ctrl, mem, first_admin_slot + 1, &sq_cmd);
    assert_eq!(
        completion_status(&read_completion(mem, ACQ_BASE, first_admin_slot + 1)),
        SC_SUCCESS
    );
}

// ---- Stage 3 DMA path: coalescing + direct DMA + persistence ----------

#[test]
pub(super) fn coalesce_spans_merges_contiguous_and_splits_on_gaps() {
    // Three abutting pages collapse into one segment.
    assert_eq!(
        coalesce_spans(&[(0x1000, 0x1000), (0x2000, 0x1000), (0x3000, 0x1000)]),
        vec![(0x1000, 0x3000)]
    );
    // A hole between spans keeps them separate.
    assert_eq!(
        coalesce_spans(&[(0x1000, 0x1000), (0x3000, 0x1000)]),
        vec![(0x1000, 0x1000), (0x3000, 0x1000)]
    );
    // A partial first span (PRP1 mid-page offset) followed by whole pages.
    assert_eq!(
        coalesce_spans(&[(0x1e00, 0x200), (0x2000, 0x1000), (0x3000, 0x1000)]),
        vec![(0x1e00, 0x2200)]
    );
    // Degenerate inputs.
    assert_eq!(coalesce_spans(&[(0x1000, 0x200)]), vec![(0x1000, 0x200)]);
    assert_eq!(coalesce_spans(&[]), Vec::<(u64, usize)>::new());
    // Zero-length spans are dropped, not treated as a break.
    assert_eq!(
        coalesce_spans(&[(0x1000, 0), (0x1000, 0x1000)]),
        vec![(0x1000, 0x1000)]
    );
}

/// Read `pages` disk pages scattered across non-adjacent guest pages (so the
/// segments do NOT coalesce), returning the bytes gathered from guest RAM in
/// transfer order. `expose_host_ptr` selects the direct-DMA vs buffered path.
pub(super) fn scatter_read_gathered(pages: usize, expose_host_ptr: bool) -> (Vec<u8>, Vec<u8>) {
    let disk: Vec<u8> = (0..PAGE_SIZE * pages)
        .map(|i| (i.wrapping_mul(31).wrapping_add(7)) as u8)
        .collect();
    let (mut ctrl, mut mem) = enabled_controller_with_disk_and_mem_len(disk.clone(), 0x40000);
    if expose_host_ptr {
        mem.enable_host_ptr();
    }
    create_io_queue_pair(&mut ctrl, &mut mem, 0, CREATE_IO_CQ_PC_BIT);

    // Data pages every other page => neighbours never abut => 1 segment/page.
    let data_gpas: Vec<u64> = (0..pages as u64)
        .map(|i| DATA_BASE + 0x2000 + i * 2 * PAGE_SIZE_U64)
        .collect();
    let list_base = DATA_BASE;
    let mut list = vec![0u8; (pages - 1) * 8];
    for (k, data_gpa) in data_gpas.iter().enumerate().skip(1) {
        let off = (k - 1) * 8;
        list[off..off + 8].copy_from_slice(&data_gpa.to_le_bytes());
    }
    assert!(mem.write_bytes(list_base, &list));

    let blocks = pages as u32 * (PAGE_SIZE as u32 / LBA_SIZE as u32);
    let read_cmd = encode_sqe_with_prps(
        NVM_OP_READ,
        0x60,
        NSID,
        data_gpas[0],
        list_base,
        [0, 0, blocks - 1],
    );
    assert!(mem.write_bytes(IO_SQ_BASE, &read_cmd));
    ctrl.mmio_write(REG_DOORBELL_BASE + 2 * 4, 4, 1);
    ctrl.process(&mut mem);
    assert_eq!(
        completion_status(&read_completion(&mem, IO_CQ_BASE, 0)),
        SC_SUCCESS
    );

    let mut gathered = Vec::with_capacity(PAGE_SIZE * pages);
    for &g in &data_gpas {
        gathered.extend_from_slice(&mem.read_bytes(g, PAGE_SIZE).unwrap());
    }
    (gathered, disk)
}

/// Write `pages` distinct guest pages scattered across non-adjacent guest
/// pages into a fresh disk, returning (disk_image, expected_concatenation).
pub(super) fn scatter_write_result(pages: usize, expose_host_ptr: bool) -> (Vec<u8>, Vec<u8>) {
    let (mut ctrl, mut mem) =
        enabled_controller_with_disk_and_mem_len(vec![0u8; PAGE_SIZE * pages], 0x40000);
    if expose_host_ptr {
        mem.enable_host_ptr();
    }
    create_io_queue_pair(&mut ctrl, &mut mem, 0, CREATE_IO_CQ_PC_BIT);

    let data_gpas: Vec<u64> = (0..pages as u64)
        .map(|i| DATA_BASE + 0x2000 + i * 2 * PAGE_SIZE_U64)
        .collect();
    let mut expected = Vec::with_capacity(PAGE_SIZE * pages);
    for (page, &g) in data_gpas.iter().enumerate() {
        let chunk: Vec<u8> = (0..PAGE_SIZE)
            .map(|i| (0x40 | (page as u8 & 0x0f)).wrapping_add((i % 0x20) as u8))
            .collect();
        assert!(mem.write_bytes(g, &chunk));
        expected.extend_from_slice(&chunk);
    }
    let list_base = DATA_BASE;
    let mut list = vec![0u8; (pages - 1) * 8];
    for (k, data_gpa) in data_gpas.iter().enumerate().skip(1) {
        let off = (k - 1) * 8;
        list[off..off + 8].copy_from_slice(&data_gpa.to_le_bytes());
    }
    assert!(mem.write_bytes(list_base, &list));

    let blocks = pages as u32 * (PAGE_SIZE as u32 / LBA_SIZE as u32);
    let write_cmd = encode_sqe_with_prps(
        NVM_OP_WRITE,
        0x61,
        NSID,
        data_gpas[0],
        list_base,
        [0, 0, blocks - 1],
    );
    assert!(mem.write_bytes(IO_SQ_BASE, &write_cmd));
    ctrl.mmio_write(REG_DOORBELL_BASE + 2 * 4, 4, 1);
    ctrl.process(&mut mem);
    assert_eq!(
        completion_status(&read_completion(&mem, IO_CQ_BASE, 0)),
        SC_SUCCESS
    );
    (ctrl.disk_image()[..PAGE_SIZE * pages].to_vec(), expected)
}
