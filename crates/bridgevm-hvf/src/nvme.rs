//! Minimal NVMe controller MMIO device model (BridgeVM HVF "Path A").
//!
//! Windows 11 Setup ships an *inbox* NVMe storage driver but no inbox virtio
//! driver, so on the BridgeVM HVF custom VMM the first storage target is an
//! emulated NVMe controller rather than virtio-blk. This module models the
//! controller's BAR0 register region, its admin queue, a single I/O queue and a
//! flat in-memory disk backend. Like [`crate::fwcfg`] it is host-only and
//! unit-testable: the HVF run loop will (later) map guest MMIO accesses onto
//! [`NvmeController::mmio_read`] / [`NvmeController::mmio_write`] and call
//! [`NvmeController::process`] with a [`GuestMemoryMut`] accessor so the queue
//! engine can read submission-queue entries / PRP data buffers out of guest RAM
//! and write completion-queue entries / read data back. Nothing here touches
//! Hypervisor.framework, so it builds and tests on any host.
//!
//! Scope (deliberately minimal — *not yet wired live*):
//!   * BAR0 registers: CAP, VS (1.4.0), CC, CSTS, AQA, ASQ, ACQ, doorbells.
//!   * Admin commands: IDENTIFY (controller + namespace), CREATE I/O
//!     COMPLETION/SUBMISSION QUEUE, SET FEATURES (number of queues).
//!   * One I/O queue processing NVM READ (0x02) / WRITE (0x01) against a flat
//!     raw disk backend with 512-byte LBAs, using PRP1/PRP2 and PRP lists for
//!     data transfers. Unit tests usually use an in-memory image; live probes
//!     can attach large host raw files without reading them into RAM.
//!
//! The DMA/queue path mirrors `fwcfg.rs`: all guest-memory traffic flows through
//! the shared [`GuestMemoryMut`] trait, and completions are written straight
//! back into the guest's completion queue.
//!
//! References: NVM Express Base Specification 1.4, sections 3 (controller
//! registers), 5 (admin command set) and the NVM Command Set; QEMU `hw/nvme/`.

use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::Path,
};

use crate::fwcfg::GuestMemoryMut;
use crate::msix::{MsixMessage, MsixTable};
use crate::pcie::{NVME_MSIX_PBA_OFFSET, NVME_MSIX_TABLE_OFFSET, NVME_MSIX_VECTOR_COUNT};

/// Size of one submission-queue entry, in bytes (NVMe fixed: 64).
pub const SQ_ENTRY_SIZE: u64 = 64;
/// Size of one completion-queue entry, in bytes (NVMe fixed: 16).
pub const CQ_ENTRY_SIZE: u64 = 16;
/// Logical block (LBA) size this model exposes to the guest.
pub const LBA_SIZE: usize = 512;
/// Guest-visible memory page size assumed for PRP transfers (single page).
pub const PAGE_SIZE: usize = 4096;
const PAGE_SIZE_U64: u64 = PAGE_SIZE as u64;
const FILE_OVERLAY_CHUNK_SIZE: u64 = PAGE_SIZE_U64;
const EXPORT_CHUNK_SIZE: usize = 1024 * 1024;
/// Maximum number of submission/completion queue entries we advertise
/// (`CAP.MQES` is 0-based, so the wire value is `MAX_QUEUE_ENTRIES - 1`).
pub const MAX_QUEUE_ENTRIES: u16 = 64;
/// I/O queue-pair capacity advertised to SET FEATURES (NUMBER OF QUEUES). The
/// model only drives one, but exposes a small pool so a multi-queue guest gets
/// a sane non-zero allocation back.
pub const MAX_IO_QUEUE_PAIRS: u16 = 8;

// ---- BAR0 register offsets (NVMe 1.4 §3.1) --------------------------------
/// Controller Capabilities (64-bit, RO).
pub const REG_CAP: u64 = 0x00;
/// Version (32-bit, RO).
pub const REG_VS: u64 = 0x08;
/// Interrupt Mask Set (32-bit, RW) — accepted but unused (we do not raise IRQs).
pub const REG_INTMS: u64 = 0x0C;
/// Interrupt Mask Clear (32-bit, RW) — accepted but unused.
pub const REG_INTMC: u64 = 0x10;
/// Controller Configuration (32-bit, RW).
pub const REG_CC: u64 = 0x14;
/// Controller Status (32-bit, RO to the guest).
pub const REG_CSTS: u64 = 0x1C;
/// Admin Queue Attributes (32-bit, RW).
pub const REG_AQA: u64 = 0x24;
/// Admin Submission Queue Base Address (64-bit, RW).
pub const REG_ASQ: u64 = 0x28;
/// Admin Completion Queue Base Address (64-bit, RW).
pub const REG_ACQ: u64 = 0x30;
/// First doorbell register (`SQ0TDBL`). With `CAP.DSTRD = 0` the stride is 4
/// bytes, so doorbell `n` lives at `DOORBELL_BASE + n * 4`.
pub const REG_DOORBELL_BASE: u64 = 0x1000;
/// First offset after the modelled doorbell aperture. One admin queue pair plus
/// `MAX_IO_QUEUE_PAIRS` I/O queue pairs, two doorbells per queue.
pub const REG_DOORBELL_END: u64 = REG_DOORBELL_BASE + (MAX_IO_QUEUE_PAIRS as u64 + 1) * 2 * 4;

// ---- CAP fields (NVMe 1.4 §3.1.1) -----------------------------------------
/// `CAP.MQES` lives in bits 15:0 and is 0-based.
const CAP_MQES_SHIFT: u64 = 0;
/// `CAP.CQR` (contiguous queues required), bit 16. We require contiguous queues.
const CAP_CQR_BIT: u64 = 1 << 16;
/// `CAP.TO` (timeout, 500 ms units), bits 31:24. Advertise a generous 1 s.
const CAP_TO_SHIFT: u64 = 24;
/// `CAP.DSTRD` (doorbell stride) bits 35:32 — 0 ⇒ 4-byte stride.
const CAP_DSTRD_SHIFT: u64 = 32;
/// `CAP.CSS` (command sets supported), bits 44:37. Bit 37 ⇒ NVM command set.
const CAP_CSS_NVM_BIT: u64 = 1 << 37;

// ---- VS (NVMe 1.4 §3.1.2) -------------------------------------------------
/// Version 1.4.0 encoded as `(MJR << 16) | (MNR << 8) | TER`.
pub const NVME_VERSION_1_4_0: u32 = 0x0001_0400;

// ---- CC fields (NVMe 1.4 §3.1.5) ------------------------------------------
const CC_EN_BIT: u32 = 1 << 0;

// ---- CSTS fields (NVMe 1.4 §3.1.6) ----------------------------------------
const CSTS_RDY_BIT: u32 = 1 << 0;

// ---- Admin opcodes (NVMe 1.4 §5, Figure 139) ------------------------------
const ADMIN_OP_DELETE_IO_SQ: u8 = 0x00;
const ADMIN_OP_CREATE_IO_SQ: u8 = 0x01;
const ADMIN_OP_GET_LOG_PAGE: u8 = 0x02;
const ADMIN_OP_DELETE_IO_CQ: u8 = 0x04;
const ADMIN_OP_CREATE_IO_CQ: u8 = 0x05;
const ADMIN_OP_IDENTIFY: u8 = 0x06;
const ADMIN_OP_SET_FEATURES: u8 = 0x09;

// ---- NVM (I/O) opcodes (NVMe NVM Command Set) -----------------------------
const NVM_OP_WRITE: u8 = 0x01;
const NVM_OP_READ: u8 = 0x02;

// ---- IDENTIFY CNS values (NVMe 1.4 §5.15.1) -------------------------------
const IDENTIFY_CNS_NAMESPACE: u32 = 0x00;
const IDENTIFY_CNS_CONTROLLER: u32 = 0x01;
const IDENTIFY_CNS_ACTIVE_NAMESPACE_LIST: u32 = 0x02;
const IDENTIFY_CNS_NAMESPACE_DESCRIPTOR_LIST: u32 = 0x03;

// ---- SET FEATURES feature IDs (NVMe 1.4 §5.21.1) --------------------------
const FEATURE_NUMBER_OF_QUEUES: u8 = 0x07;

// ---- CREATE I/O COMPLETION QUEUE fields (NVMe 1.4 §5.3) -------------------
const CREATE_IO_CQ_PC_BIT: u32 = 1 << 0;
const CREATE_IO_CQ_IEN_BIT: u32 = 1 << 1;
const CREATE_IO_CQ_IV_SHIFT: u32 = 16;

// ---- GET LOG PAGE log identifiers (NVMe 1.4 §5.14.1) ----------------------
const LOG_PAGE_SMART_HEALTH: u8 = 0x02;

// ---- Completion status codes (NVMe 1.4 §4.6.1, generic command status) ----
/// Successful completion (status code type 0, code 0).
const SC_SUCCESS: u16 = 0x0000;
/// Invalid Field in Command.
const SC_INVALID_FIELD: u16 = 0x0002;
/// Invalid Opcode.
const SC_INVALID_OPCODE: u16 = 0x0001;

/// The single namespace's identifier (NSID 1).
pub const NSID: u32 = 1;

const NS_EUI64: [u8; 8] = *b"BVMNVME1";
const NS_NGUID: [u8; 16] = *b"BridgeVM-NVMeNS1";
const NS_UUID: [u8; 16] = [
    0x42, 0x56, 0x4d, 0x00, 0x20, 0x26, 0x06, 0x20, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01,
];

/// A decoded 64-byte NVMe submission-queue entry. Only the fields this minimal
/// model consumes are surfaced; everything is read from guest RAM little-endian.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SubmissionEntry {
    /// Command Dword 0: opcode in bits 7:0, command identifier in bits 31:16.
    pub opcode: u8,
    pub command_id: u16,
    /// Namespace Identifier (CDW1).
    pub nsid: u32,
    /// First PRP entry / data pointer (bytes 24..32).
    pub prp1: u64,
    /// Second PRP entry (bytes 32..40) — unused for single-page transfers.
    pub prp2: u64,
    /// Command Dwords 10..16 (command-specific).
    pub cdw10: u32,
    pub cdw11: u32,
    pub cdw12: u32,
    pub cdw13: u32,
    pub cdw14: u32,
    pub cdw15: u32,
}

impl SubmissionEntry {
    /// Decode a 64-byte submission-queue entry from guest RAM (little-endian).
    pub fn from_bytes(b: &[u8; 64]) -> Self {
        let dw = |i: usize| u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
        let qw = |i: usize| {
            u64::from_le_bytes([
                b[i],
                b[i + 1],
                b[i + 2],
                b[i + 3],
                b[i + 4],
                b[i + 5],
                b[i + 6],
                b[i + 7],
            ])
        };
        let cdw0 = dw(0);
        Self {
            opcode: (cdw0 & 0xff) as u8,
            command_id: (cdw0 >> 16) as u16,
            nsid: dw(4),
            prp1: qw(24),
            prp2: qw(32),
            cdw10: dw(40),
            cdw11: dw(44),
            cdw12: dw(48),
            cdw13: dw(52),
            cdw14: dw(56),
            cdw15: dw(60),
        }
    }
}

/// State for one submission queue created by the guest.
#[derive(Debug, Clone)]
struct SubmissionQueue {
    /// Guest-physical base address of the (contiguous) queue.
    base: u64,
    /// Number of entries (queue depth).
    size: u16,
    /// Consumer-side head; the controller advances it as it fetches entries.
    head: u16,
    /// Producer-side tail last reported by the guest through the SQ doorbell.
    tail_doorbell: u16,
    /// The completion queue this SQ posts completions to.
    cqid: u16,
}

/// State for one completion queue created by the guest.
#[derive(Debug, Clone)]
struct CompletionQueue {
    /// Guest-physical base address of the (contiguous) queue.
    base: u64,
    /// Number of entries (queue depth).
    size: u16,
    /// Producer-side tail; the controller advances it as it posts completions.
    tail: u16,
    /// Phase tag; toggles every time the tail wraps (NVMe 1.4 §4.6).
    phase: bool,
    /// Last head the guest reported through the CQ doorbell.
    head: u16,
    /// MSI-X vector to signal when this completion queue receives an entry.
    interrupt_vector: u16,
    /// Whether completions on this CQ should generate an interrupt.
    interrupts_enabled: bool,
}

/// Completion metadata that the platform layer turns into an interrupt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NvmeCompletionEvent {
    pub cqid: u16,
    pub vector: u16,
}

#[derive(Debug)]
enum DiskBackend {
    Memory(Vec<u8>),
    RawFile(RawFileDisk),
}

#[derive(Debug)]
struct RawFileDisk {
    file: File,
    len: u64,
    overlay: BTreeMap<u64, Vec<u8>>,
    write_back: bool,
}

impl DiskBackend {
    fn memory(mut disk: Vec<u8>) -> Self {
        let len = rounded_disk_len(disk.len());
        disk.resize(len, 0);
        Self::Memory(disk)
    }

    fn raw_file(path: impl AsRef<Path>, write_back: bool) -> io::Result<Self> {
        let path = path.as_ref();
        let file = OpenOptions::new().read(true).write(write_back).open(path)?;
        let len = file.metadata()?.len();
        if len % LBA_SIZE as u64 != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "{} is {len} bytes, not a multiple of the {LBA_SIZE}-byte NVMe LBA size",
                    path.display()
                ),
            ));
        }
        Ok(Self::RawFile(RawFileDisk {
            file,
            len,
            overlay: BTreeMap::new(),
            write_back,
        }))
    }

    fn byte_len(&self) -> u64 {
        match self {
            Self::Memory(disk) => disk.len() as u64,
            Self::RawFile(disk) => disk.len,
        }
    }

    fn memory_image(&self) -> Option<&[u8]> {
        match self {
            Self::Memory(disk) => Some(disk),
            Self::RawFile(_) => None,
        }
    }

    fn read_at(&mut self, offset: u64, len: usize) -> io::Result<Vec<u8>> {
        self.validate_range(offset, len)?;
        match self {
            Self::Memory(disk) => {
                let start = offset as usize;
                Ok(disk[start..start + len].to_vec())
            }
            Self::RawFile(disk) => disk.read_at(offset, len),
        }
    }

    fn write_at(&mut self, offset: u64, data: &[u8]) -> io::Result<()> {
        self.validate_range(offset, data.len())?;
        match self {
            Self::Memory(disk) => {
                let start = offset as usize;
                disk[start..start + data.len()].copy_from_slice(data);
                Ok(())
            }
            Self::RawFile(disk) => disk.write_at(offset, data),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Memory(_) => Ok(()),
            Self::RawFile(disk) => disk.file.flush(),
        }
    }

    fn export_to_path(&mut self, path: impl AsRef<Path>) -> io::Result<u64> {
        let mut out = File::create(path)?;
        let len = self.byte_len();
        let mut offset = 0u64;
        while offset < len {
            let chunk_len = (len - offset).min(EXPORT_CHUNK_SIZE as u64) as usize;
            let chunk = self.read_at(offset, chunk_len)?;
            out.write_all(&chunk)?;
            offset += chunk_len as u64;
        }
        out.flush()?;
        Ok(len)
    }

    fn validate_range(&self, offset: u64, len: usize) -> io::Result<()> {
        let end = offset.checked_add(len as u64).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "NVMe disk range overflows")
        })?;
        if end > self.byte_len() {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                format!(
                    "NVMe disk range {offset:#x}..{end:#x} exceeds image size {:#x}",
                    self.byte_len()
                ),
            ));
        }
        Ok(())
    }
}

impl RawFileDisk {
    fn read_at(&mut self, offset: u64, len: usize) -> io::Result<Vec<u8>> {
        let mut out = vec![0u8; len];
        if len == 0 {
            return Ok(out);
        }
        let end = offset + len as u64;
        self.file.seek(SeekFrom::Start(offset))?;
        self.file.read_exact(&mut out)?;
        for (&chunk_base, chunk) in self.overlay.range(..end) {
            let chunk_end = chunk_base + chunk.len() as u64;
            if chunk_end <= offset {
                continue;
            }
            let copy_start = offset.max(chunk_base);
            let copy_end = end.min(chunk_end);
            let src = (copy_start - chunk_base) as usize;
            let dst = (copy_start - offset) as usize;
            let copy_len = (copy_end - copy_start) as usize;
            out[dst..dst + copy_len].copy_from_slice(&chunk[src..src + copy_len]);
        }
        Ok(out)
    }

    fn write_at(&mut self, offset: u64, data: &[u8]) -> io::Result<()> {
        if data.is_empty() {
            return Ok(());
        }
        if self.write_back {
            self.file.seek(SeekFrom::Start(offset))?;
            self.file.write_all(data)?;
            return Ok(());
        }

        let mut copied = 0usize;
        while copied < data.len() {
            let abs = offset + copied as u64;
            let chunk_base = (abs / FILE_OVERLAY_CHUNK_SIZE) * FILE_OVERLAY_CHUNK_SIZE;
            let chunk_len = self.chunk_len(chunk_base)?;
            let chunk_off = (abs - chunk_base) as usize;
            let copy_len = (data.len() - copied).min(chunk_len - chunk_off);

            if !self.overlay.contains_key(&chunk_base) {
                let mut chunk = vec![0u8; chunk_len];
                self.file.seek(SeekFrom::Start(chunk_base))?;
                self.file.read_exact(&mut chunk)?;
                self.overlay.insert(chunk_base, chunk);
            }
            let chunk = self.overlay.get_mut(&chunk_base).unwrap();
            chunk[chunk_off..chunk_off + copy_len]
                .copy_from_slice(&data[copied..copied + copy_len]);
            copied += copy_len;
        }
        Ok(())
    }

    fn chunk_len(&self, chunk_base: u64) -> io::Result<usize> {
        if chunk_base >= self.len {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "overlay chunk starts past the NVMe image",
            ));
        }
        Ok((self.len - chunk_base).min(FILE_OVERLAY_CHUNK_SIZE) as usize)
    }
}

/// A modelled minimal NVMe controller.
#[derive(Debug)]
pub struct NvmeController {
    // --- BAR0 register backing store ---
    cc: u32,
    csts: u32,
    aqa: u32,
    asq: u64,
    acq: u64,
    intms: u32,

    // --- Queues. Index 0 is the admin queue; 1.. are I/O queues. ---
    sqs: Vec<Option<SubmissionQueue>>,
    cqs: Vec<Option<CompletionQueue>>,

    // --- Backend ---
    /// Raw disk backing store, `LBA_SIZE`-byte logical blocks.
    disk: DiskBackend,
    /// Negotiated maximum number of I/O queue pairs (SET FEATURES 0x07).
    max_io_queues: u16,
    /// Command-specific result for the *next* completion's DW0 (e.g. the queue
    /// count granted by SET FEATURES). Consumed when the completion is posted.
    last_feature_result: u32,
    /// BAR-backed MSI-X table and PBA for this endpoint.
    msix: MsixTable,
}

impl NvmeController {
    /// Create a controller with a `disk_bytes`-sized backing store. The size is
    /// rounded up to a whole number of `LBA_SIZE` blocks.
    pub fn new(disk_bytes: usize) -> Self {
        Self::with_disk_backend(DiskBackend::memory(vec![0u8; rounded_disk_len(disk_bytes)]))
    }

    /// Create a controller from an existing raw disk image. The image is padded
    /// with zeros up to a whole number of `LBA_SIZE` blocks so namespace capacity
    /// and transfer bounds stay block-aligned.
    pub fn with_disk_image(disk: Vec<u8>) -> Self {
        Self::with_disk_backend(DiskBackend::memory(disk))
    }

    /// Create a controller backed by a host raw disk file. When `write_back` is
    /// false, guest writes are kept in a sparse in-memory overlay; when true,
    /// writes are applied directly to the host file.
    pub fn with_raw_file(path: impl AsRef<Path>, write_back: bool) -> io::Result<Self> {
        Ok(Self::with_disk_backend(DiskBackend::raw_file(
            path, write_back,
        )?))
    }

    fn with_disk_backend(disk: DiskBackend) -> Self {
        Self {
            cc: 0,
            csts: 0,
            aqa: 0,
            asq: 0,
            acq: 0,
            intms: 0,
            // Slot 0 reserved for the admin queue; grow lazily for I/O queues.
            sqs: vec![None],
            cqs: vec![None],
            disk,
            // Capacity for SET FEATURES (NUMBER OF QUEUES) to negotiate against.
            // The model only ever drives one I/O queue, but advertises a small
            // pool so a guest requesting several is granted a sane non-zero count.
            max_io_queues: MAX_IO_QUEUE_PAIRS,
            last_feature_result: 0,
            msix: MsixTable::new(NVME_MSIX_VECTOR_COUNT),
        }
    }

    /// Replace the backing disk image, padding to a full LBA. This resets queue
    /// and controller register state, mirroring a cold-plugged different device.
    pub fn load_disk_image(&mut self, disk: Vec<u8>) {
        *self = Self::with_disk_image(disk);
    }

    /// Replace the backing disk with a host raw file and reset controller state.
    pub fn load_raw_file(&mut self, path: impl AsRef<Path>, write_back: bool) -> io::Result<()> {
        *self = Self::with_raw_file(path, write_back)?;
        Ok(())
    }

    /// Snapshot of the current disk image, including guest writes that have been
    /// processed through the NVMe queues. This is only available for the small
    /// in-memory backend used by unit tests and ad-hoc media.
    pub fn disk_image(&self) -> &[u8] {
        self.disk
            .memory_image()
            .expect("NVMe disk image is host-file backed; export it instead")
    }

    /// Snapshot view for callers that need to distinguish memory-backed media
    /// from large host-file-backed media.
    pub fn disk_image_if_memory(&self) -> Option<&[u8]> {
        self.disk.memory_image()
    }

    /// Export the full current disk image to `path`, applying any sparse overlay
    /// writes on top of the source raw file.
    pub fn export_disk_image(&mut self, path: impl AsRef<Path>) -> io::Result<u64> {
        self.disk.export_to_path(path)
    }

    /// Flush host-file-backed write-through media.
    pub fn flush_disk(&mut self) -> io::Result<()> {
        self.disk.flush()
    }

    /// Current byte length of the backing disk.
    pub fn disk_len(&self) -> u64 {
        self.disk.byte_len()
    }

    /// Number of `LBA_SIZE`-byte logical blocks in the backing disk.
    pub fn block_count(&self) -> u64 {
        self.disk.byte_len() / LBA_SIZE as u64
    }

    /// The assembled 64-bit `CAP` register value.
    fn cap(&self) -> u64 {
        let mqes = u64::from(MAX_QUEUE_ENTRIES - 1) << CAP_MQES_SHIFT;
        let to = 2u64 << CAP_TO_SHIFT; // 2 * 500 ms = 1 s
                                       // DSTRD = 0 (4-byte stride). CSS bit 37 ⇒ NVM command set supported.
        mqes | CAP_CQR_BIT | to | (0u64 << CAP_DSTRD_SHIFT) | CAP_CSS_NVM_BIT
    }

    // ---- MMIO register interface ------------------------------------------
    //
    // NVMe controller registers are little-endian (unlike fw_cfg's big-endian
    // selector/DMA registers). The HVF run loop hands us the guest's native
    // little-endian access; we return the low `size` bytes of the register.

    /// Handle a guest MMIO read of `size` bytes (1/2/4/8) at `offset` in BAR0.
    pub fn mmio_read(&self, offset: u64, size: u8) -> u64 {
        if let Some(table_offset) = self.msix_table_offset(offset) {
            return self.msix.table_read(table_offset, size);
        }
        if let Some(pba_offset) = self.msix_pba_offset(offset) {
            return self.msix.pba_read(pba_offset, size);
        }
        // 64-bit registers can be read as two 32-bit halves; resolve the
        // containing register and slice out the requested window.
        let (reg_base, full): (u64, u64) = match offset {
            REG_CAP | 0x04 => (REG_CAP, self.cap()),
            REG_VS => (REG_VS, u64::from(NVME_VERSION_1_4_0)),
            REG_INTMS => (REG_INTMS, u64::from(self.intms)),
            REG_INTMC => (REG_INTMC, u64::from(self.intms)),
            REG_CC => (REG_CC, u64::from(self.cc)),
            REG_CSTS => (REG_CSTS, u64::from(self.csts)),
            REG_AQA => (REG_AQA, u64::from(self.aqa)),
            REG_ASQ | 0x2C => (REG_ASQ, self.asq),
            REG_ACQ | 0x34 => (REG_ACQ, self.acq),
            _ => (offset, 0),
        };
        let byte_shift = (offset - reg_base) * 8;
        let value = full >> byte_shift;
        mask_to_size(value, size)
    }

    /// Handle a guest MMIO write of `size` bytes at `offset` in BAR0. Doorbell
    /// writes only record the new tail/head; the queue engine runs in
    /// [`NvmeController::process`], called by the run loop after the MMIO write.
    pub fn mmio_write(&mut self, offset: u64, size: u8, value: u64) {
        if let Some(table_offset) = self.msix_table_offset(offset) {
            self.msix.table_write(table_offset, size, value);
            return;
        }
        if let Some(pba_offset) = self.msix_pba_offset(offset) {
            self.msix.pba_write(pba_offset, size, value);
            return;
        }
        let v32 = value as u32;
        match offset {
            REG_INTMS => self.intms |= v32,
            REG_INTMC => self.intms &= !v32,
            REG_CC => self.write_cc(v32),
            REG_AQA => self.aqa = v32,
            REG_ASQ => self.asq = merge_u64(self.asq, value, size, false),
            0x2C => self.asq = merge_u64(self.asq, value, size, true),
            REG_ACQ => self.acq = merge_u64(self.acq, value, size, false),
            0x34 => self.acq = merge_u64(self.acq, value, size, true),
            o if is_modelled_doorbell(o) => self.write_doorbell(o, v32),
            _ => {}
        }
    }

    /// Apply a write to `CC`. Toggling `CC.EN` 0→1 readies the controller
    /// (installs the admin queues and sets `CSTS.RDY`); 1→0 resets it.
    fn write_cc(&mut self, value: u32) {
        let was_enabled = self.cc & CC_EN_BIT != 0;
        let now_enabled = value & CC_EN_BIT != 0;
        self.cc = value;

        if now_enabled && !was_enabled {
            // Enable: materialise the admin SQ/CQ from AQA/ASQ/ACQ and signal
            // ready. AQA.ASQS / AQA.ACQS are 0-based queue sizes.
            let asqs = (self.aqa & 0x0fff) as u16 + 1;
            let acqs = ((self.aqa >> 16) & 0x0fff) as u16 + 1;
            self.sqs[0] = Some(SubmissionQueue {
                base: self.asq,
                size: asqs,
                head: 0,
                tail_doorbell: 0,
                cqid: 0,
            });
            self.cqs[0] = Some(CompletionQueue {
                base: self.acq,
                size: acqs,
                tail: 0,
                phase: true,
                head: 0,
                interrupt_vector: 0,
                interrupts_enabled: true,
            });
            self.csts |= CSTS_RDY_BIT;
        } else if !now_enabled && was_enabled {
            // Reset: drop all queues and clear ready.
            self.sqs = vec![None];
            self.cqs = vec![None];
            self.csts &= !CSTS_RDY_BIT;
        }
    }

    /// Record a doorbell write. The doorbell layout (DSTRD = 0, 4-byte stride)
    /// is `SQ0TDBL, CQ0HDBL, SQ1TDBL, CQ1HDBL, …` — i.e. for doorbell index
    /// `n`, even `n` is `SQ(n/2)` tail and odd `n` is `CQ(n/2)` head.
    fn write_doorbell(&mut self, offset: u64, value: u32) {
        let idx = ((offset - REG_DOORBELL_BASE) / 4) as usize;
        let qid = idx / 2;
        let is_cq = idx % 2 == 1;
        let val = value as u16;
        if is_cq {
            if let Some(Some(cq)) = self.cqs.get_mut(qid) {
                cq.head = val;
            }
        } else if let Some(Some(sq)) = self.sqs.get_mut(qid) {
            sq.tail_doorbell = val;
        }
    }

    // ---- Queue processing -------------------------------------------------

    /// Drain every submission queue whose doorbell has advanced past the
    /// controller's head, executing commands and posting completions back into
    /// guest RAM. The run loop calls this after each doorbell write (and may
    /// call it speculatively — it is a no-op when nothing is pending).
    pub fn process(&mut self, mem: &mut dyn GuestMemoryMut) -> Vec<NvmeCompletionEvent> {
        // Admin queue (index 0) first, then I/O queues.
        let mut completions = Vec::new();
        let sq_count = self.sqs.len();
        for qid in 0..sq_count {
            self.process_sq(qid, mem, &mut completions);
        }
        completions
    }

    /// Drain submission queue `qid` until its head catches the guest's tail.
    fn process_sq(
        &mut self,
        qid: usize,
        mem: &mut dyn GuestMemoryMut,
        completions: &mut Vec<NvmeCompletionEvent>,
    ) {
        loop {
            let (base, size, head, tail, cqid) = match self.sqs.get(qid) {
                Some(Some(sq)) => (sq.base, sq.size, sq.head, sq.tail_doorbell, sq.cqid),
                _ => return,
            };
            if head == tail {
                return; // queue empty
            }
            let entry_gpa = base + u64::from(head) * SQ_ENTRY_SIZE;
            let Some(raw) = mem.read_bytes(entry_gpa, SQ_ENTRY_SIZE as usize) else {
                return; // unbacked SQ memory; stop draining
            };
            let mut buf = [0u8; 64];
            buf.copy_from_slice(&raw);
            let cmd = SubmissionEntry::from_bytes(&buf);

            // Advance the SQ head (wrapping at queue size).
            let new_head = (head + 1) % size;
            if let Some(Some(sq)) = self.sqs.get_mut(qid) {
                sq.head = new_head;
            }

            let status = if qid == 0 {
                self.execute_admin(&cmd, mem)
            } else {
                self.execute_io(&cmd, mem)
            };
            if nvme_trace_enabled() {
                let kind = if qid == 0 { "admin" } else { "io" };
                println!(
                    "NVME {kind} qid={qid} cid={} op={:#04x} nsid={} cdw10={:#010x} cdw11={:#010x} cdw12={:#010x} prp1={:#x} prp2={:#x} status={:#06x}",
                    cmd.command_id,
                    cmd.opcode,
                    cmd.nsid,
                    cmd.cdw10,
                    cmd.cdw11,
                    cmd.cdw12,
                    cmd.prp1,
                    cmd.prp2,
                    status
                );
            }
            if let Some(completion) = self.post_completion(cqid, qid as u16, &cmd, status, mem) {
                if nvme_trace_enabled() {
                    println!(
                        "NVME completion cid={} sqid={} cqid={} vector={}",
                        cmd.command_id, qid, completion.cqid, completion.vector
                    );
                }
                completions.push(completion);
            }
        }
    }

    /// Execute an admin command, returning the NVMe status field to report.
    fn execute_admin(&mut self, cmd: &SubmissionEntry, mem: &mut dyn GuestMemoryMut) -> u16 {
        match cmd.opcode {
            ADMIN_OP_IDENTIFY => self.admin_identify(cmd, mem),
            ADMIN_OP_GET_LOG_PAGE => self.admin_get_log_page(cmd, mem),
            ADMIN_OP_CREATE_IO_CQ => self.admin_create_io_cq(cmd),
            ADMIN_OP_CREATE_IO_SQ => self.admin_create_io_sq(cmd),
            ADMIN_OP_SET_FEATURES => self.admin_set_features(cmd),
            ADMIN_OP_DELETE_IO_SQ | ADMIN_OP_DELETE_IO_CQ => SC_SUCCESS,
            _ => SC_INVALID_OPCODE,
        }
    }

    /// IDENTIFY (CNS in CDW10 bits 7:0). Writes a 4 KiB structure to PRP1.
    fn admin_identify(&self, cmd: &SubmissionEntry, mem: &mut dyn GuestMemoryMut) -> u16 {
        let cns = cmd.cdw10 & 0xff;
        let data = match cns {
            IDENTIFY_CNS_CONTROLLER => self.identify_controller(),
            IDENTIFY_CNS_ACTIVE_NAMESPACE_LIST => self.identify_active_namespace_list(cmd.nsid),
            IDENTIFY_CNS_NAMESPACE_DESCRIPTOR_LIST => {
                if cmd.nsid == NSID {
                    self.identify_namespace_descriptor_list()
                } else {
                    return SC_INVALID_FIELD;
                }
            }
            IDENTIFY_CNS_NAMESPACE => {
                if cmd.nsid == NSID {
                    self.identify_namespace()
                } else {
                    // Unallocated namespace ⇒ a zeroed structure (NVMe 1.4).
                    vec![0u8; PAGE_SIZE]
                }
            }
            _ => return SC_INVALID_FIELD,
        };
        if nvme_trace_enabled() {
            let label = identify_cns_name(cns);
            let preview_len = data.len().min(32);
            println!(
                "NVME identify {label} cns={cns:#x} nsid={} len={} first={} block_count={}",
                cmd.nsid,
                data.len(),
                hex_preview(&data[..preview_len]),
                self.block_count()
            );
        }
        if mem.write_bytes(cmd.prp1, &data) {
            SC_SUCCESS
        } else {
            SC_INVALID_FIELD
        }
    }

    /// Build a 4 KiB Identify Controller structure (NVMe 1.4 §5.15.2.2).
    fn identify_controller(&self) -> Vec<u8> {
        let mut d = vec![0u8; PAGE_SIZE];
        // VID (0..2) / SSVID (2..4): a recognisable but inert vendor id.
        d[0..2].copy_from_slice(&0x1b36u16.to_le_bytes()); // Red Hat / QEMU
        d[2..4].copy_from_slice(&0x1b36u16.to_le_bytes());
        // SN (4..24), MN (24..64), FR (64..72): ASCII, space-padded.
        write_ascii(&mut d[4..24], "BRIDGEVM0000000001");
        write_ascii(&mut d[24..64], "BridgeVM NVMe");
        write_ascii(&mut d[64..72], "1.0");
        // RAB (72) recommended arbitration burst.
        d[72] = 0;
        // VER (80..84): identify data agrees with VS.
        d[80..84].copy_from_slice(&NVME_VERSION_1_4_0.to_le_bytes());
        // SQES (512): min/max submission-queue entry size = 2^6 = 64 bytes.
        d[512] = 0x66;
        // CQES (513): min/max completion-queue entry size = 2^4 = 16 bytes.
        d[513] = 0x44;
        // NN (516..520): number of namespaces = 1.
        d[516..520].copy_from_slice(&1u32.to_le_bytes());
        // SUBNQN (768..1024): NUL-terminated subsystem NQN. Linux warns for
        // NVMe >= 1.2.1 if this field is empty or consumes the whole NQN field.
        write_cstr(
            &mut d[768..1024],
            "nqn.2026-06.dev.bridgevm:bridgevm-hvf:nvme0",
        );
        d
    }

    /// Build a 4 KiB Identify Namespace structure (NVMe 1.4 §5.15.2.1).
    fn identify_namespace(&self) -> Vec<u8> {
        let mut d = vec![0u8; PAGE_SIZE];
        let nsze = self.block_count();
        // NSZE (0..8), NCAP (8..16), NUSE (16..24): all in logical blocks.
        d[0..8].copy_from_slice(&nsze.to_le_bytes());
        d[8..16].copy_from_slice(&nsze.to_le_bytes());
        d[16..24].copy_from_slice(&nsze.to_le_bytes());
        // NLBAF (25): number of LBA formats minus one ⇒ 0 ⇒ one format.
        d[25] = 0;
        // FLBAS (26): formatted LBA size ⇒ format index 0.
        d[26] = 0;
        // NGUID (104..120) / EUI64 (120..128): stable non-zero namespace IDs.
        d[104..120].copy_from_slice(&NS_NGUID);
        d[120..128].copy_from_slice(&NS_EUI64);
        // LBAF0 (128..132): MS=0, LBADS = log2(512) = 9 (bits 23:16), RP=0.
        let lbads: u32 = 9 << 16;
        d[128..132].copy_from_slice(&lbads.to_le_bytes());
        d
    }

    /// Build a 4 KiB Identify Active Namespace ID List (CNS=0x02). The list
    /// contains active namespace IDs greater than the command NSID, in ascending
    /// order, terminated by zero. This model exposes exactly NSID 1.
    fn identify_active_namespace_list(&self, after_nsid: u32) -> Vec<u8> {
        let mut d = vec![0u8; PAGE_SIZE];
        if after_nsid < NSID {
            d[0..4].copy_from_slice(&NSID.to_le_bytes());
        }
        d
    }

    /// Build a 4 KiB Identify Namespace Identification Descriptor List
    /// (CNS=0x03). UUID, NGUID and EUI64 descriptors mirror the stable namespace
    /// identifiers in Identify Namespace, followed by a zero descriptor header to
    /// terminate the list.
    fn identify_namespace_descriptor_list(&self) -> Vec<u8> {
        let mut d = vec![0u8; PAGE_SIZE];
        let mut off = 0usize;
        append_namespace_id_descriptor(&mut d, &mut off, 0x03, &NS_UUID);
        append_namespace_id_descriptor(&mut d, &mut off, 0x02, &NS_NGUID);
        append_namespace_id_descriptor(&mut d, &mut off, 0x01, &NS_EUI64);
        d
    }

    /// GET LOG PAGE. Linux reads SMART / health information during probe; a
    /// zeroed, normal-temperature log is enough for this minimal volatile disk.
    fn admin_get_log_page(&self, cmd: &SubmissionEntry, mem: &mut dyn GuestMemoryMut) -> u16 {
        let lid = (cmd.cdw10 & 0xff) as u8;
        let numdl = (cmd.cdw10 >> 16) & 0xffff;
        let numdu = cmd.cdw11 & 0xffff;
        let dword_count = ((numdu << 16) | numdl).saturating_add(1);
        let byte_count = (dword_count as usize).saturating_mul(4).min(PAGE_SIZE);

        let data = match lid {
            LOG_PAGE_SMART_HEALTH => self.smart_health_log(byte_count),
            _ => return SC_INVALID_FIELD,
        };
        if mem.write_bytes(cmd.prp1, &data) {
            SC_SUCCESS
        } else {
            SC_INVALID_FIELD
        }
    }

    fn smart_health_log(&self, byte_count: usize) -> Vec<u8> {
        let mut d = vec![0u8; byte_count];
        if d.len() >= 4 {
            // Composite temperature in Kelvin, little-endian at bytes 1..3.
            // 300K is boring and healthy.
            d[1..3].copy_from_slice(&300u16.to_le_bytes());
            d[3] = 100; // available spare (%)
        }
        if d.len() >= 5 {
            d[4] = 10; // available spare threshold (%)
        }
        d
    }

    /// CREATE I/O COMPLETION QUEUE (NVMe 1.4 §5.3). CDW10: QID bits 15:0,
    /// QSIZE bits 31:16 (0-based). CDW11: PC bit 0, IEN bit 1, interrupt
    /// vector bits 31:16. PRP1 is the queue base.
    fn admin_create_io_cq(&mut self, cmd: &SubmissionEntry) -> u16 {
        let qid = (cmd.cdw10 & 0xffff) as usize;
        let qsize = ((cmd.cdw10 >> 16) & 0xffff) as u16 + 1;
        let interrupt_vector = ((cmd.cdw11 >> CREATE_IO_CQ_IV_SHIFT) & 0xffff) as u16;
        let interrupts_enabled = cmd.cdw11 & CREATE_IO_CQ_IEN_BIT != 0;
        if qid == 0 {
            return SC_INVALID_FIELD; // QID 0 is the admin queue
        }
        if cmd.cdw11 & CREATE_IO_CQ_PC_BIT == 0 {
            return SC_INVALID_FIELD; // CAP.CQR requires physically contiguous queues.
        }
        if interrupts_enabled && interrupt_vector >= NVME_MSIX_VECTOR_COUNT {
            return SC_INVALID_FIELD;
        }
        ensure_slot(&mut self.cqs, qid);
        self.cqs[qid] = Some(CompletionQueue {
            base: cmd.prp1,
            size: qsize,
            tail: 0,
            phase: true,
            head: 0,
            interrupt_vector,
            interrupts_enabled,
        });
        SC_SUCCESS
    }

    /// CREATE I/O SUBMISSION QUEUE (NVMe 1.4 §5.4). CDW10: QID / QSIZE as for
    /// the CQ; CDW11 bits 31:16 carry the associated CQID. PRP1 is the base.
    fn admin_create_io_sq(&mut self, cmd: &SubmissionEntry) -> u16 {
        let qid = (cmd.cdw10 & 0xffff) as usize;
        let qsize = ((cmd.cdw10 >> 16) & 0xffff) as u16 + 1;
        let cqid = ((cmd.cdw11 >> 16) & 0xffff) as u16;
        if qid == 0 {
            return SC_INVALID_FIELD;
        }
        // The completion queue this SQ targets must already exist.
        if self.cqs.get(cqid as usize).map(Option::is_some) != Some(true) {
            return SC_INVALID_FIELD;
        }
        ensure_slot(&mut self.sqs, qid);
        self.sqs[qid] = Some(SubmissionQueue {
            base: cmd.prp1,
            size: qsize,
            head: 0,
            tail_doorbell: 0,
            cqid,
        });
        SC_SUCCESS
    }

    /// SET FEATURES (NVMe 1.4 §5.21). Only NUMBER OF QUEUES (0x07) is honoured;
    /// any other feature is accepted as a no-op so setup does not stall.
    fn admin_set_features(&mut self, cmd: &SubmissionEntry) -> u16 {
        let fid = (cmd.cdw10 & 0xff) as u8;
        if fid == FEATURE_NUMBER_OF_QUEUES {
            // CDW11: NSQR bits 15:0, NCQR bits 31:16 (both 0-based requests).
            let nsqr = (cmd.cdw11 & 0xffff) as u16;
            let ncqr = ((cmd.cdw11 >> 16) & 0xffff) as u16;
            // Grant the smaller of each request and our capacity (all 0-based).
            let capacity = self.max_io_queues.saturating_sub(1);
            let sq_granted = nsqr.min(capacity);
            let cq_granted = ncqr.min(capacity);
            // The completion DW0 carries the allocated counts (0-based: NSQA in
            // bits 15:0, NCQA in bits 31:16); the generic completion path emits
            // it via `last_feature_result`.
            self.last_feature_result = (u32::from(cq_granted) << 16) | u32::from(sq_granted);
        }
        SC_SUCCESS
    }

    /// Execute an NVM I/O command (READ / WRITE) against the disk backend.
    fn execute_io(&mut self, cmd: &SubmissionEntry, mem: &mut dyn GuestMemoryMut) -> u16 {
        match cmd.opcode {
            NVM_OP_READ => self.io_read(cmd, mem),
            NVM_OP_WRITE => self.io_write(cmd, mem),
            _ => SC_INVALID_OPCODE,
        }
    }

    /// NVM READ (0x02). SLBA in CDW10/11 (64-bit), NLB in CDW12 bits 15:0
    /// (0-based). Data is scattered through PRP1/PRP2 or a PRP list.
    fn io_read(&mut self, cmd: &SubmissionEntry, mem: &mut dyn GuestMemoryMut) -> u16 {
        let Some((start, len)) = self.transfer_range(cmd) else {
            return SC_INVALID_FIELD;
        };
        let Some(spans) = prp_spans(cmd, len, mem) else {
            return SC_INVALID_FIELD;
        };
        let mut disk_off = start;
        for (gpa, span_len) in spans {
            let Ok(data) = self.disk.read_at(disk_off, span_len) else {
                return SC_INVALID_FIELD;
            };
            if !mem.write_bytes(gpa, &data) {
                return SC_INVALID_FIELD;
            }
            disk_off += span_len as u64;
        }
        SC_SUCCESS
    }

    /// NVM WRITE (0x01). Same addressing as READ; copies guest data into disk.
    fn io_write(&mut self, cmd: &SubmissionEntry, mem: &mut dyn GuestMemoryMut) -> u16 {
        let Some((start, len)) = self.transfer_range(cmd) else {
            return SC_INVALID_FIELD;
        };
        let Some(spans) = prp_spans(cmd, len, mem) else {
            return SC_INVALID_FIELD;
        };
        let mut disk_off = start;
        for (gpa, span_len) in spans {
            let Some(data) = mem.read_bytes(gpa, span_len) else {
                return SC_INVALID_FIELD;
            };
            if self.disk.write_at(disk_off, &data).is_err() {
                return SC_INVALID_FIELD;
            }
            disk_off += span_len as u64;
        }
        SC_SUCCESS
    }

    /// Decode (SLBA, NLB) into a byte range into `self.disk`, validating it fits
    /// the disk. Returns `(start_byte, len_bytes)`.
    fn transfer_range(&self, cmd: &SubmissionEntry) -> Option<(u64, usize)> {
        let slba = u64::from(cmd.cdw10) | (u64::from(cmd.cdw11) << 32);
        let nlb = u64::from(cmd.cdw12 & 0xffff) + 1; // 0-based count
        let len = nlb.checked_mul(LBA_SIZE as u64)?;
        let start = slba.checked_mul(LBA_SIZE as u64)?;
        if start.checked_add(len)? > self.disk.byte_len() {
            return None; // out of range
        }
        if len > usize::MAX as u64 {
            return None;
        }
        Some((start, len as usize))
    }

    /// Post a 16-byte completion-queue entry for `cmd` into completion queue
    /// `cqid`, advancing its tail and toggling the phase tag on wrap.
    fn post_completion(
        &mut self,
        cqid: u16,
        sqid: u16,
        cmd: &SubmissionEntry,
        status: u16,
        mem: &mut dyn GuestMemoryMut,
    ) -> Option<NvmeCompletionEvent> {
        let dw0 = std::mem::take(&mut self.last_feature_result);
        let (base, tail, size, phase, sq_head, interrupt_vector, interrupts_enabled) =
            match self.cqs.get(cqid as usize) {
                Some(Some(cq)) => {
                    let sq_head = match self.sqs.get(sqid as usize) {
                        Some(Some(sq)) => sq.head,
                        _ => 0,
                    };
                    (
                        cq.base,
                        cq.tail,
                        cq.size,
                        cq.phase,
                        sq_head,
                        cq.interrupt_vector,
                        cq.interrupts_enabled,
                    )
                }
                _ => return None,
            };

        // Status field (CDW3 bits 31:17 = status code, bit 16 = phase tag).
        let phase_bit = u16::from(phase);
        let status_field = (status << 1) | phase_bit;

        let mut entry = [0u8; 16];
        entry[0..4].copy_from_slice(&dw0.to_le_bytes()); // command-specific DW0
                                                         // DW1 reserved (4..8).
        entry[8..10].copy_from_slice(&sq_head.to_le_bytes()); // SQ head pointer
        entry[10..12].copy_from_slice(&sqid.to_le_bytes()); // SQ identifier
        entry[12..14].copy_from_slice(&cmd.command_id.to_le_bytes());
        entry[14..16].copy_from_slice(&status_field.to_le_bytes());

        let entry_gpa = base + u64::from(tail) * CQ_ENTRY_SIZE;
        if !mem.write_bytes(entry_gpa, &entry) {
            return None;
        }

        // Advance the CQ tail, toggling the phase tag when it wraps.
        let new_tail = (tail + 1) % size;
        if let Some(Some(cq)) = self.cqs.get_mut(cqid as usize) {
            cq.tail = new_tail;
            if new_tail == 0 {
                cq.phase = !cq.phase;
            }
        }
        interrupts_enabled.then_some(NvmeCompletionEvent {
            cqid,
            vector: interrupt_vector,
        })
    }

    pub fn raise_msix(
        &mut self,
        vector: u16,
        function_enabled: bool,
        function_masked: bool,
    ) -> Option<MsixMessage> {
        self.msix.raise(vector, function_enabled, function_masked)
    }

    pub fn drain_pending_msix(
        &mut self,
        function_enabled: bool,
        function_masked: bool,
    ) -> Vec<MsixMessage> {
        self.msix.drain_pending(function_enabled, function_masked)
    }

    fn msix_table_offset(&self, offset: u64) -> Option<u64> {
        let base = u64::from(NVME_MSIX_TABLE_OFFSET);
        let rel = offset.checked_sub(base)?;
        (rel < self.msix.table_byte_len()).then_some(rel)
    }

    fn msix_pba_offset(&self, offset: u64) -> Option<u64> {
        let base = u64::from(NVME_MSIX_PBA_OFFSET);
        let rel = offset.checked_sub(base)?;
        (rel < self.msix.pba_byte_len()).then_some(rel)
    }
}

/// Decode a command's PRP data pointers into guest-physical spans covering
/// `len` bytes. PRP1 may start at an offset within the first memory page; PRP2
/// is either the second data page or, for larger transfers, a pointer into a PRP
/// list containing little-endian entries. The command's PRP2 list pointer may
/// itself include an offset into the first list page; chained list-page pointers
/// and data-page entries must be page-aligned.
fn prp_spans(
    cmd: &SubmissionEntry,
    len: usize,
    mem: &dyn GuestMemoryMut,
) -> Option<Vec<(u64, usize)>> {
    if len == 0 {
        return Some(Vec::new());
    }
    if cmd.prp1 == 0 {
        return None;
    }

    let mut spans = Vec::new();
    let mut remaining = len;
    let first_page_left = (PAGE_SIZE_U64 - (cmd.prp1 % PAGE_SIZE_U64)) as usize;
    let first_len = remaining.min(first_page_left);
    spans.push((cmd.prp1, first_len));
    remaining -= first_len;

    if remaining == 0 {
        return Some(spans);
    }
    if cmd.prp2 == 0 {
        return None;
    }

    if remaining <= PAGE_SIZE {
        if cmd.prp2 % PAGE_SIZE_U64 != 0 {
            return None;
        }
        spans.push((cmd.prp2, remaining));
        return Some(spans);
    }

    let mut list_gpa = cmd.prp2;
    let mut list_pages_seen = 0usize;

    while remaining > 0 {
        let list_offset = (list_gpa % PAGE_SIZE_U64) as usize;
        if list_offset % 8 != 0 {
            return None;
        }
        list_pages_seen += 1;
        if list_pages_seen > 16 {
            return None;
        }

        let raw = mem.read_bytes(list_gpa, PAGE_SIZE - list_offset)?;
        let mut followed_chain = false;
        let entries_in_page = raw.len() / 8;
        for (idx, chunk) in raw.chunks_exact(8).enumerate() {
            let entry = u64::from_le_bytes(chunk.try_into().unwrap());
            if entry == 0 {
                return None;
            }

            if idx == entries_in_page - 1 && remaining > PAGE_SIZE {
                if entry % PAGE_SIZE_U64 != 0 {
                    return None;
                }
                list_gpa = entry;
                followed_chain = true;
                break;
            }

            if entry % PAGE_SIZE_U64 != 0 {
                return None;
            }
            let span_len = remaining.min(PAGE_SIZE);
            spans.push((entry, span_len));
            remaining -= span_len;
            if remaining == 0 {
                return Some(spans);
            }
        }

        if !followed_chain {
            return None;
        }
    }

    Some(spans)
}

// ---- small helpers --------------------------------------------------------

/// Mask `value` to a 1/2/4/8-byte access width.
fn mask_to_size(value: u64, size: u8) -> u64 {
    match size {
        1 => value & 0xff,
        2 => value & 0xffff,
        4 => value & 0xffff_ffff,
        _ => value,
    }
}

fn is_modelled_doorbell(offset: u64) -> bool {
    (REG_DOORBELL_BASE..REG_DOORBELL_END).contains(&offset) && offset % 4 == 0
}

fn nvme_trace_enabled() -> bool {
    matches!(
        std::env::var("BRIDGEVM_TRACE_NVME").ok().as_deref(),
        Some("1") | Some("true") | Some("TRUE") | Some("yes") | Some("YES")
    )
}

fn identify_cns_name(cns: u32) -> &'static str {
    match cns {
        IDENTIFY_CNS_NAMESPACE => "namespace",
        IDENTIFY_CNS_CONTROLLER => "controller",
        IDENTIFY_CNS_ACTIVE_NAMESPACE_LIST => "active-ns-list",
        IDENTIFY_CNS_NAMESPACE_DESCRIPTOR_LIST => "ns-desc-list",
        _ => "unknown",
    }
}

fn hex_preview(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use std::fmt::Write;
        let _ = write!(&mut out, "{b:02x}");
    }
    out
}

fn rounded_disk_len(bytes: usize) -> usize {
    bytes.div_ceil(LBA_SIZE) * LBA_SIZE
}

/// Merge a partial write into a 64-bit register. `high` selects the upper
/// 32-bit half (for split 32-bit accesses to a 64-bit register).
fn merge_u64(current: u64, value: u64, size: u8, high: bool) -> u64 {
    if size >= 8 {
        return value;
    }
    if high {
        (current & 0x0000_0000_ffff_ffff) | (u64::from(value as u32) << 32)
    } else {
        (current & 0xffff_ffff_0000_0000) | u64::from(value as u32)
    }
}

/// Grow `slots` so index `idx` is addressable, filling new slots with `None`.
fn ensure_slot<T>(slots: &mut Vec<Option<T>>, idx: usize) {
    if idx >= slots.len() {
        slots.resize_with(idx + 1, || None);
    }
}

/// Copy `s` into `dst` as ASCII, space-padding the remainder (NVMe string
/// fields are space- not NUL-padded).
fn write_ascii(dst: &mut [u8], s: &str) {
    for b in dst.iter_mut() {
        *b = b' ';
    }
    let bytes = s.as_bytes();
    let n = bytes.len().min(dst.len());
    dst[..n].copy_from_slice(&bytes[..n]);
}

/// Copy `s` into `dst` as a C string, clearing the full destination first.
fn write_cstr(dst: &mut [u8], s: &str) {
    dst.fill(0);
    if dst.is_empty() {
        return;
    }
    let bytes = s.as_bytes();
    let n = bytes.len().min(dst.len() - 1);
    dst[..n].copy_from_slice(&bytes[..n]);
}

fn append_namespace_id_descriptor(dst: &mut [u8], off: &mut usize, nidt: u8, id: &[u8]) {
    let end = *off + 4 + id.len();
    assert!(end <= dst.len(), "namespace ID descriptor list overflow");
    dst[*off] = nidt;
    dst[*off + 1] = id.len() as u8;
    // bytes 2..4 are reserved and remain zero.
    dst[*off + 4..end].copy_from_slice(id);
    *off = end;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    /// A flat span of guest RAM for exercising the queue/DMA path in tests,
    /// mirroring `fwcfg.rs`'s `FakeMem`.
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

    /// Build a 64-byte submission-queue entry from its decoded fields.
    fn encode_sqe(
        opcode: u8,
        command_id: u16,
        nsid: u32,
        prp1: u64,
        cdw10: u32,
        cdw11: u32,
        cdw12: u32,
    ) -> [u8; 64] {
        encode_sqe_with_prps(opcode, command_id, nsid, prp1, 0, cdw10, cdw11, cdw12)
    }

    fn encode_sqe_with_prps(
        opcode: u8,
        command_id: u16,
        nsid: u32,
        prp1: u64,
        prp2: u64,
        cdw10: u32,
        cdw11: u32,
        cdw12: u32,
    ) -> [u8; 64] {
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
    const MEM_BASE: u64 = 0x4000_0000;
    const ASQ_BASE: u64 = 0x4000_1000; // admin submission queue
    const ACQ_BASE: u64 = 0x4000_2000; // admin completion queue
    const IO_SQ_BASE: u64 = 0x4000_3000;
    const IO_CQ_BASE: u64 = 0x4000_4000;
    const DATA_BASE: u64 = 0x4000_5000; // PRP data buffer
    const QDEPTH: u16 = 8;

    fn temp_path(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "bridgevm-hvf-nvme-{name}-{}-{nanos}",
            std::process::id()
        ))
    }

    /// Enable a fresh controller with admin queues installed.
    fn enabled_controller_with_disk_and_mem_len(
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

    fn enabled_controller_with_mem_len(mem_len: usize) -> (NvmeController, FakeMem) {
        enabled_controller_with_disk_and_mem_len(vec![0u8; 1 << 20], mem_len)
    }

    fn enabled_controller() -> (NvmeController, FakeMem) {
        enabled_controller_with_mem_len(0x8000)
    }

    fn enabled_controller_with_raw_file(
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
    fn submit_admin(ctrl: &mut NvmeController, mem: &mut FakeMem, slot: u16, sqe: &[u8; 64]) {
        let gpa = ASQ_BASE + u64::from(slot) * SQ_ENTRY_SIZE;
        assert!(mem.write_bytes(gpa, sqe));
        // Ring SQ0 tail doorbell (offset 0x1000) with new tail = slot + 1.
        ctrl.mmio_write(REG_DOORBELL_BASE, 4, u64::from(slot + 1));
        ctrl.process(mem);
    }

    fn read_completion(mem: &FakeMem, cq_base: u64, slot: u16) -> [u8; 16] {
        let gpa = cq_base + u64::from(slot) * CQ_ENTRY_SIZE;
        let raw = mem.read_bytes(gpa, 16).unwrap();
        let mut e = [0u8; 16];
        e.copy_from_slice(&raw);
        e
    }

    fn completion_status(entry: &[u8; 16]) -> u16 {
        u16::from_le_bytes([entry[14], entry[15]]) >> 1
    }

    fn create_io_queue_pair(
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

    #[test]
    fn cap_advertises_small_mqes_and_zero_dstrd() {
        let ctrl = NvmeController::new(0);
        let cap = ctrl.mmio_read(REG_CAP, 8);
        // MQES is 0-based; we advertise MAX_QUEUE_ENTRIES.
        let mqes = (cap & 0xffff) as u16 + 1;
        assert_eq!(mqes, MAX_QUEUE_ENTRIES);
        // DSTRD (bits 35:32) must be 0 ⇒ 4-byte doorbell stride.
        assert_eq!((cap >> 32) & 0xf, 0);
        // NVM command set bit (37) must be set.
        assert_ne!(cap & (1 << 37), 0);
    }

    #[test]
    fn doorbell_decode_stays_inside_the_modelled_aperture() {
        assert!(is_modelled_doorbell(REG_DOORBELL_BASE));
        assert!(is_modelled_doorbell(REG_DOORBELL_END - 4));
        assert!(!is_modelled_doorbell(REG_DOORBELL_END));
        assert!(!is_modelled_doorbell(REG_DOORBELL_BASE + 2));

        let (mut ctrl, _mem) = enabled_controller();
        ctrl.mmio_write(REG_DOORBELL_END, 4, 7);
        let admin_sq = ctrl.sqs[0].as_ref().expect("admin SQ installed");
        assert_eq!(
            admin_sq.tail_doorbell, 0,
            "BAR offsets beyond the modelled doorbells must not be treated as SQ0TDBL"
        );
    }

    #[test]
    fn msix_table_and_pba_live_in_bar0_without_overlapping_doorbells() {
        let mut ctrl = NvmeController::new(0);
        let table = u64::from(NVME_MSIX_TABLE_OFFSET);
        let pba = u64::from(NVME_MSIX_PBA_OFFSET);

        assert_eq!(ctrl.mmio_read(table + 12, 4), 1, "vectors start masked");
        ctrl.mmio_write(table, 8, 0x0808_0000);
        ctrl.mmio_write(table + 8, 4, 35);

        assert_eq!(ctrl.raise_msix(0, true, false), None);
        assert_eq!(ctrl.mmio_read(pba, 8), 1, "masked vector sets PBA bit");

        ctrl.mmio_write(table + 12, 4, 0);
        assert_eq!(
            ctrl.drain_pending_msix(true, false),
            vec![MsixMessage {
                vector: 0,
                address: 0x0808_0000,
                data: 35,
            }]
        );
        assert_eq!(ctrl.mmio_read(pba, 8), 0);
    }

    #[test]
    fn cap_low_half_readable_as_32_bits() {
        let ctrl = NvmeController::new(0);
        let lo = ctrl.mmio_read(REG_CAP, 4);
        let mqes = (lo & 0xffff) as u16 + 1;
        assert_eq!(mqes, MAX_QUEUE_ENTRIES);
    }

    #[test]
    fn vs_reads_1_4_0() {
        let ctrl = NvmeController::new(0);
        assert_eq!(ctrl.mmio_read(REG_VS, 4), u64::from(NVME_VERSION_1_4_0));
        assert_eq!(ctrl.mmio_read(REG_VS, 4), 0x0001_0400);
    }

    #[test]
    fn disk_image_constructor_pads_and_snapshots_media() {
        let mut ctrl = NvmeController::with_disk_image(vec![0xaa; LBA_SIZE + 7]);
        assert_eq!(ctrl.disk_image().len(), LBA_SIZE * 2);
        assert_eq!(ctrl.block_count(), 2);
        assert_eq!(ctrl.disk_image()[0], 0xaa);
        assert_eq!(ctrl.disk_image()[LBA_SIZE + 6], 0xaa);
        assert_eq!(ctrl.disk_image()[LBA_SIZE + 7], 0);

        ctrl.load_disk_image(vec![0xbb; 3]);
        assert_eq!(ctrl.disk_image().len(), LBA_SIZE);
        assert_eq!(ctrl.block_count(), 1);
        assert_eq!(&ctrl.disk_image()[..3], &[0xbb; 3]);
    }

    #[test]
    fn raw_file_backend_uses_sparse_overlay_and_exports_snapshot() {
        let source = temp_path("raw-overlay-source");
        let snapshot = temp_path("raw-overlay-snapshot");
        let slba = 5u64;
        let start = slba as usize * LBA_SIZE;
        let mut disk = vec![0u8; LBA_SIZE * 16];
        let original: Vec<u8> = (0..LBA_SIZE).map(|i| 0x20 | (i % 0x20) as u8).collect();
        disk[start..start + LBA_SIZE].copy_from_slice(&original);
        fs::write(&source, &disk).unwrap();

        let (mut ctrl, mut mem) = enabled_controller_with_raw_file(&source, false, 0x10000);
        assert_eq!(ctrl.block_count(), 16);
        assert!(ctrl.disk_image_if_memory().is_none());
        create_io_queue_pair(&mut ctrl, &mut mem, 0, CREATE_IO_CQ_PC_BIT);

        let read = encode_sqe(
            NVM_OP_READ,
            0x30,
            NSID,
            DATA_BASE,
            slba as u32,
            (slba >> 32) as u32,
            0,
        );
        assert!(mem.write_bytes(IO_SQ_BASE, &read));
        ctrl.mmio_write(REG_DOORBELL_BASE + 2 * 4, 4, 1);
        ctrl.process(&mut mem);
        assert_eq!(mem.read_bytes(DATA_BASE, LBA_SIZE).unwrap(), original);

        let replacement: Vec<u8> = (0..LBA_SIZE).map(|i| 0x80 | (i % 0x40) as u8).collect();
        assert!(mem.write_bytes(DATA_BASE, &replacement));
        let write = encode_sqe(
            NVM_OP_WRITE,
            0x31,
            NSID,
            DATA_BASE,
            slba as u32,
            (slba >> 32) as u32,
            0,
        );
        assert!(mem.write_bytes(IO_SQ_BASE + SQ_ENTRY_SIZE, &write));
        ctrl.mmio_write(REG_DOORBELL_BASE + 2 * 4, 4, 2);
        ctrl.process(&mut mem);
        assert_eq!(
            completion_status(&read_completion(&mem, IO_CQ_BASE, 1)),
            SC_SUCCESS
        );

        assert_eq!(
            &fs::read(&source).unwrap()[start..start + LBA_SIZE],
            original.as_slice(),
            "read-only file backend keeps guest writes in the overlay"
        );
        assert_eq!(
            ctrl.export_disk_image(&snapshot).unwrap(),
            disk.len() as u64
        );
        assert_eq!(
            &fs::read(&snapshot).unwrap()[start..start + LBA_SIZE],
            replacement.as_slice(),
            "snapshot export applies overlay writes"
        );

        fs::remove_file(source).ok();
        fs::remove_file(snapshot).ok();
    }

    #[test]
    fn raw_file_backend_write_back_updates_source_file() {
        let source = temp_path("raw-writeback-source");
        let slba = 3u64;
        let start = slba as usize * LBA_SIZE;
        fs::write(&source, vec![0u8; LBA_SIZE * 8]).unwrap();

        let (mut ctrl, mut mem) = enabled_controller_with_raw_file(&source, true, 0x10000);
        create_io_queue_pair(&mut ctrl, &mut mem, 0, CREATE_IO_CQ_PC_BIT);

        let replacement: Vec<u8> = (0..LBA_SIZE).map(|i| 0x40 | (i % 0x20) as u8).collect();
        assert!(mem.write_bytes(DATA_BASE, &replacement));
        let write = encode_sqe(
            NVM_OP_WRITE,
            0x32,
            NSID,
            DATA_BASE,
            slba as u32,
            (slba >> 32) as u32,
            0,
        );
        assert!(mem.write_bytes(IO_SQ_BASE, &write));
        ctrl.mmio_write(REG_DOORBELL_BASE + 2 * 4, 4, 1);
        ctrl.process(&mut mem);
        ctrl.flush_disk().unwrap();

        assert_eq!(
            &fs::read(&source).unwrap()[start..start + LBA_SIZE],
            replacement.as_slice(),
            "write-back file backend persists guest writes to the source"
        );

        fs::remove_file(source).ok();
    }

    #[test]
    fn enabling_cc_sets_csts_rdy() {
        let mut ctrl = NvmeController::new(0);
        assert_eq!(
            ctrl.mmio_read(REG_CSTS, 4) & 1,
            0,
            "RDY clear before enable"
        );
        ctrl.mmio_write(
            REG_AQA,
            4,
            (u32::from(QDEPTH - 1) << 16 | u32::from(QDEPTH - 1)).into(),
        );
        ctrl.mmio_write(REG_ASQ, 8, ASQ_BASE);
        ctrl.mmio_write(REG_ACQ, 8, ACQ_BASE);
        ctrl.mmio_write(REG_CC, 4, u64::from(CC_EN_BIT));
        assert_eq!(ctrl.mmio_read(REG_CSTS, 4) & 1, 1, "RDY follows CC.EN");
        // Disabling clears RDY again.
        ctrl.mmio_write(REG_CC, 4, 0);
        assert_eq!(ctrl.mmio_read(REG_CSTS, 4) & 1, 0);
    }

    #[test]
    fn identify_controller_produces_completion_and_valid_struct() {
        let (mut ctrl, mut mem) = enabled_controller();
        // IDENTIFY, CNS=1 (controller), data → DATA_BASE.
        let sqe = encode_sqe(
            ADMIN_OP_IDENTIFY,
            0x55,
            0,
            DATA_BASE,
            IDENTIFY_CNS_CONTROLLER,
            0,
            0,
        );
        submit_admin(&mut ctrl, &mut mem, 0, &sqe);

        // A completion landed in slot 0 of the admin CQ, success, matching CID.
        let cqe = read_completion(&mem, ACQ_BASE, 0);
        assert_eq!(completion_status(&cqe), SC_SUCCESS);
        let cid = u16::from_le_bytes([cqe[12], cqe[13]]);
        assert_eq!(cid, 0x55);
        // Phase tag set on first lap.
        assert_eq!(cqe[14] & 1, 1);

        // The identify struct is 4 KiB and carries NN = 1 namespace and the
        // expected SQES/CQES entry-size encoding.
        let id = mem.read_bytes(DATA_BASE, PAGE_SIZE).unwrap();
        assert_eq!(id.len(), PAGE_SIZE);
        let nn = u32::from_le_bytes([id[516], id[517], id[518], id[519]]);
        assert_eq!(nn, 1, "one namespace");
        assert_eq!(id[512], 0x66, "SQES = 64-byte entries");
        assert_eq!(id[513], 0x44, "CQES = 16-byte entries");
        assert!(
            id[768..1024].starts_with(b"nqn.2026-06.dev.bridgevm:bridgevm-hvf:nvme0\0"),
            "SUBNQN must be present and NUL-terminated for Linux"
        );
    }

    #[test]
    fn process_reports_admin_completion_vector_zero() {
        let (mut ctrl, mut mem) = enabled_controller();
        let sqe = encode_sqe(
            ADMIN_OP_IDENTIFY,
            0x56,
            0,
            DATA_BASE,
            IDENTIFY_CNS_CONTROLLER,
            0,
            0,
        );
        assert!(mem.write_bytes(ASQ_BASE, &sqe));
        ctrl.mmio_write(REG_DOORBELL_BASE, 4, 1);

        assert_eq!(
            ctrl.process(&mut mem),
            vec![NvmeCompletionEvent { cqid: 0, vector: 0 }]
        );
    }

    #[test]
    fn identify_namespace_reports_512b_lba_and_capacity() {
        let (mut ctrl, mut mem) = enabled_controller();
        let sqe = encode_sqe(
            ADMIN_OP_IDENTIFY,
            1,
            NSID,
            DATA_BASE,
            IDENTIFY_CNS_NAMESPACE,
            0,
            0,
        );
        submit_admin(&mut ctrl, &mut mem, 0, &sqe);
        assert_eq!(
            completion_status(&read_completion(&mem, ACQ_BASE, 0)),
            SC_SUCCESS
        );

        let id = mem.read_bytes(DATA_BASE, PAGE_SIZE).unwrap();
        // NSZE = total logical blocks = disk size / 512.
        let nsze = u64::from_le_bytes(id[0..8].try_into().unwrap());
        assert_eq!(nsze, (1 << 20) / LBA_SIZE as u64);
        // LBAF0 LBADS (bits 23:16) = 9 ⇒ 2^9 = 512-byte LBAs.
        let lbaf0 = u32::from_le_bytes([id[128], id[129], id[130], id[131]]);
        assert_eq!((lbaf0 >> 16) & 0xff, 9);
        assert_eq!(&id[104..120], &NS_NGUID);
        assert_eq!(&id[120..128], &NS_EUI64);
    }

    #[test]
    fn identify_active_namespace_list_reports_nsid_one_once() {
        let (mut ctrl, mut mem) = enabled_controller();
        let sqe = encode_sqe(
            ADMIN_OP_IDENTIFY,
            2,
            0,
            DATA_BASE,
            IDENTIFY_CNS_ACTIVE_NAMESPACE_LIST,
            0,
            0,
        );
        submit_admin(&mut ctrl, &mut mem, 0, &sqe);
        assert_eq!(
            completion_status(&read_completion(&mem, ACQ_BASE, 0)),
            SC_SUCCESS
        );

        let list = mem.read_bytes(DATA_BASE, PAGE_SIZE).unwrap();
        assert_eq!(u32::from_le_bytes(list[0..4].try_into().unwrap()), NSID);
        assert_eq!(
            u32::from_le_bytes(list[4..8].try_into().unwrap()),
            0,
            "namespace list must be zero-terminated"
        );

        let sqe = encode_sqe(
            ADMIN_OP_IDENTIFY,
            3,
            NSID,
            DATA_BASE + PAGE_SIZE as u64,
            IDENTIFY_CNS_ACTIVE_NAMESPACE_LIST,
            0,
            0,
        );
        submit_admin(&mut ctrl, &mut mem, 1, &sqe);
        assert_eq!(
            completion_status(&read_completion(&mem, ACQ_BASE, 1)),
            SC_SUCCESS
        );
        let empty = mem
            .read_bytes(DATA_BASE + PAGE_SIZE as u64, PAGE_SIZE)
            .unwrap();
        assert_eq!(
            u32::from_le_bytes(empty[0..4].try_into().unwrap()),
            0,
            "no active namespaces follow NSID 1"
        );
    }

    #[test]
    fn identify_namespace_descriptor_list_reports_stable_identifiers() {
        let (mut ctrl, mut mem) = enabled_controller();
        let sqe = encode_sqe(
            ADMIN_OP_IDENTIFY,
            4,
            NSID,
            DATA_BASE,
            IDENTIFY_CNS_NAMESPACE_DESCRIPTOR_LIST,
            0,
            0,
        );
        submit_admin(&mut ctrl, &mut mem, 0, &sqe);
        assert_eq!(
            completion_status(&read_completion(&mem, ACQ_BASE, 0)),
            SC_SUCCESS
        );

        let desc = mem.read_bytes(DATA_BASE, PAGE_SIZE).unwrap();
        assert_eq!(desc[0], 0x03, "first descriptor is UUID");
        assert_eq!(desc[1], 16, "UUID descriptor length");
        assert_eq!(
            &desc[4..20],
            &NS_UUID,
            "UUID descriptor carries the stable namespace UUID"
        );
        assert_eq!(desc[20], 0x02, "second descriptor is NGUID");
        assert_eq!(desc[21], 16, "NGUID descriptor length");
        assert_eq!(&desc[24..40], &NS_NGUID);
        assert_eq!(desc[40], 0x01, "third descriptor is EUI64");
        assert_eq!(desc[41], 8, "EUI64 descriptor length");
        assert_eq!(&desc[44..52], &NS_EUI64);
        assert_eq!(desc[52], 0, "zero descriptor length terminates the list");
    }

    #[test]
    fn get_log_page_smart_health_completes() {
        let (mut ctrl, mut mem) = enabled_controller();
        let numd = (512u32 / 4) - 1;
        let cdw10 = (numd << 16) | u32::from(LOG_PAGE_SMART_HEALTH);
        let sqe = encode_sqe(
            ADMIN_OP_GET_LOG_PAGE,
            5,
            0xffff_ffff,
            DATA_BASE,
            cdw10,
            0,
            0,
        );
        submit_admin(&mut ctrl, &mut mem, 0, &sqe);
        assert_eq!(
            completion_status(&read_completion(&mem, ACQ_BASE, 0)),
            SC_SUCCESS
        );

        let smart = mem.read_bytes(DATA_BASE, 512).unwrap();
        assert_eq!(smart[0], 0, "no critical warning bits set");
        assert_eq!(
            u16::from_le_bytes([smart[1], smart[2]]),
            300,
            "composite temperature is reported in Kelvin"
        );
        assert_eq!(smart[3], 100, "available spare percentage");
    }

    #[test]
    fn set_features_number_of_queues_completes() {
        let (mut ctrl, mut mem) = enabled_controller();
        // Request more queues than we have; controller grants what it can.
        let cdw11 = (3u32 << 16) | 3; // NCQR=3, NSQR=3 (0-based)
        let sqe = encode_sqe(
            ADMIN_OP_SET_FEATURES,
            7,
            0,
            0,
            u32::from(FEATURE_NUMBER_OF_QUEUES),
            cdw11,
            0,
        );
        submit_admin(&mut ctrl, &mut mem, 0, &sqe);
        let cqe = read_completion(&mem, ACQ_BASE, 0);
        assert_eq!(completion_status(&cqe), SC_SUCCESS);
    }

    #[test]
    fn create_io_queues_then_write_read_round_trips_one_lba() {
        let (mut ctrl, mut mem) = enabled_controller();

        // 1) CREATE I/O COMPLETION QUEUE (QID 1, depth QDEPTH, base IO_CQ_BASE).
        let cdw10 = (u32::from(QDEPTH - 1) << 16) | 1; // QSIZE(0-based)<<16 | QID
        let cq_cmd = encode_sqe(
            ADMIN_OP_CREATE_IO_CQ,
            1,
            0,
            IO_CQ_BASE,
            cdw10,
            CREATE_IO_CQ_PC_BIT,
            0,
        );
        submit_admin(&mut ctrl, &mut mem, 0, &cq_cmd);
        assert_eq!(
            completion_status(&read_completion(&mem, ACQ_BASE, 0)),
            SC_SUCCESS
        );

        // 2) CREATE I/O SUBMISSION QUEUE (QID 1 → CQID 1, base IO_SQ_BASE).
        let sq_cmd = encode_sqe(
            ADMIN_OP_CREATE_IO_SQ,
            2,
            0,
            IO_SQ_BASE,
            cdw10,
            1u32 << 16, // CQID = 1 in bits 31:16
            0,
        );
        submit_admin(&mut ctrl, &mut mem, 1, &sq_cmd);
        assert_eq!(
            completion_status(&read_completion(&mem, ACQ_BASE, 1)),
            SC_SUCCESS
        );

        // 3) Stage a known pattern in the guest data buffer and WRITE LBA 7.
        let pattern: Vec<u8> = (0..LBA_SIZE).map(|i| (i % 256) as u8).collect();
        assert!(mem.write_bytes(DATA_BASE, &pattern));
        let slba: u64 = 7;
        let write_cmd = encode_sqe(
            NVM_OP_WRITE,
            0x10,
            NSID,
            DATA_BASE,
            slba as u32,         // CDW10 = SLBA low
            (slba >> 32) as u32, // CDW11 = SLBA high
            0,                   // CDW12 = NLB 0-based ⇒ 1 block
        );
        // I/O SQ 1 tail doorbell is at DOORBELL_BASE + 2*4 (SQ1TDBL).
        let io_sq1_dbl = REG_DOORBELL_BASE + 2 * 4;
        let gpa = IO_SQ_BASE; // slot 0
        assert!(mem.write_bytes(gpa, &write_cmd));
        ctrl.mmio_write(io_sq1_dbl, 4, 1); // tail = 1
        ctrl.process(&mut mem);
        let w_cqe = read_completion(&mem, IO_CQ_BASE, 0);
        assert_eq!(completion_status(&w_cqe), SC_SUCCESS, "WRITE completes ok");

        // 4) Zero the data buffer, then READ LBA 7 back into it.
        assert!(mem.write_bytes(DATA_BASE, &vec![0u8; LBA_SIZE]));
        let read_cmd = encode_sqe(
            NVM_OP_READ,
            0x11,
            NSID,
            DATA_BASE,
            slba as u32,
            (slba >> 32) as u32,
            0,
        );
        assert!(mem.write_bytes(IO_SQ_BASE + SQ_ENTRY_SIZE, &read_cmd)); // slot 1
        ctrl.mmio_write(io_sq1_dbl, 4, 2); // tail = 2
        ctrl.process(&mut mem);
        let r_cqe = read_completion(&mem, IO_CQ_BASE, 1);
        assert_eq!(completion_status(&r_cqe), SC_SUCCESS, "READ completes ok");

        // 5) The data round-trips through the disk backend byte-for-byte.
        let got = mem.read_bytes(DATA_BASE, LBA_SIZE).unwrap();
        assert_eq!(got, pattern, "WRITE then READ of one LBA round-trips");
    }

    #[test]
    fn io_completion_queue_uses_interrupt_vector_from_cdw11_high_half() {
        let (mut ctrl, mut mem) = enabled_controller();
        let cdw10 = (u32::from(QDEPTH - 1) << 16) | 1;
        let cq_cdw11 = CREATE_IO_CQ_PC_BIT | CREATE_IO_CQ_IEN_BIT | (1u32 << CREATE_IO_CQ_IV_SHIFT);

        submit_admin(
            &mut ctrl,
            &mut mem,
            0,
            &encode_sqe(ADMIN_OP_CREATE_IO_CQ, 1, 0, IO_CQ_BASE, cdw10, cq_cdw11, 0),
        );
        assert_eq!(
            completion_status(&read_completion(&mem, ACQ_BASE, 0)),
            SC_SUCCESS
        );

        submit_admin(
            &mut ctrl,
            &mut mem,
            1,
            &encode_sqe(
                ADMIN_OP_CREATE_IO_SQ,
                2,
                0,
                IO_SQ_BASE,
                cdw10,
                1u32 << 16,
                0,
            ),
        );
        assert_eq!(
            completion_status(&read_completion(&mem, ACQ_BASE, 1)),
            SC_SUCCESS
        );

        let read_cmd = encode_sqe(NVM_OP_READ, 0x44, NSID, DATA_BASE, 0, 0, 0);
        assert!(mem.write_bytes(IO_SQ_BASE, &read_cmd));
        ctrl.mmio_write(REG_DOORBELL_BASE + 2 * 4, 4, 1);

        assert_eq!(
            ctrl.process(&mut mem),
            vec![NvmeCompletionEvent { cqid: 1, vector: 1 }],
            "CQ interrupt vector is CDW11[31:16], not the low PC/IEN bits"
        );
        assert_eq!(
            completion_status(&read_completion(&mem, IO_CQ_BASE, 0)),
            SC_SUCCESS
        );
    }

    #[test]
    fn read_uses_prp2_for_two_page_transfer() {
        let disk: Vec<u8> = (0..PAGE_SIZE * 4).map(|i| (i % 251) as u8).collect();
        let (mut ctrl, mut mem) = enabled_controller_with_disk_and_mem_len(disk.clone(), 0x10000);
        create_io_queue_pair(&mut ctrl, &mut mem, 0, CREATE_IO_CQ_PC_BIT);

        let second_page = DATA_BASE + PAGE_SIZE_U64;
        let read_cmd = encode_sqe_with_prps(
            NVM_OP_READ,
            0x50,
            NSID,
            DATA_BASE,
            second_page,
            0,
            0,
            15, // 16 LBAs = 8192 bytes = PRP1 + direct PRP2 page
        );
        assert!(mem.write_bytes(IO_SQ_BASE, &read_cmd));
        ctrl.mmio_write(REG_DOORBELL_BASE + 2 * 4, 4, 1);
        ctrl.process(&mut mem);

        assert_eq!(
            mem.read_bytes(DATA_BASE, PAGE_SIZE).unwrap(),
            disk[0..PAGE_SIZE]
        );
        assert_eq!(
            mem.read_bytes(second_page, PAGE_SIZE).unwrap(),
            disk[PAGE_SIZE..PAGE_SIZE * 2]
        );
        assert_eq!(
            completion_status(&read_completion(&mem, IO_CQ_BASE, 0)),
            SC_SUCCESS
        );
    }

    #[test]
    fn read_uses_prp_list_for_larger_transfer() {
        let pages = 6usize;
        let disk: Vec<u8> = (0..PAGE_SIZE * pages)
            .map(|i| 0x80 | ((i % 0x40) as u8))
            .collect();
        let (mut ctrl, mut mem) = enabled_controller_with_disk_and_mem_len(disk.clone(), 0x20000);
        create_io_queue_pair(&mut ctrl, &mut mem, 0, CREATE_IO_CQ_PC_BIT);

        let list_base = DATA_BASE + PAGE_SIZE_U64;
        let data0 = DATA_BASE + 2 * PAGE_SIZE_U64;
        let mut list = vec![0u8; PAGE_SIZE];
        for page in 1..pages {
            let gpa = data0 + (page as u64) * PAGE_SIZE_U64;
            let off = (page - 1) * 8;
            list[off..off + 8].copy_from_slice(&gpa.to_le_bytes());
        }
        assert!(mem.write_bytes(list_base, &list));

        let blocks = pages as u32 * (PAGE_SIZE as u32 / LBA_SIZE as u32);
        let read_cmd =
            encode_sqe_with_prps(NVM_OP_READ, 0x51, NSID, data0, list_base, 0, 0, blocks - 1);
        assert!(mem.write_bytes(IO_SQ_BASE, &read_cmd));
        ctrl.mmio_write(REG_DOORBELL_BASE + 2 * 4, 4, 1);
        ctrl.process(&mut mem);

        for page in 0..pages {
            let gpa = data0 + (page as u64) * PAGE_SIZE_U64;
            let start = page * PAGE_SIZE;
            assert_eq!(
                mem.read_bytes(gpa, PAGE_SIZE).unwrap(),
                disk[start..start + PAGE_SIZE],
                "page {page} should be populated through the PRP list"
            );
        }
        assert_eq!(
            completion_status(&read_completion(&mem, IO_CQ_BASE, 0)),
            SC_SUCCESS
        );
    }

    #[test]
    fn read_uses_prp_list_starting_at_prp2_offset() {
        let pages = 4usize;
        let disk: Vec<u8> = (0..PAGE_SIZE * pages)
            .map(|i| 0x20 | ((i % 0x5f) as u8))
            .collect();
        let (mut ctrl, mut mem) = enabled_controller_with_disk_and_mem_len(disk.clone(), 0x20000);
        create_io_queue_pair(&mut ctrl, &mut mem, 0, CREATE_IO_CQ_PC_BIT);

        let list_base = DATA_BASE + PAGE_SIZE_U64;
        let list_ptr = list_base + 0x100;
        let data0 = DATA_BASE + 2 * PAGE_SIZE_U64;
        let mut list = vec![0u8; (pages - 1) * 8];
        for page in 1..pages {
            let gpa = data0 + (page as u64) * PAGE_SIZE_U64;
            let off = (page - 1) * 8;
            list[off..off + 8].copy_from_slice(&gpa.to_le_bytes());
        }
        assert!(mem.write_bytes(list_ptr, &list));

        let blocks = pages as u32 * (PAGE_SIZE as u32 / LBA_SIZE as u32);
        let read_cmd =
            encode_sqe_with_prps(NVM_OP_READ, 0x53, NSID, data0, list_ptr, 0, 0, blocks - 1);
        assert!(mem.write_bytes(IO_SQ_BASE, &read_cmd));
        ctrl.mmio_write(REG_DOORBELL_BASE + 2 * 4, 4, 1);
        ctrl.process(&mut mem);

        for page in 0..pages {
            let gpa = data0 + (page as u64) * PAGE_SIZE_U64;
            let start = page * PAGE_SIZE;
            assert_eq!(
                mem.read_bytes(gpa, PAGE_SIZE).unwrap(),
                disk[start..start + PAGE_SIZE],
                "page {page} should be populated through the offset PRP list"
            );
        }
        assert_eq!(
            completion_status(&read_completion(&mem, IO_CQ_BASE, 0)),
            SC_SUCCESS
        );
    }

    #[test]
    fn write_uses_prp_list_for_larger_transfer() {
        let pages = 4usize;
        let (mut ctrl, mut mem) =
            enabled_controller_with_disk_and_mem_len(vec![0u8; PAGE_SIZE * pages], 0x18000);
        create_io_queue_pair(&mut ctrl, &mut mem, 0, CREATE_IO_CQ_PC_BIT);

        let list_base = DATA_BASE + PAGE_SIZE_U64;
        let data0 = DATA_BASE + 2 * PAGE_SIZE_U64;
        let replacement: Vec<u8> = (0..PAGE_SIZE * pages)
            .map(|i| 0x40 | ((i % 0x20) as u8))
            .collect();
        assert!(mem.write_bytes(data0, &replacement[0..PAGE_SIZE]));

        let mut list = vec![0u8; PAGE_SIZE];
        for page in 1..pages {
            let gpa = data0 + (page as u64) * PAGE_SIZE_U64;
            assert!(mem.write_bytes(gpa, &replacement[page * PAGE_SIZE..(page + 1) * PAGE_SIZE]));
            let off = (page - 1) * 8;
            list[off..off + 8].copy_from_slice(&gpa.to_le_bytes());
        }
        assert!(mem.write_bytes(list_base, &list));

        let blocks = pages as u32 * (PAGE_SIZE as u32 / LBA_SIZE as u32);
        let write_cmd =
            encode_sqe_with_prps(NVM_OP_WRITE, 0x52, NSID, data0, list_base, 0, 0, blocks - 1);
        assert!(mem.write_bytes(IO_SQ_BASE, &write_cmd));
        ctrl.mmio_write(REG_DOORBELL_BASE + 2 * 4, 4, 1);
        ctrl.process(&mut mem);

        assert_eq!(
            &ctrl.disk_image()[0..PAGE_SIZE * pages],
            replacement.as_slice()
        );
        assert_eq!(
            completion_status(&read_completion(&mem, IO_CQ_BASE, 0)),
            SC_SUCCESS
        );
    }

    #[test]
    fn read_out_of_range_lba_fails() {
        let (mut ctrl, mut mem) = enabled_controller();
        // Create I/O CQ + SQ (QID 1) as above.
        let cdw10 = (u32::from(QDEPTH - 1) << 16) | 1;
        submit_admin(
            &mut ctrl,
            &mut mem,
            0,
            &encode_sqe(
                ADMIN_OP_CREATE_IO_CQ,
                1,
                0,
                IO_CQ_BASE,
                cdw10,
                CREATE_IO_CQ_PC_BIT,
                0,
            ),
        );
        submit_admin(
            &mut ctrl,
            &mut mem,
            1,
            &encode_sqe(ADMIN_OP_CREATE_IO_SQ, 2, 0, IO_SQ_BASE, cdw10, 1 << 16, 0),
        );
        // Read a block far past the end of the 1 MiB disk.
        let bad_lba = 1u64 << 40;
        let read_cmd = encode_sqe(
            NVM_OP_READ,
            0x22,
            NSID,
            DATA_BASE,
            bad_lba as u32,
            (bad_lba >> 32) as u32,
            0,
        );
        assert!(mem.write_bytes(IO_SQ_BASE, &read_cmd));
        ctrl.mmio_write(REG_DOORBELL_BASE + 2 * 4, 4, 1);
        ctrl.process(&mut mem);
        let cqe = read_completion(&mem, IO_CQ_BASE, 0);
        assert_eq!(completion_status(&cqe), SC_INVALID_FIELD);
    }

    #[test]
    fn unknown_admin_opcode_reports_invalid_opcode() {
        let (mut ctrl, mut mem) = enabled_controller();
        let sqe = encode_sqe(0xfe, 0x99, 0, DATA_BASE, 0, 0, 0);
        submit_admin(&mut ctrl, &mut mem, 0, &sqe);
        let cqe = read_completion(&mem, ACQ_BASE, 0);
        assert_eq!(completion_status(&cqe), SC_INVALID_OPCODE);
        // Completion still references the submitting command id.
        assert_eq!(u16::from_le_bytes([cqe[12], cqe[13]]), 0x99);
    }
}
