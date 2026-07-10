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
//!     COMPLETION/SUBMISSION QUEUE, GET/SET FEATURES (small Windows-observed
//!     subset).
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
    collections::{BTreeMap, VecDeque},
    fs::{File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::Path,
    sync::OnceLock,
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
type NvmePage = [u8; PAGE_SIZE];
const PAGE_SIZE_U64: u64 = PAGE_SIZE as u64;
const FILE_OVERLAY_CHUNK_SIZE: u64 = PAGE_SIZE_U64;
const EXPORT_CHUNK_SIZE: usize = 1024 * 1024;
const ZERO_APST_FEATURE_DATA: [u8; 256] = [0u8; 256];
/// Maximum number of submission/completion queue entries we advertise
/// (`CAP.MQES` is 0-based, so the wire value is `MAX_QUEUE_ENTRIES - 1`).
pub const MAX_QUEUE_ENTRIES: u16 = 1024;
/// I/O queue-pair capacity advertised to SET FEATURES (NUMBER OF QUEUES). The
/// model only drives one, but exposes a small pool so a multi-queue guest gets
/// a sane non-zero allocation back.
pub const MAX_IO_QUEUE_PAIRS: u16 = 8;
/// Maximum outstanding Asynchronous Event Request commands retained without
/// completion. The identify controller AERL field advertises this as zero-based.
pub const MAX_ASYNC_EVENT_REQUESTS: u8 = 4;
/// Number of recent commands retained for live bring-up diagnostics.
pub const COMMAND_TRACE_CAPACITY: usize = 256;

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
/// Controller Memory Buffer Location (32-bit, RO). We advertise no CMB.
pub const REG_CMBLOC: u64 = 0x38;
/// Controller Memory Buffer Size (32-bit, RO). We advertise no CMB.
pub const REG_CMBSZ: u64 = 0x3C;
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
const ADMIN_OP_GET_FEATURES: u8 = 0x0a;
const ADMIN_OP_ASYNC_EVENT_REQUEST: u8 = 0x0c;
const ADMIN_OP_SECURITY_SEND: u8 = 0x81;
const ADMIN_OP_SECURITY_RECV: u8 = 0x82;

// ---- NVM (I/O) opcodes (NVMe NVM Command Set) -----------------------------
const NVM_OP_FLUSH: u8 = 0x00;
const NVM_OP_WRITE: u8 = 0x01;
const NVM_OP_READ: u8 = 0x02;

// ---- Command Set Identifiers (NVMe 1.4 §7.1) ------------------------------
const COMMAND_SET_NVM: u8 = 0x00;

// ---- IDENTIFY CNS values (NVMe 1.4 §5.15.1) -------------------------------
const IDENTIFY_CNS_NAMESPACE: u32 = 0x00;
const IDENTIFY_CNS_CONTROLLER: u32 = 0x01;
const IDENTIFY_CNS_ACTIVE_NAMESPACE_LIST: u32 = 0x02;
const IDENTIFY_CNS_NAMESPACE_DESCRIPTOR_LIST: u32 = 0x03;
const IDENTIFY_CNS_COMMAND_SET_CONTROLLER: u32 = 0x06;

// ---- SET FEATURES feature IDs (NVMe 1.4 §5.21.1) --------------------------
const FEATURE_ARBITRATION: u8 = 0x01;
const FEATURE_POWER_MANAGEMENT: u8 = 0x02;
const FEATURE_TEMPERATURE_THRESHOLD: u8 = 0x04;
const FEATURE_ERROR_RECOVERY: u8 = 0x05;
const FEATURE_VOLATILE_WRITE_CACHE: u8 = 0x06;
const FEATURE_NUMBER_OF_QUEUES: u8 = 0x07;
const FEATURE_INTERRUPT_COALESCING: u8 = 0x08;
const FEATURE_INTERRUPT_VECTOR_CONFIGURATION: u8 = 0x09;
const FEATURE_WRITE_ATOMICITY_NORMAL: u8 = 0x0a;
const FEATURE_ASYNC_EVENT_CONFIGURATION: u8 = 0x0b;
const FEATURE_AUTONOMOUS_POWER_STATE_TRANSITION: u8 = 0x0c;
const GET_FEATURE_SELECT_SHIFT: u32 = 8;
const GET_FEATURE_SELECT_DEFAULT: u32 = 0x1;
const GET_FEATURE_SELECT_SAVED: u32 = 0x2;
const GET_FEATURE_SELECT_CAPABILITIES: u32 = 0x3;
const FEATURE_CAP_NAMESPACE_SPECIFIC: u32 = 1 << 1;
const FEATURE_CAP_CHANGEABLE: u32 = 1 << 2;

// ---- Identify Controller feature bits -------------------------------------
const VWC_PRESENT: u8 = 1 << 0;
const VWC_NSID_BROADCAST_SUPPORT: u8 = 3 << 1;
const VWC_QEMU_DEFAULT: u8 = VWC_PRESENT | VWC_NSID_BROADCAST_SUPPORT;

// ---- CREATE I/O COMPLETION QUEUE fields (NVMe 1.4 §5.3) -------------------
const CREATE_IO_CQ_PC_BIT: u32 = 1 << 0;
const CREATE_IO_CQ_IEN_BIT: u32 = 1 << 1;
const CREATE_IO_CQ_IV_SHIFT: u32 = 16;

// ---- GET LOG PAGE log identifiers (NVMe 1.4 §5.14.1) ----------------------
const LOG_PAGE_SMART_HEALTH: u8 = 0x02;
const LOG_PAGE_FIRMWARE_SLOT_INFO: u8 = 0x03;
const LOG_PAGE_COMMAND_EFFECTS: u8 = 0x05;

// ---- Command Effects log bits (NVMe 1.4 §5.14.1.5) ------------------------
const CMD_EFFECT_CSUPP: u32 = 1 << 0;
const CMD_EFFECT_LBCC: u32 = 1 << 1;

// ---- Security protocol values (NVMe 1.4 §5.22 / QEMU nvme_security_*) -----
const SECURITY_PROTOCOL_INFORMATION: u8 = 0x00;
const SECURITY_PROTOCOL_DMTF_SPDM: u8 = 0xe8;
const SECURITY_PROTOCOL_INFO_LIST_LEN: usize = 10;

// ---- Completion status codes (NVMe 1.4 §4.6.1, generic command status) ----
/// Successful completion (status code type 0, code 0).
const SC_SUCCESS: u16 = 0x0000;
/// Invalid Field in Command.
const SC_INVALID_FIELD: u16 = 0x0002;
/// Do Not Retry bit, carried in the NVMe completion status code field.
const SC_DNR: u16 = 0x4000;
/// QEMU's default for unsupported optional/vendor command surfaces.
const SC_INVALID_FIELD_DNR: u16 = SC_INVALID_FIELD | SC_DNR;
/// Invalid Opcode.
const SC_INVALID_OPCODE: u16 = 0x0001;

/// The primary namespace's identifier (NSID 1).
pub const NSID: u32 = 1;
/// Optional second namespace (NSID 2), used as a blank Windows install target
/// alongside the NSID-1 installer source.
pub const NSID2: u32 = 2;

const NS_EUI64: [u8; 8] = *b"BVMNVME1";
const NS_NGUID: [u8; 16] = *b"BridgeVM-NVMeNS1";
const NS_UUID: [u8; 16] = [
    0x42, 0x56, 0x4d, 0x00, 0x20, 0x26, 0x06, 0x20, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01,
];
const NS2_EUI64: [u8; 8] = *b"BVMNVME2";
const NS2_NGUID: [u8; 16] = *b"BridgeVM-NVMeNS2";
const NS2_UUID: [u8; 16] = [
    0x42, 0x56, 0x4d, 0x00, 0x20, 0x26, 0x06, 0x20, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02,
];

/// Per-namespace stable identifiers (NGUID, EUI64, UUID) for NSID 1 and 2.
fn namespace_identifiers(nsid: u32) -> ([u8; 16], [u8; 8], [u8; 16]) {
    if nsid == NSID2 {
        (NS2_NGUID, NS2_EUI64, NS2_UUID)
    } else {
        (NS_NGUID, NS_EUI64, NS_UUID)
    }
}

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

/// Completion routing metadata captured with a processed NVMe command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NvmeCompletionTrace {
    pub cqid: u16,
    pub vector: u16,
}

/// A recent NVMe submission entry processed by the controller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NvmeCommandTrace {
    pub sqid: u16,
    pub cqid: u16,
    pub sq_head: u16,
    pub sq_tail: u16,
    pub sq_entry_gpa: u64,
    pub opcode: u8,
    pub command_id: u16,
    pub nsid: u32,
    pub prp1: u64,
    pub prp2: u64,
    pub cdw10: u32,
    pub cdw11: u32,
    pub cdw12: u32,
    pub cdw13: u32,
    pub cdw14: u32,
    pub cdw15: u32,
    pub status: u16,
    pub completion_posted: bool,
    pub completion: Option<NvmeCompletionTrace>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CommandResult {
    status: u16,
    complete: bool,
}

impl CommandResult {
    const fn complete(status: u16) -> Self {
        Self {
            status,
            complete: true,
        }
    }

    const fn pending() -> Self {
        Self {
            status: SC_SUCCESS,
            complete: false,
        }
    }
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
        let mut out = vec![0u8; len];
        self.read_at_into(offset, &mut out)?;
        Ok(out)
    }

    /// Read `dst.len()` bytes at `offset` directly into `dst`. Both backends are
    /// bounds-checked against the namespace size, so a guest-supplied range that
    /// runs past the image is rejected rather than reading adjacent bytes. This is
    /// the coalesced data-path primitive: the NVMe engine issues one call per
    /// physically-contiguous guest segment (often the whole transfer) instead of
    /// one allocation + `pread` per 4 KiB PRP page.
    fn read_at_into(&mut self, offset: u64, dst: &mut [u8]) -> io::Result<()> {
        self.validate_range(offset, dst.len())?;
        match self {
            Self::Memory(disk) => {
                let start = offset as usize;
                dst.copy_from_slice(&disk[start..start + dst.len()]);
                Ok(())
            }
            Self::RawFile(disk) => disk.read_at_into(offset, dst),
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
    /// Read `dst.len()` bytes at `offset`: one `pread` of the whole span from the
    /// backing file, then the sparse write overlay merged on top. Merging over the
    /// whole span (rather than page-by-page) keeps the coalesced read a single
    /// syscall while preserving the COW-overlay read semantics exactly.
    fn read_at_into(&mut self, offset: u64, dst: &mut [u8]) -> io::Result<()> {
        if dst.is_empty() {
            return Ok(());
        }
        let end = offset + dst.len() as u64;
        self.file.seek(SeekFrom::Start(offset))?;
        self.file.read_exact(dst)?;
        let overlay_start = offset.saturating_sub(FILE_OVERLAY_CHUNK_SIZE - 1);
        for (&chunk_base, chunk) in self.overlay.range(overlay_start..end) {
            let chunk_end = chunk_base + chunk.len() as u64;
            if chunk_end <= offset {
                continue;
            }
            let copy_start = offset.max(chunk_base);
            let copy_end = end.min(chunk_end);
            let src = (copy_start - chunk_base) as usize;
            let dst_off = (copy_start - offset) as usize;
            let copy_len = (copy_end - copy_start) as usize;
            dst[dst_off..dst_off + copy_len].copy_from_slice(&chunk[src..src + copy_len]);
        }
        Ok(())
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

fn second_namespace_missing() -> io::Error {
    io::Error::new(io::ErrorKind::NotFound, "NVMe NSID 2 is not attached")
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
    pending_sq_bits: Vec<u64>,

    // --- Backend ---
    /// Raw disk backing store for NSID 1, `LBA_SIZE`-byte logical blocks.
    disk: DiskBackend,
    /// Optional NSID-2 backing store (blank Windows install target).
    disk2: Option<DiskBackend>,
    /// Negotiated maximum number of I/O queue pairs (SET FEATURES 0x07).
    max_io_queues: u16,
    /// Command-specific result for the *next* completion's DW0 (e.g. the queue
    /// count granted by SET FEATURES). Consumed when the completion is posted.
    last_feature_result: u32,
    /// Current volatile write cache state. QEMU's default NVMe endpoint
    /// advertises a present cache and boots with it enabled.
    volatile_write_cache_enabled: bool,
    /// BAR-backed MSI-X table and PBA for this endpoint.
    msix: MsixTable,
    /// Recent command/completion history for live Windows bring-up.
    command_trace: VecDeque<NvmeCommandTrace>,
    /// Reusable staging buffer for the data path's buffered fallback (used when
    /// the guest-memory view exposes no stable host pointer for direct DMA). Kept
    /// at its high-water mark across commands so a steady stream of IO reuses one
    /// allocation instead of allocating per PRP page. Holds no state between
    /// commands; each command fully overwrites the prefix it reads.
    io_scratch: Vec<u8>,
    /// Reusable PRP decode output for NVM READ/WRITE. The hot I/O path fills this
    /// per command instead of allocating a span vector for every transfer.
    prp_spans_scratch: Vec<(u64, usize)>,
    /// Reusable physically-contiguous guest segments derived from PRP spans.
    io_segments_scratch: Vec<(u64, usize)>,
    /// Outstanding AER commands that should complete only when an async event is
    /// raised. This minimal controller does not raise events yet, so it only
    /// tracks the advertised limit and leaves accepted requests pending.
    pending_async_event_requests: u8,
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
            pending_sq_bits: vec![0],
            disk,
            disk2: None,
            // Capacity for SET FEATURES (NUMBER OF QUEUES) to negotiate against.
            // The model only ever drives one I/O queue, but advertises a small
            // pool so a guest requesting several is granted a sane non-zero count.
            max_io_queues: MAX_IO_QUEUE_PAIRS,
            last_feature_result: 0,
            volatile_write_cache_enabled: true,
            msix: MsixTable::new(NVME_MSIX_VECTOR_COUNT),
            command_trace: VecDeque::with_capacity(COMMAND_TRACE_CAPACITY),
            io_scratch: Vec::new(),
            prp_spans_scratch: Vec::new(),
            io_segments_scratch: Vec::new(),
            pending_async_event_requests: 0,
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

    pub fn reset_registers_keep_disks(&mut self) {
        let disk = std::mem::replace(&mut self.disk, DiskBackend::memory(Vec::new()));
        let disk2 = self.disk2.take();
        *self = Self::with_disk_backend(disk);
        self.disk2 = disk2;
    }

    /// Attach a blank NSID-2 target namespace backed by `disk_bytes` of in-memory
    /// storage (rounded to a whole LBA). Windows sees a second, empty disk.
    pub fn attach_second_namespace(&mut self, disk_bytes: usize) {
        self.disk2 = Some(DiskBackend::memory(vec![0u8; rounded_disk_len(disk_bytes)]));
    }

    /// Attach an NSID-2 target namespace backed by a host raw file (see
    /// [`Self::with_raw_file`] for the `write_back` semantics).
    pub fn attach_second_namespace_raw_file(
        &mut self,
        path: impl AsRef<Path>,
        write_back: bool,
    ) -> io::Result<()> {
        self.disk2 = Some(DiskBackend::raw_file(path, write_back)?);
        Ok(())
    }

    /// Whether an NSID-2 target namespace is attached.
    pub fn has_second_namespace(&self) -> bool {
        self.disk2.is_some()
    }

    /// The number of active namespaces (1 or 2).
    fn namespace_count(&self) -> u32 {
        if self.disk2.is_some() {
            2
        } else {
            1
        }
    }

    /// Immutable backing store for `nsid`, if that namespace is active.
    fn backend_for_nsid(&self, nsid: u32) -> Option<&DiskBackend> {
        match nsid {
            NSID => Some(&self.disk),
            NSID2 => self.disk2.as_ref(),
            _ => None,
        }
    }

    /// Mutable backing store for `nsid`, if that namespace is active.
    fn backend_for_nsid_mut(&mut self, nsid: u32) -> Option<&mut DiskBackend> {
        match nsid {
            NSID => Some(&mut self.disk),
            NSID2 => self.disk2.as_mut(),
            _ => None,
        }
    }

    /// Block count for a specific namespace.
    fn block_count_for(&self, nsid: u32) -> u64 {
        self.backend_for_nsid(nsid)
            .map(|b| b.byte_len() / LBA_SIZE as u64)
            .unwrap_or(0)
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

    pub fn second_namespace_disk_image_if_memory(&self) -> Option<&[u8]> {
        self.disk2.as_ref().and_then(DiskBackend::memory_image)
    }

    /// Export the full current disk image to `path`, applying any sparse overlay
    /// writes on top of the source raw file.
    pub fn export_disk_image(&mut self, path: impl AsRef<Path>) -> io::Result<u64> {
        self.disk.export_to_path(path)
    }

    pub fn export_second_namespace_disk_image(
        &mut self,
        path: impl AsRef<Path>,
    ) -> io::Result<u64> {
        self.disk2
            .as_mut()
            .ok_or_else(second_namespace_missing)?
            .export_to_path(path)
    }

    /// Flush host-file-backed write-through media.
    pub fn flush_disk(&mut self) -> io::Result<()> {
        self.disk.flush()
    }

    pub fn flush_second_namespace_disk(&mut self) -> io::Result<()> {
        self.disk2
            .as_mut()
            .ok_or_else(second_namespace_missing)?
            .flush()
    }

    /// Current byte length of the backing disk.
    pub fn disk_len(&self) -> u64 {
        self.disk.byte_len()
    }

    pub fn second_namespace_disk_len(&self) -> Option<u64> {
        self.disk2.as_ref().map(DiskBackend::byte_len)
    }

    /// Number of `LBA_SIZE`-byte logical blocks in the backing disk.
    pub fn block_count(&self) -> u64 {
        self.disk.byte_len() / LBA_SIZE as u64
    }

    /// Snapshot recent commands processed by the queue engine, oldest first.
    pub fn recent_command_trace(&self) -> Vec<NvmeCommandTrace> {
        self.command_trace.iter().copied().collect()
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
            self.pending_sq_bits.clear();
            self.pending_sq_bits.push(0);
            self.csts |= CSTS_RDY_BIT;
        } else if !now_enabled && was_enabled {
            // Reset: drop all queues and clear ready.
            self.sqs = vec![None];
            self.cqs = vec![None];
            self.pending_sq_bits.clear();
            self.pending_sq_bits.push(0);
            self.csts &= !CSTS_RDY_BIT;
            self.pending_async_event_requests = 0;
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
        } else {
            let mut has_work = None;
            if let Some(Some(sq)) = self.sqs.get_mut(qid) {
                sq.tail_doorbell = val;
                has_work = Some(sq.head != sq.tail_doorbell);
            }
            match has_work {
                Some(true) => self.mark_sq_pending(qid),
                Some(false) => self.clear_sq_pending(qid),
                None => {}
            }
        }
    }

    // ---- Queue processing -------------------------------------------------

    /// Drain every submission queue whose doorbell has advanced past the
    /// controller's head, executing commands and posting completions back into
    /// guest RAM. The run loop calls this after each doorbell write (and may
    /// call it speculatively — it is a no-op when nothing is pending).
    pub fn process(&mut self, mem: &mut dyn GuestMemoryMut) -> Vec<NvmeCompletionEvent> {
        let mut completions = Vec::new();
        self.process_into(mem, &mut completions);
        completions
    }

    /// Drain pending submission queues, appending completion interrupt events to
    /// caller-owned storage.
    pub fn process_into(
        &mut self,
        mem: &mut dyn GuestMemoryMut,
        completions: &mut Vec<NvmeCompletionEvent>,
    ) {
        // Admin queue (index 0) first, then I/O queues whose tail doorbell has
        // advanced. The run loop calls this speculatively, so avoid scanning the
        // whole advertised queue space when no SQ has work.
        let mut word_idx = 0usize;
        while word_idx < self.pending_sq_bits.len() {
            let mut pending_word = self.pending_sq_bits[word_idx];
            while pending_word != 0 {
                let bit = pending_word.trailing_zeros() as usize;
                let qid = word_idx * 64 + bit;
                self.process_sq(qid, mem, completions);
                if !self.sq_has_work(qid) {
                    self.clear_sq_pending(qid);
                }
                pending_word &= !(1u64 << bit);
            }
            word_idx += 1;
        }
    }

    fn mark_sq_pending(&mut self, qid: usize) {
        let word_idx = qid / 64;
        if self.pending_sq_bits.len() <= word_idx {
            self.pending_sq_bits.resize(word_idx + 1, 0);
        }
        self.pending_sq_bits[word_idx] |= 1u64 << (qid % 64);
    }

    fn clear_sq_pending(&mut self, qid: usize) {
        let word_idx = qid / 64;
        if let Some(word) = self.pending_sq_bits.get_mut(word_idx) {
            *word &= !(1u64 << (qid % 64));
        }
    }

    fn sq_has_work(&self, qid: usize) -> bool {
        self.sqs
            .get(qid)
            .and_then(Option::as_ref)
            .is_some_and(|sq| sq.head != sq.tail_doorbell)
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
            let mut buf = [0u8; SQ_ENTRY_SIZE as usize];
            if !mem.read_into(entry_gpa, &mut buf) {
                return; // unbacked SQ memory; stop draining
            }
            let cmd = SubmissionEntry::from_bytes(&buf);

            // Advance the SQ head (wrapping at queue size).
            let new_head = (head + 1) % size;
            if let Some(Some(sq)) = self.sqs.get_mut(qid) {
                sq.head = new_head;
            }

            let result = if qid == 0 {
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
                    result.status
                );
            }
            let (completion_posted, completion) = if result.complete {
                self.post_completion(cqid, qid as u16, &cmd, result.status, mem)
            } else {
                (false, None)
            };
            self.record_command_trace(NvmeCommandTrace {
                sqid: qid as u16,
                cqid,
                sq_head: head,
                sq_tail: tail,
                sq_entry_gpa: entry_gpa,
                opcode: cmd.opcode,
                command_id: cmd.command_id,
                nsid: cmd.nsid,
                prp1: cmd.prp1,
                prp2: cmd.prp2,
                cdw10: cmd.cdw10,
                cdw11: cmd.cdw11,
                cdw12: cmd.cdw12,
                cdw13: cmd.cdw13,
                cdw14: cmd.cdw14,
                cdw15: cmd.cdw15,
                status: result.status,
                completion_posted,
                completion: completion.map(|c| NvmeCompletionTrace {
                    cqid: c.cqid,
                    vector: c.vector,
                }),
            });
            if let Some(completion) = completion {
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

    fn record_command_trace(&mut self, trace: NvmeCommandTrace) {
        if self.command_trace.len() == COMMAND_TRACE_CAPACITY {
            self.command_trace.pop_front();
        }
        self.command_trace.push_back(trace);
    }

    /// Execute an admin command, returning the NVMe status field to report.
    fn execute_admin(
        &mut self,
        cmd: &SubmissionEntry,
        mem: &mut dyn GuestMemoryMut,
    ) -> CommandResult {
        let status = match cmd.opcode {
            ADMIN_OP_IDENTIFY => self.admin_identify(cmd, mem),
            ADMIN_OP_GET_LOG_PAGE => self.admin_get_log_page(cmd, mem),
            ADMIN_OP_CREATE_IO_CQ => self.admin_create_io_cq(cmd),
            ADMIN_OP_CREATE_IO_SQ => self.admin_create_io_sq(cmd),
            ADMIN_OP_SET_FEATURES => self.admin_set_features(cmd),
            ADMIN_OP_GET_FEATURES => self.admin_get_features(cmd, mem),
            ADMIN_OP_ASYNC_EVENT_REQUEST => return self.admin_async_event_request(),
            ADMIN_OP_SECURITY_SEND => self.admin_security_send(cmd),
            ADMIN_OP_SECURITY_RECV => self.admin_security_receive(cmd, mem),
            ADMIN_OP_DELETE_IO_SQ | ADMIN_OP_DELETE_IO_CQ => SC_SUCCESS,
            _ => SC_INVALID_OPCODE,
        };
        CommandResult::complete(status)
    }

    fn admin_async_event_request(&mut self) -> CommandResult {
        if self.pending_async_event_requests >= MAX_ASYNC_EVENT_REQUESTS {
            return CommandResult::complete(SC_INVALID_FIELD);
        }
        self.pending_async_event_requests += 1;
        CommandResult::pending()
    }

    /// IDENTIFY (CNS in CDW10 bits 7:0). Writes a 4 KiB structure to PRP1.
    fn admin_identify(&self, cmd: &SubmissionEntry, mem: &mut dyn GuestMemoryMut) -> u16 {
        let cns = cmd.cdw10 & 0xff;
        let data = match cns {
            IDENTIFY_CNS_CONTROLLER => self.identify_controller(),
            IDENTIFY_CNS_COMMAND_SET_CONTROLLER => {
                let csi = ((cmd.cdw11 >> 24) & 0xff) as u8;
                if csi != COMMAND_SET_NVM {
                    return SC_INVALID_FIELD;
                }
                self.identify_command_set_controller()
            }
            IDENTIFY_CNS_ACTIVE_NAMESPACE_LIST => self.identify_active_namespace_list(cmd.nsid),
            IDENTIFY_CNS_NAMESPACE_DESCRIPTOR_LIST => {
                if self.backend_for_nsid(cmd.nsid).is_some() {
                    self.identify_namespace_descriptor_list(cmd.nsid)
                } else {
                    return SC_INVALID_FIELD;
                }
            }
            IDENTIFY_CNS_NAMESPACE => {
                if self.backend_for_nsid(cmd.nsid).is_some() {
                    self.identify_namespace(cmd.nsid)
                } else {
                    // Unallocated namespace ⇒ a zeroed structure (NVMe 1.4).
                    [0u8; PAGE_SIZE]
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
    fn identify_controller(&self) -> NvmePage {
        let mut d = [0u8; PAGE_SIZE];
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
        // AERL (259): maximum concurrently outstanding async event requests,
        // zero-based. Windows submits AERs during setup; they should remain
        // pending rather than completing as invalid opcodes.
        d[259] = MAX_ASYNC_EVENT_REQUESTS - 1;
        // OACS (256..258): advertise Security Send/Receive now that the minimal
        // QEMU-compatible security protocol information query is implemented.
        d[256..258].copy_from_slice(&1u16.to_le_bytes());
        // SQES (512): min/max submission-queue entry size = 2^6 = 64 bytes.
        d[512] = 0x66;
        // CQES (513): min/max completion-queue entry size = 2^4 = 16 bytes.
        d[513] = 0x44;
        // NN (516..520): maximum/number of namespaces (1 or 2).
        d[516..520].copy_from_slice(&self.namespace_count().to_le_bytes());
        // VWC (525): QEMU advertises a present volatile write cache and support
        // for broadcast-NSID flushes.
        d[525] = VWC_QEMU_DEFAULT;
        // SUBNQN (768..1024): NUL-terminated subsystem NQN. Linux warns for
        // NVMe >= 1.2.1 if this field is empty or consumes the whole NQN field.
        write_cstr(
            &mut d[768..1024],
            "nqn.2026-06.dev.bridgevm:bridgevm-hvf:nvme0",
        );
        d
    }

    /// Build a 4 KiB command-set-specific Identify Controller structure for the
    /// NVM command set (CNS=0x06, CSI=0). QEMU answers this Windows probe with
    /// an otherwise boring `NvmeIdCtrlNvm`; keep the BridgeVM page conservative
    /// rather than advertising optional NVM commands that are not implemented.
    fn identify_command_set_controller(&self) -> NvmePage {
        [0u8; PAGE_SIZE]
    }

    /// Build a 4 KiB Identify Namespace structure (NVMe 1.4 §5.15.2.1).
    fn identify_namespace(&self, nsid: u32) -> NvmePage {
        let mut d = [0u8; PAGE_SIZE];
        let nsze = self.block_count_for(nsid);
        // NSZE (0..8), NCAP (8..16), NUSE (16..24): all in logical blocks.
        d[0..8].copy_from_slice(&nsze.to_le_bytes());
        d[8..16].copy_from_slice(&nsze.to_le_bytes());
        d[16..24].copy_from_slice(&nsze.to_le_bytes());
        // NLBAF (25): number of LBA formats minus one ⇒ 0 ⇒ one format.
        d[25] = 0;
        // FLBAS (26): formatted LBA size ⇒ format index 0.
        d[26] = 0;
        // NGUID (104..120) / EUI64 (120..128): stable non-zero namespace IDs.
        let (nguid, eui64, _uuid) = namespace_identifiers(nsid);
        d[104..120].copy_from_slice(&nguid);
        d[120..128].copy_from_slice(&eui64);
        // LBAF0 (128..132): MS=0, LBADS = log2(512) = 9 (bits 23:16), RP=0.
        let lbads: u32 = 9 << 16;
        d[128..132].copy_from_slice(&lbads.to_le_bytes());
        d
    }

    /// Build a 4 KiB Identify Active Namespace ID List (CNS=0x02). The list
    /// contains active namespace IDs greater than the command NSID, in ascending
    /// order, terminated by zero.
    fn identify_active_namespace_list(&self, after_nsid: u32) -> NvmePage {
        let mut d = [0u8; PAGE_SIZE];
        let mut off = 0usize;
        for nsid in [NSID, NSID2] {
            if nsid > after_nsid && self.backend_for_nsid(nsid).is_some() {
                d[off..off + 4].copy_from_slice(&nsid.to_le_bytes());
                off += 4;
            }
        }
        d
    }

    /// Build a 4 KiB Identify Namespace Identification Descriptor List
    /// (CNS=0x03). UUID, NGUID and EUI64 descriptors mirror the stable namespace
    /// identifiers in Identify Namespace, followed by a zero descriptor header to
    /// terminate the list.
    fn identify_namespace_descriptor_list(&self, nsid: u32) -> NvmePage {
        let mut d = [0u8; PAGE_SIZE];
        let mut off = 0usize;
        let (nguid, eui64, uuid) = namespace_identifiers(nsid);
        append_namespace_id_descriptor(&mut d, &mut off, 0x03, &uuid);
        append_namespace_id_descriptor(&mut d, &mut off, 0x02, &nguid);
        append_namespace_id_descriptor(&mut d, &mut off, 0x01, &eui64);
        d
    }

    /// GET LOG PAGE. Linux reads SMART / health information during probe, and
    /// Windows asks for the command-effects log while sizing the controller.
    fn admin_get_log_page(&self, cmd: &SubmissionEntry, mem: &mut dyn GuestMemoryMut) -> u16 {
        let lid = (cmd.cdw10 & 0xff) as u8;
        let numdl = (cmd.cdw10 >> 16) & 0xffff;
        let numdu = cmd.cdw11 & 0xffff;
        let offset = ((u64::from(cmd.cdw13)) << 32) | u64::from(cmd.cdw12);
        if offset & 0x3 != 0 || offset >= PAGE_SIZE_U64 {
            return SC_INVALID_FIELD_DNR;
        }
        let dword_count = ((numdu << 16) | numdl).saturating_add(1);
        let max_len = PAGE_SIZE - offset as usize;
        let byte_count = (dword_count as usize).saturating_mul(4).min(max_len);

        let log = match lid {
            LOG_PAGE_SMART_HEALTH => self.smart_health_log(),
            LOG_PAGE_FIRMWARE_SLOT_INFO => self.firmware_slot_info_log(),
            LOG_PAGE_COMMAND_EFFECTS => self.command_effects_log(),
            _ => return SC_INVALID_FIELD_DNR,
        };
        let start = offset as usize;
        let data = &log[start..start + byte_count];
        if mem.write_bytes(cmd.prp1, data) {
            SC_SUCCESS
        } else {
            SC_INVALID_FIELD
        }
    }

    fn smart_health_log(&self) -> NvmePage {
        let mut d = [0u8; PAGE_SIZE];
        // Composite temperature in Kelvin, little-endian at bytes 1..3.
        // 300K is boring and healthy.
        d[1..3].copy_from_slice(&300u16.to_le_bytes());
        d[3] = 100; // available spare (%)
        d[4] = 10; // available spare threshold (%)
        d
    }

    fn firmware_slot_info_log(&self) -> NvmePage {
        let mut d = [0u8; PAGE_SIZE];
        // Active Firmware Info: active slot 1, no pending activation slot.
        d[0] = 1;
        write_ascii(&mut d[8..72], "BridgeVM NVMe firmware slot 1");
        d
    }

    fn command_effects_log(&self) -> NvmePage {
        let mut d = [0u8; PAGE_SIZE];
        let mut set_admin = |opcode: u8, effects: u32| {
            let off = usize::from(opcode) * 4;
            d[off..off + 4].copy_from_slice(&effects.to_le_bytes());
        };
        set_admin(ADMIN_OP_DELETE_IO_SQ, CMD_EFFECT_CSUPP);
        set_admin(ADMIN_OP_CREATE_IO_SQ, CMD_EFFECT_CSUPP);
        set_admin(ADMIN_OP_GET_LOG_PAGE, CMD_EFFECT_CSUPP);
        set_admin(ADMIN_OP_DELETE_IO_CQ, CMD_EFFECT_CSUPP);
        set_admin(ADMIN_OP_CREATE_IO_CQ, CMD_EFFECT_CSUPP);
        set_admin(ADMIN_OP_IDENTIFY, CMD_EFFECT_CSUPP);
        set_admin(ADMIN_OP_SET_FEATURES, CMD_EFFECT_CSUPP);
        set_admin(ADMIN_OP_GET_FEATURES, CMD_EFFECT_CSUPP);
        set_admin(ADMIN_OP_ASYNC_EVENT_REQUEST, CMD_EFFECT_CSUPP);
        set_admin(ADMIN_OP_SECURITY_SEND, CMD_EFFECT_CSUPP);
        set_admin(ADMIN_OP_SECURITY_RECV, CMD_EFFECT_CSUPP);

        let mut set_io = |opcode: u8, effects: u32| {
            let off = 1024 + usize::from(opcode) * 4;
            d[off..off + 4].copy_from_slice(&effects.to_le_bytes());
        };
        set_io(NVM_OP_FLUSH, CMD_EFFECT_CSUPP | CMD_EFFECT_LBCC);
        set_io(NVM_OP_WRITE, CMD_EFFECT_CSUPP | CMD_EFFECT_LBCC);
        set_io(NVM_OP_READ, CMD_EFFECT_CSUPP);
        d
    }

    /// SECURITY SEND. QEMU advertises the opcode, but without an SPDM socket it
    /// rejects every protocol as invalid-field. Keep that shape while the
    /// controller only supports the discovery receive path below.
    fn admin_security_send(&self, _cmd: &SubmissionEntry) -> u16 {
        SC_INVALID_FIELD_DNR
    }

    /// SECURITY RECEIVE. Match QEMU's default no-SPDM behavior: the only
    /// successful request is SECP=0/SPSP=0, which returns the supported security
    /// protocol list. SPDM and certificate paths remain invalid-field.
    fn admin_security_receive(&self, cmd: &SubmissionEntry, mem: &mut dyn GuestMemoryMut) -> u16 {
        let secp = ((cmd.cdw10 >> 24) & 0xff) as u8;
        let spsp = (cmd.cdw10 >> 8) & 0xffff;
        let alloc_len = cmd.cdw11;
        match (secp, spsp) {
            (SECURITY_PROTOCOL_INFORMATION, 0) => {
                if alloc_len < SECURITY_PROTOCOL_INFO_LIST_LEN as u32 {
                    return SC_INVALID_FIELD_DNR;
                }
                let mut resp = [0u8; SECURITY_PROTOCOL_INFO_LIST_LEN];
                // QEMU reports a two-byte supported-protocol list containing
                // Security Protocol Information and a second zero entry when no
                // SPDM socket is configured.
                resp[7] = 2;
                resp[8] = SECURITY_PROTOCOL_INFORMATION;
                resp[9] = 0;
                if mem.write_bytes(cmd.prp1, &resp) {
                    SC_SUCCESS
                } else {
                    SC_INVALID_FIELD
                }
            }
            (SECURITY_PROTOCOL_DMTF_SPDM, _) => SC_INVALID_FIELD_DNR,
            _ => SC_INVALID_FIELD_DNR,
        }
    }

    /// CREATE I/O COMPLETION QUEUE (NVMe 1.4 §5.3). CDW10: QID bits 15:0,
    /// QSIZE bits 31:16 (0-based). CDW11: PC bit 0, IEN bit 1, interrupt
    /// vector bits 31:16. PRP1 is the queue base.
    fn admin_create_io_cq(&mut self, cmd: &SubmissionEntry) -> u16 {
        let qid = (cmd.cdw10 & 0xffff) as usize;
        let qsize_zero_based = ((cmd.cdw10 >> 16) & 0xffff) as u16;
        let interrupt_vector = ((cmd.cdw11 >> CREATE_IO_CQ_IV_SHIFT) & 0xffff) as u16;
        let interrupts_enabled = cmd.cdw11 & CREATE_IO_CQ_IEN_BIT != 0;
        if qid == 0 || qid > usize::from(self.max_io_queues) {
            return SC_INVALID_FIELD; // QID 0 is admin; higher QIDs lack doorbells.
        }
        if qsize_zero_based >= MAX_QUEUE_ENTRIES {
            return SC_INVALID_FIELD;
        }
        let qsize = qsize_zero_based + 1;
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
        let qsize_zero_based = ((cmd.cdw10 >> 16) & 0xffff) as u16;
        let cqid = ((cmd.cdw11 >> 16) & 0xffff) as u16;
        if qid == 0 || qid > usize::from(self.max_io_queues) {
            return SC_INVALID_FIELD;
        }
        if qsize_zero_based >= MAX_QUEUE_ENTRIES {
            return SC_INVALID_FIELD;
        }
        let qsize = qsize_zero_based + 1;
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
        self.clear_sq_pending(qid);
        SC_SUCCESS
    }

    /// SET FEATURES (NVMe 1.4 §5.21). Keep the small set Windows probes aligned
    /// with QEMU defaults; unsupported features remain harmless no-ops here.
    fn admin_set_features(&mut self, cmd: &SubmissionEntry) -> u16 {
        let fid = (cmd.cdw10 & 0xff) as u8;
        match fid {
            FEATURE_NUMBER_OF_QUEUES => {
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
            FEATURE_VOLATILE_WRITE_CACHE => {
                self.volatile_write_cache_enabled = (cmd.cdw11 & 1) != 0;
                if !self.volatile_write_cache_enabled {
                    let _ = self.disk.flush();
                }
            }
            _ => {}
        }
        SC_SUCCESS
    }

    /// GET FEATURES (NVMe 1.4 §5.14). Windows probes several optional features
    /// during setup. Return boring, disabled defaults for the generic features
    /// this tiny controller can safely expose, and report invalid-field (not
    /// invalid-opcode) for reserved/vendor-specific feature IDs.
    fn admin_get_features(&mut self, cmd: &SubmissionEntry, mem: &mut dyn GuestMemoryMut) -> u16 {
        let fid = (cmd.cdw10 & 0xff) as u8;
        let select = (cmd.cdw10 >> GET_FEATURE_SELECT_SHIFT) & 0x7;
        if select == GET_FEATURE_SELECT_CAPABILITIES {
            let Some(capabilities) = feature_capabilities(fid) else {
                return SC_INVALID_FIELD_DNR;
            };
            self.last_feature_result = capabilities;
            return SC_SUCCESS;
        }
        let wants_default = matches!(
            select,
            GET_FEATURE_SELECT_DEFAULT | GET_FEATURE_SELECT_SAVED
        );
        let value = match fid {
            FEATURE_ARBITRATION => 0,
            FEATURE_POWER_MANAGEMENT => 0,
            FEATURE_TEMPERATURE_THRESHOLD => 0,
            FEATURE_ERROR_RECOVERY => 0,
            FEATURE_VOLATILE_WRITE_CACHE => {
                if wants_default {
                    0
                } else {
                    u32::from(self.volatile_write_cache_enabled)
                }
            }
            FEATURE_NUMBER_OF_QUEUES => {
                let granted = u32::from(self.max_io_queues.saturating_sub(1));
                (granted << 16) | granted
            }
            FEATURE_INTERRUPT_COALESCING => 0,
            FEATURE_INTERRUPT_VECTOR_CONFIGURATION => cmd.cdw11 & 0xffff,
            FEATURE_WRITE_ATOMICITY_NORMAL => 0,
            FEATURE_ASYNC_EVENT_CONFIGURATION => 0,
            FEATURE_AUTONOMOUS_POWER_STATE_TRANSITION => {
                if cmd.prp1 != 0 && !mem.write_bytes(cmd.prp1, &ZERO_APST_FEATURE_DATA) {
                    return SC_INVALID_FIELD;
                }
                0
            }
            _ => return SC_INVALID_FIELD_DNR,
        };
        self.last_feature_result = value;
        SC_SUCCESS
    }

    /// Execute an NVM I/O command against the disk backend.
    fn execute_io(&mut self, cmd: &SubmissionEntry, mem: &mut dyn GuestMemoryMut) -> CommandResult {
        let status = match cmd.opcode {
            NVM_OP_FLUSH => self.io_flush(cmd),
            NVM_OP_READ => self.io_read(cmd, mem),
            NVM_OP_WRITE => self.io_write(cmd, mem),
            _ => SC_INVALID_OPCODE,
        };
        CommandResult::complete(status)
    }

    /// NVM FLUSH (0x00). QEMU accepts both NSID 1 and broadcast NSID for a
    /// single-NVM-namespace controller. Memory-backed media is already coherent;
    /// host-file media uses the existing flush hook.
    fn io_flush(&mut self, cmd: &SubmissionEntry) -> u16 {
        // Broadcast NSID flushes every active namespace.
        if cmd.nsid == u32::MAX {
            let mut ok = self.disk.flush().is_ok();
            if let Some(disk2) = self.disk2.as_mut() {
                ok &= disk2.flush().is_ok();
            }
            return if ok { SC_SUCCESS } else { SC_INVALID_FIELD };
        }
        let Some(backend) = self.backend_for_nsid_mut(cmd.nsid) else {
            return SC_INVALID_FIELD;
        };
        match backend.flush() {
            Ok(()) => SC_SUCCESS,
            Err(_) => SC_INVALID_FIELD,
        }
    }

    /// NVM READ (0x02). SLBA in CDW10/11 (64-bit), NLB in CDW12 bits 15:0
    /// (0-based). Data is scattered through PRP1/PRP2 or a PRP list.
    ///
    /// The transfer is decoded once into physically-contiguous guest segments,
    /// then either DMA'd straight from the backing store into guest RAM (when the
    /// memory view exposes stable host pointers) or staged through the reusable
    /// scratch buffer. Both paths issue at most one disk read per contiguous guest
    /// segment — commonly one for the whole command — where the old path did one
    /// allocation + `pread` per 4 KiB PRP page.
    fn io_read(&mut self, cmd: &SubmissionEntry, mem: &mut dyn GuestMemoryMut) -> u16 {
        let Some(byte_len) = self.backend_for_nsid(cmd.nsid).map(DiskBackend::byte_len) else {
            return SC_INVALID_FIELD;
        };
        let Some((start, len)) = transfer_range(cmd, byte_len) else {
            return SC_INVALID_FIELD;
        };
        let mut spans = std::mem::take(&mut self.prp_spans_scratch);
        spans.clear();
        if !prp_spans_into(cmd, len, mem, &mut spans) {
            self.prp_spans_scratch = spans;
            return SC_INVALID_FIELD;
        }
        let mut segments = std::mem::take(&mut self.io_segments_scratch);
        segments.clear();
        coalesce_spans_into(&spans, &mut segments);
        let status = if let Some(status) = self.io_read_direct(cmd.nsid, start, &segments, mem) {
            status
        } else {
            self.io_read_buffered(cmd.nsid, start, len, &segments, mem)
        };
        spans.clear();
        segments.clear();
        self.prp_spans_scratch = spans;
        self.io_segments_scratch = segments;
        status
    }

    /// Zero-copy read fast path: `pread` the backing store straight into guest
    /// RAM through [`GuestMemoryMut::host_ptr`]. Returns `None` (so the caller
    /// falls back to the buffered path) when the memory view exposes no host
    /// pointer, without having touched guest RAM.
    fn io_read_direct(
        &mut self,
        nsid: u32,
        start: u64,
        segments: &[(u64, usize)],
        mem: &mut dyn GuestMemoryMut,
    ) -> Option<u16> {
        let first = segments.first()?;
        // Probe before writing anything: an all-or-nothing host_ptr view either
        // resolves the first span or the whole memory lacks direct pointers.
        mem.host_ptr(first.0, first.1)?;
        let mut disk_off = start;
        for &(gpa, seg_len) in segments {
            let Some(ptr) = mem.host_ptr(gpa, seg_len) else {
                return Some(SC_INVALID_FIELD);
            };
            // SAFETY: host_ptr validated [gpa, gpa+seg_len) lies inside the guest
            // RAM mapping. process() runs under the platform lock, so no vCPU
            // accesses this span concurrently, and segments are processed one at a
            // time so at most one mutable view is live.
            let dst = unsafe { std::slice::from_raw_parts_mut(ptr, seg_len) };
            let Some(backend) = self.backend_for_nsid_mut(nsid) else {
                return Some(SC_INVALID_FIELD);
            };
            if backend.read_at_into(disk_off, dst).is_err() {
                return Some(SC_INVALID_FIELD);
            }
            disk_off += seg_len as u64;
        }
        Some(SC_SUCCESS)
    }

    /// Buffered read path: one coalesced `read_at_into` per contiguous guest
    /// segment through the reusable scratch buffer, then scattered into guest RAM
    /// with `write_bytes`. Used when direct DMA is unavailable (unit tests, any
    /// memory view without `host_ptr`).
    fn io_read_buffered(
        &mut self,
        nsid: u32,
        start: u64,
        len: usize,
        segments: &[(u64, usize)],
        mem: &mut dyn GuestMemoryMut,
    ) -> u16 {
        if len == 0 {
            return SC_SUCCESS;
        }
        let mut scratch = std::mem::take(&mut self.io_scratch);
        if scratch.len() < len {
            scratch.resize(len, 0);
        }
        let status = if let Some(backend) = self.backend_for_nsid_mut(nsid) {
            if backend.read_at_into(start, &mut scratch[..len]).is_err() {
                SC_INVALID_FIELD
            } else {
                let mut off = 0usize;
                let mut ok = true;
                for &(gpa, seg_len) in segments {
                    if !mem.write_bytes(gpa, &scratch[off..off + seg_len]) {
                        ok = false;
                        break;
                    }
                    off += seg_len;
                }
                if ok {
                    SC_SUCCESS
                } else {
                    SC_INVALID_FIELD
                }
            }
        } else {
            SC_INVALID_FIELD
        };
        self.io_scratch = scratch;
        status
    }

    /// NVM WRITE (0x01). Same addressing as READ; copies guest data into disk.
    ///
    /// Synchronous write-back is preserved exactly: in write-through mode each
    /// segment's `write_at` lands in the host file before the command completes.
    fn io_write(&mut self, cmd: &SubmissionEntry, mem: &mut dyn GuestMemoryMut) -> u16 {
        let Some(byte_len) = self.backend_for_nsid(cmd.nsid).map(DiskBackend::byte_len) else {
            return SC_INVALID_FIELD;
        };
        let Some((start, len)) = transfer_range(cmd, byte_len) else {
            return SC_INVALID_FIELD;
        };
        let mut spans = std::mem::take(&mut self.prp_spans_scratch);
        spans.clear();
        if !prp_spans_into(cmd, len, mem, &mut spans) {
            self.prp_spans_scratch = spans;
            return SC_INVALID_FIELD;
        }
        let mut segments = std::mem::take(&mut self.io_segments_scratch);
        segments.clear();
        coalesce_spans_into(&spans, &mut segments);
        let status = if let Some(status) = self.io_write_direct(cmd.nsid, start, &segments, mem) {
            status
        } else {
            self.io_write_buffered(cmd.nsid, start, len, &segments, mem)
        };
        spans.clear();
        segments.clear();
        self.prp_spans_scratch = spans;
        self.io_segments_scratch = segments;
        status
    }

    /// Zero-copy write fast path: `pwrite` guest RAM straight to the backing store
    /// through [`GuestMemoryMut::host_ptr`]. Returns `None` (fall back to buffered)
    /// when no host pointer is available, without having written anything.
    fn io_write_direct(
        &mut self,
        nsid: u32,
        start: u64,
        segments: &[(u64, usize)],
        mem: &mut dyn GuestMemoryMut,
    ) -> Option<u16> {
        let first = segments.first()?;
        mem.host_ptr(first.0, first.1)?;
        let mut disk_off = start;
        for &(gpa, seg_len) in segments {
            let Some(ptr) = mem.host_ptr(gpa, seg_len) else {
                return Some(SC_INVALID_FIELD);
            };
            // SAFETY: host_ptr validated [gpa, gpa+seg_len) lies inside the guest
            // RAM mapping; this is a read-only view for the disk write, and
            // process() holds the platform lock so the span is not mutated
            // concurrently.
            let src = unsafe { std::slice::from_raw_parts(ptr, seg_len) };
            let Some(backend) = self.backend_for_nsid_mut(nsid) else {
                return Some(SC_INVALID_FIELD);
            };
            if backend.write_at(disk_off, src).is_err() {
                return Some(SC_INVALID_FIELD);
            }
            disk_off += seg_len as u64;
        }
        Some(SC_SUCCESS)
    }

    /// Buffered write path: gather the contiguous guest segments into the reusable
    /// scratch buffer with `read_into`, then one `write_at` for the whole
    /// contiguous disk range. Used when direct DMA is unavailable.
    fn io_write_buffered(
        &mut self,
        nsid: u32,
        start: u64,
        len: usize,
        segments: &[(u64, usize)],
        mem: &mut dyn GuestMemoryMut,
    ) -> u16 {
        if len == 0 {
            return SC_SUCCESS;
        }
        let mut scratch = std::mem::take(&mut self.io_scratch);
        if scratch.len() < len {
            scratch.resize(len, 0);
        }
        let mut off = 0usize;
        let mut gathered = true;
        for &(gpa, seg_len) in segments {
            if !mem.read_into(gpa, &mut scratch[off..off + seg_len]) {
                gathered = false;
                break;
            }
            off += seg_len;
        }
        let status = if !gathered {
            SC_INVALID_FIELD
        } else if let Some(backend) = self.backend_for_nsid_mut(nsid) {
            if backend.write_at(start, &scratch[..len]).is_ok() {
                SC_SUCCESS
            } else {
                SC_INVALID_FIELD
            }
        } else {
            SC_INVALID_FIELD
        };
        self.io_scratch = scratch;
        status
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
    ) -> (bool, Option<NvmeCompletionEvent>) {
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
                _ => return (false, None),
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
            return (false, None);
        }

        // Advance the CQ tail, toggling the phase tag when it wraps.
        let new_tail = (tail + 1) % size;
        if let Some(Some(cq)) = self.cqs.get_mut(cqid as usize) {
            cq.tail = new_tail;
            if new_tail == 0 {
                cq.phase = !cq.phase;
            }
        }
        (
            true,
            interrupts_enabled.then_some(NvmeCompletionEvent {
                cqid,
                vector: interrupt_vector,
            }),
        )
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

    pub fn drain_pending_msix_into(
        &mut self,
        function_enabled: bool,
        function_masked: bool,
        out: &mut Vec<MsixMessage>,
    ) {
        self.msix
            .drain_pending_into(function_enabled, function_masked, out);
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
fn prp_spans_into(
    cmd: &SubmissionEntry,
    len: usize,
    mem: &dyn GuestMemoryMut,
    out: &mut Vec<(u64, usize)>,
) -> bool {
    let start = out.len();
    if len == 0 {
        return true;
    }
    if cmd.prp1 == 0 {
        return false;
    }

    let mut remaining = len;
    let first_page_left = (PAGE_SIZE_U64 - (cmd.prp1 % PAGE_SIZE_U64)) as usize;
    let first_len = remaining.min(first_page_left);
    out.push((cmd.prp1, first_len));
    remaining -= first_len;

    if remaining == 0 {
        return true;
    }
    if cmd.prp2 == 0 {
        out.truncate(start);
        return false;
    }

    if remaining <= PAGE_SIZE {
        if cmd.prp2 % PAGE_SIZE_U64 != 0 {
            out.truncate(start);
            return false;
        }
        out.push((cmd.prp2, remaining));
        return true;
    }

    let mut list_gpa = cmd.prp2;
    let mut list_pages_seen = 0usize;

    while remaining > 0 {
        let list_offset = (list_gpa % PAGE_SIZE_U64) as usize;
        if list_offset % 8 != 0 {
            out.truncate(start);
            return false;
        }
        list_pages_seen += 1;
        if list_pages_seen > 16 {
            out.truncate(start);
            return false;
        }

        let list_len = PAGE_SIZE - list_offset;
        let mut list_buf = [0u8; PAGE_SIZE];
        if !mem.read_into(list_gpa, &mut list_buf[..list_len]) {
            out.truncate(start);
            return false;
        }
        let raw = &list_buf[..list_len];
        let mut followed_chain = false;
        let entries_in_page = raw.len() / 8;
        for (idx, chunk) in raw.chunks_exact(8).enumerate() {
            let entry = u64::from_le_bytes(chunk.try_into().unwrap());
            if entry == 0 {
                out.truncate(start);
                return false;
            }

            if idx == entries_in_page - 1 && remaining > PAGE_SIZE {
                if entry % PAGE_SIZE_U64 != 0 {
                    out.truncate(start);
                    return false;
                }
                list_gpa = entry;
                followed_chain = true;
                break;
            }

            if entry % PAGE_SIZE_U64 != 0 {
                out.truncate(start);
                return false;
            }
            let span_len = remaining.min(PAGE_SIZE);
            out.push((entry, span_len));
            remaining -= span_len;
            if remaining == 0 {
                return true;
            }
        }

        if !followed_chain {
            out.truncate(start);
            return false;
        }
    }

    true
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
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        matches!(
            std::env::var("BRIDGEVM_TRACE_NVME").ok().as_deref(),
            Some("1") | Some("true") | Some("TRUE") | Some("yes") | Some("YES")
        )
    })
}

fn identify_cns_name(cns: u32) -> &'static str {
    match cns {
        IDENTIFY_CNS_NAMESPACE => "namespace",
        IDENTIFY_CNS_CONTROLLER => "controller",
        IDENTIFY_CNS_ACTIVE_NAMESPACE_LIST => "active-ns-list",
        IDENTIFY_CNS_NAMESPACE_DESCRIPTOR_LIST => "ns-desc-list",
        IDENTIFY_CNS_COMMAND_SET_CONTROLLER => "command-set-controller",
        _ => "unknown",
    }
}

fn feature_capabilities(fid: u8) -> Option<u32> {
    match fid {
        FEATURE_TEMPERATURE_THRESHOLD
        | FEATURE_VOLATILE_WRITE_CACHE
        | FEATURE_NUMBER_OF_QUEUES
        | FEATURE_WRITE_ATOMICITY_NORMAL
        | FEATURE_ASYNC_EVENT_CONFIGURATION => Some(FEATURE_CAP_CHANGEABLE),
        FEATURE_ERROR_RECOVERY => Some(FEATURE_CAP_CHANGEABLE | FEATURE_CAP_NAMESPACE_SPECIFIC),
        FEATURE_ARBITRATION
        | FEATURE_POWER_MANAGEMENT
        | FEATURE_INTERRUPT_COALESCING
        | FEATURE_INTERRUPT_VECTOR_CONFIGURATION
        | FEATURE_AUTONOMOUS_POWER_STATE_TRANSITION => Some(0),
        _ => None,
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

/// Decode (SLBA, NLB) into a byte range, validating it fits a `byte_len`-sized
/// namespace. Returns `(start_byte, len_bytes)`.
fn transfer_range(cmd: &SubmissionEntry, byte_len: u64) -> Option<(u64, usize)> {
    let slba = u64::from(cmd.cdw10) | (u64::from(cmd.cdw11) << 32);
    let nlb = u64::from(cmd.cdw12 & 0xffff) + 1; // 0-based count
    let len = nlb.checked_mul(LBA_SIZE as u64)?;
    let start = slba.checked_mul(LBA_SIZE as u64)?;
    if start.checked_add(len)? > byte_len {
        return None; // out of range
    }
    if len > usize::MAX as u64 {
        return None;
    }
    Some((start, len as usize))
}

/// Merge physically-contiguous PRP spans into larger segments. The spans arrive
/// in transfer order and the disk range is contiguous, so any run whose guest
/// addresses abut collapses into one segment — turning a 128 KiB scatter of
/// thirty-two 4 KiB pages into a single segment when the guest allocated the
/// buffer contiguously. `checked_add` keeps a guest-controlled address near the
/// top of the space from overflowing (and, under overflow-checks, panicking).
#[cfg(test)]
fn coalesce_spans(spans: &[(u64, usize)]) -> Vec<(u64, usize)> {
    let mut out = Vec::with_capacity(spans.len());
    coalesce_spans_into(spans, &mut out);
    out
}

fn coalesce_spans_into(spans: &[(u64, usize)], out: &mut Vec<(u64, usize)>) {
    out.reserve(spans.len());
    for &(gpa, len) in spans {
        if len == 0 {
            continue;
        }
        if let Some(last) = out.last_mut() {
            if last.0.checked_add(last.1 as u64) == Some(gpa) {
                // Segment length is bounded by the transfer length, which
                // transfer_range already validated fits the namespace.
                last.1 += len;
                continue;
            }
        }
        out.push((gpa, len));
    }
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
        /// When set, [`GuestMemoryMut::host_ptr`] resolves spans to real pointers
        /// into `bytes`, so the NVMe data path takes its zero-copy direct-DMA
        /// branch instead of the reusable-scratch fallback. Off by default so the
        /// existing suite keeps covering the buffered path.
        expose_host_ptr: bool,
    }

    impl FakeMem {
        fn new(base: u64, len: usize) -> Self {
            Self {
                base,
                bytes: vec![0u8; len],
                expose_host_ptr: false,
            }
        }
        fn at(&self, gpa: u64) -> usize {
            (gpa - self.base) as usize
        }
        /// Expose stable host pointers so IO takes the direct-DMA branch.
        fn enable_host_ptr(&mut self) {
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

    fn completion_dw0(entry: &[u8; 16]) -> u32 {
        u32::from_le_bytes([entry[0], entry[1], entry[2], entry[3]])
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
    fn cap_advertises_configured_mqes_and_zero_dstrd() {
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
    fn second_namespace_raw_file_overlay_exports_snapshot() {
        let source = temp_path("raw-nsid2-overlay-source");
        let snapshot = temp_path("raw-nsid2-overlay-snapshot");
        let slba = 2u64;
        let start = slba as usize * LBA_SIZE;
        let original: Vec<u8> = (0..LBA_SIZE).map(|i| 0x10 | (i % 0x10) as u8).collect();
        let replacement: Vec<u8> = (0..LBA_SIZE).map(|i| 0x90 | (i % 0x30) as u8).collect();
        let mut disk = vec![0u8; LBA_SIZE * 8];
        disk[start..start + LBA_SIZE].copy_from_slice(&original);
        fs::write(&source, &disk).unwrap();

        let (mut ctrl, mut mem) = enabled_controller_with_mem_len(0x10000);
        ctrl.attach_second_namespace_raw_file(&source, false)
            .unwrap();
        create_io_queue_pair(&mut ctrl, &mut mem, 0, CREATE_IO_CQ_PC_BIT);

        assert!(mem.write_bytes(DATA_BASE, &replacement));
        let write = encode_sqe(
            NVM_OP_WRITE,
            0x72,
            NSID2,
            DATA_BASE,
            slba as u32,
            (slba >> 32) as u32,
            0,
        );
        assert!(mem.write_bytes(IO_SQ_BASE, &write));
        ctrl.mmio_write(REG_DOORBELL_BASE + 2 * 4, 4, 1);
        ctrl.process(&mut mem);

        assert_eq!(
            &fs::read(&source).unwrap()[start..start + LBA_SIZE],
            original.as_slice(),
            "read-only NSID2 raw file keeps guest writes in the overlay"
        );
        assert_eq!(
            ctrl.export_second_namespace_disk_image(&snapshot).unwrap(),
            disk.len() as u64
        );
        assert_eq!(
            &fs::read(&snapshot).unwrap()[start..start + LBA_SIZE],
            replacement.as_slice(),
            "NSID2 snapshot export applies overlay writes"
        );

        fs::remove_file(source).ok();
        fs::remove_file(snapshot).ok();
    }

    #[test]
    fn second_namespace_raw_file_write_back_updates_source_file() {
        let source = temp_path("raw-nsid2-writeback-source");
        let slba = 4u64;
        let start = slba as usize * LBA_SIZE;
        fs::write(&source, vec![0u8; LBA_SIZE * 8]).unwrap();

        let (mut ctrl, mut mem) = enabled_controller_with_mem_len(0x10000);
        ctrl.attach_second_namespace_raw_file(&source, true)
            .unwrap();
        create_io_queue_pair(&mut ctrl, &mut mem, 0, CREATE_IO_CQ_PC_BIT);

        let replacement: Vec<u8> = (0..LBA_SIZE).map(|i| 0x50 | (i % 0x20) as u8).collect();
        assert!(mem.write_bytes(DATA_BASE, &replacement));
        let write = encode_sqe(
            NVM_OP_WRITE,
            0x73,
            NSID2,
            DATA_BASE,
            slba as u32,
            (slba >> 32) as u32,
            0,
        );
        assert!(mem.write_bytes(IO_SQ_BASE, &write));
        ctrl.mmio_write(REG_DOORBELL_BASE + 2 * 4, 4, 1);
        ctrl.process(&mut mem);
        ctrl.flush_second_namespace_disk().unwrap();

        assert_eq!(
            &fs::read(&source).unwrap()[start..start + LBA_SIZE],
            replacement.as_slice(),
            "NSID2 write-back raw file persists guest writes to the source"
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
        let oacs = u16::from_le_bytes([id[256], id[257]]);
        assert_eq!(
            oacs & 1,
            1,
            "OACS advertises Security Send/Receive like QEMU's default NVMe"
        );
        let nn = u32::from_le_bytes([id[516], id[517], id[518], id[519]]);
        assert_eq!(nn, 1, "one namespace");
        assert_eq!(
            id[259],
            MAX_ASYNC_EVENT_REQUESTS - 1,
            "AERL advertises the retained async-event request slots"
        );
        assert_eq!(id[512], 0x66, "SQES = 64-byte entries");
        assert_eq!(id[513], 0x44, "CQES = 16-byte entries");
        assert_eq!(
            id[525], VWC_QEMU_DEFAULT,
            "VWC advertises QEMU's present cache plus broadcast-NSID flush support"
        );
        assert!(
            id[768..1024].starts_with(b"nqn.2026-06.dev.bridgevm:bridgevm-hvf:nvme0\0"),
            "SUBNQN must be present and NUL-terminated for Linux"
        );
    }

    #[test]
    fn identify_command_set_controller_completes_for_nvm_csi() {
        let (mut ctrl, mut mem) = enabled_controller();
        let sqe = encode_sqe(
            ADMIN_OP_IDENTIFY,
            0x57,
            0xffff_ffff,
            DATA_BASE,
            IDENTIFY_CNS_COMMAND_SET_CONTROLLER,
            u32::from(COMMAND_SET_NVM) << 24,
            0,
        );
        submit_admin(&mut ctrl, &mut mem, 0, &sqe);
        assert_eq!(
            completion_status(&read_completion(&mem, ACQ_BASE, 0)),
            SC_SUCCESS
        );

        let id = mem.read_bytes(DATA_BASE, PAGE_SIZE).unwrap();
        assert_eq!(id, vec![0u8; PAGE_SIZE]);
    }

    #[test]
    fn identify_command_set_controller_rejects_unknown_csi() {
        let (mut ctrl, mut mem) = enabled_controller();
        let sqe = encode_sqe(
            ADMIN_OP_IDENTIFY,
            0x58,
            0xffff_ffff,
            DATA_BASE,
            IDENTIFY_CNS_COMMAND_SET_CONTROLLER,
            0xff << 24,
            0,
        );
        submit_admin(&mut ctrl, &mut mem, 0, &sqe);
        assert_eq!(
            completion_status(&read_completion(&mem, ACQ_BASE, 0)),
            SC_INVALID_FIELD
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
        let trace = ctrl.recent_command_trace();
        assert_eq!(trace.len(), 1);
        assert_eq!(trace[0].sqid, 0);
        assert_eq!(trace[0].cqid, 0);
        assert_eq!(trace[0].sq_head, 0);
        assert_eq!(trace[0].sq_tail, 1);
        assert_eq!(trace[0].sq_entry_gpa, ASQ_BASE);
        assert_eq!(trace[0].opcode, ADMIN_OP_IDENTIFY);
        assert_eq!(trace[0].command_id, 0x56);
        assert_eq!(trace[0].prp1, DATA_BASE);
        assert_eq!(trace[0].cdw10, IDENTIFY_CNS_CONTROLLER);
        assert_eq!(trace[0].status, SC_SUCCESS);
        assert!(trace[0].completion_posted);
        assert_eq!(
            trace[0].completion,
            Some(NvmeCompletionTrace { cqid: 0, vector: 0 })
        );
    }

    #[test]
    fn process_into_reuses_caller_completion_storage() {
        let (mut ctrl, mut mem) = enabled_controller();
        let sqe = encode_sqe(
            ADMIN_OP_IDENTIFY,
            0x57,
            0,
            DATA_BASE,
            IDENTIFY_CNS_CONTROLLER,
            0,
            0,
        );
        assert!(mem.write_bytes(ASQ_BASE, &sqe));
        ctrl.mmio_write(REG_DOORBELL_BASE, 4, 1);

        let mut completions = Vec::with_capacity(4);
        let completion_capacity = completions.capacity();
        let completion_ptr = completions.as_ptr();
        ctrl.process_into(&mut mem, &mut completions);

        assert_eq!(
            completions,
            vec![NvmeCompletionEvent { cqid: 0, vector: 0 }]
        );
        assert_eq!(completions.capacity(), completion_capacity);
        assert_eq!(completions.as_ptr(), completion_ptr);

        completions.clear();
        ctrl.process_into(&mut mem, &mut completions);
        assert!(completions.is_empty());
        assert_eq!(completions.capacity(), completion_capacity);
        assert_eq!(completions.as_ptr(), completion_ptr);
    }

    #[test]
    fn process_into_drains_only_pending_doorbelled_submission_queue() {
        let qid = MAX_IO_QUEUE_PAIRS;
        let high_io_cq = 0x4000_8000;
        let high_io_sq = 0x4000_9000;
        let (mut ctrl, mut mem) = enabled_controller_with_mem_len(0x20000);
        let cdw10 = (u32::from(QDEPTH - 1) << 16) | u32::from(qid);
        let cq_cdw11 = CREATE_IO_CQ_PC_BIT | CREATE_IO_CQ_IEN_BIT | (1u32 << CREATE_IO_CQ_IV_SHIFT);

        submit_admin(
            &mut ctrl,
            &mut mem,
            0,
            &encode_sqe(ADMIN_OP_CREATE_IO_CQ, 1, 0, high_io_cq, cdw10, cq_cdw11, 0),
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
                high_io_sq,
                cdw10,
                u32::from(qid) << 16,
                0,
            ),
        );
        assert_eq!(
            completion_status(&read_completion(&mem, ACQ_BASE, 1)),
            SC_SUCCESS
        );

        let read_cmd = encode_sqe(NVM_OP_READ, 0x70, NSID, DATA_BASE, 0, 0, 0);
        assert!(mem.write_bytes(high_io_sq, &read_cmd));
        let mut completions = Vec::new();
        ctrl.process_into(&mut mem, &mut completions);
        assert!(
            completions.is_empty(),
            "no SQ doorbell means no pending work"
        );

        ctrl.mmio_write(REG_DOORBELL_BASE + u64::from(qid) * 8, 4, 1);
        ctrl.process_into(&mut mem, &mut completions);

        assert_eq!(
            completions,
            vec![NvmeCompletionEvent {
                cqid: qid,
                vector: 1,
            }]
        );
        assert_eq!(
            completion_status(&read_completion(&mem, high_io_cq, 0)),
            SC_SUCCESS
        );

        completions.clear();
        ctrl.process_into(&mut mem, &mut completions);
        assert!(completions.is_empty(), "drained SQ bit is cleared");
    }

    #[test]
    fn second_namespace_is_listed_sized_and_bounds_checked() {
        let (mut ctrl, mut mem) = enabled_controller();
        // 2 MiB blank install target as NSID 2.
        let target_bytes = 2 * 1024 * 1024usize;
        ctrl.attach_second_namespace(target_bytes);
        assert!(ctrl.has_second_namespace());

        // Active namespace list (after NSID 0) reports NSID 1 then NSID 2 then 0.
        let sqe = encode_sqe(
            ADMIN_OP_IDENTIFY,
            0x10,
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
        assert_eq!(u32::from_le_bytes(list[4..8].try_into().unwrap()), NSID2);
        assert_eq!(u32::from_le_bytes(list[8..12].try_into().unwrap()), 0);

        // Identify Namespace for NSID 2 reports the target's block count.
        let sqe = encode_sqe(
            ADMIN_OP_IDENTIFY,
            0x11,
            NSID2,
            DATA_BASE,
            IDENTIFY_CNS_NAMESPACE,
            0,
            0,
        );
        submit_admin(&mut ctrl, &mut mem, 1, &sqe);
        assert_eq!(
            completion_status(&read_completion(&mem, ACQ_BASE, 1)),
            SC_SUCCESS
        );
        let ns = mem.read_bytes(DATA_BASE, PAGE_SIZE).unwrap();
        let nsze = u64::from_le_bytes(ns[0..8].try_into().unwrap());
        assert_eq!(nsze, (target_bytes / LBA_SIZE) as u64);

        // Transfer bounds are enforced per namespace: reading the first LBA past
        // the small 1 MiB NSID-1 disk fails, but that LBA is valid on the 2 MiB
        // NSID 2. `block_count_for` reflects each namespace's own size.
        assert_eq!(ctrl.block_count_for(NSID), (1 << 20) / LBA_SIZE as u64);
        assert_eq!(
            ctrl.block_count_for(NSID2),
            (target_bytes / LBA_SIZE) as u64
        );
        assert_eq!(ctrl.block_count_for(3), 0, "unallocated namespace");
        let over_ns1_lba = (1 << 20) / LBA_SIZE as u32; // first LBA past NSID 1
        let read = SubmissionEntry {
            opcode: NVM_OP_READ,
            command_id: 0,
            nsid: NSID,
            prp1: 0,
            prp2: 0,
            cdw10: over_ns1_lba,
            cdw11: 0,
            cdw12: 0, // NLB 0-based => 1 block
            cdw13: 0,
            cdw14: 0,
            cdw15: 0,
        };
        assert!(transfer_range(&read, ctrl.block_count_for(NSID) * LBA_SIZE as u64).is_none());
        assert!(transfer_range(&read, ctrl.block_count_for(NSID2) * LBA_SIZE as u64).is_some());
    }

    #[test]
    fn async_event_request_is_accepted_and_left_pending() {
        let (mut ctrl, mut mem) = enabled_controller();
        let sqe = encode_sqe(ADMIN_OP_ASYNC_EVENT_REQUEST, 0x77, 0, 0, 0, 0, 0);
        assert!(mem.write_bytes(ASQ_BASE, &sqe));
        ctrl.mmio_write(REG_DOORBELL_BASE, 4, 1);

        assert_eq!(ctrl.process(&mut mem), Vec::<NvmeCompletionEvent>::new());
        let admin_sq = ctrl.sqs[0].as_ref().expect("admin SQ installed");
        assert_eq!(admin_sq.head, 1, "AER consumes an SQ entry");
        assert_eq!(read_completion(&mem, ACQ_BASE, 0), [0u8; 16]);
        assert_eq!(ctrl.pending_async_event_requests, 1);

        let trace = ctrl.recent_command_trace();
        assert_eq!(trace.len(), 1);
        assert_eq!(trace[0].opcode, ADMIN_OP_ASYNC_EVENT_REQUEST);
        assert_eq!(trace[0].command_id, 0x77);
        assert_eq!(trace[0].status, SC_SUCCESS);
        assert!(!trace[0].completion_posted);
        assert_eq!(trace[0].completion, None);
    }

    #[test]
    fn nvme_reset_preserving_media_clears_controller_state() {
        // Given: both namespaces carry guest-written data and volatile controller
        // state is dirty.
        let (mut ctrl, mut mem) = enabled_controller();
        ctrl.attach_second_namespace(LBA_SIZE * 8);
        create_io_queue_pair(&mut ctrl, &mut mem, 0, CREATE_IO_CQ_PC_BIT);

        let ns1_pattern: Vec<u8> = (0..LBA_SIZE).map(|i| 0x40 | (i % 0x20) as u8).collect();
        assert!(mem.write_bytes(DATA_BASE, &ns1_pattern));
        let ns1_write = encode_sqe(NVM_OP_WRITE, 0x41, NSID, DATA_BASE, 3, 0, 0);
        assert!(mem.write_bytes(IO_SQ_BASE, &ns1_write));
        ctrl.mmio_write(REG_DOORBELL_BASE + 2 * 4, 4, 1);
        ctrl.process(&mut mem);
        assert_eq!(
            completion_status(&read_completion(&mem, IO_CQ_BASE, 0)),
            SC_SUCCESS
        );

        let ns2_pattern: Vec<u8> = (0..LBA_SIZE).map(|i| 0x80 | (i % 0x40) as u8).collect();
        ctrl.backend_for_nsid_mut(NSID2)
            .expect("NSID 2 attached")
            .write_at(0, &ns2_pattern)
            .unwrap();

        let aer = encode_sqe(ADMIN_OP_ASYNC_EVENT_REQUEST, 0x42, 0, 0, 0, 0, 0);
        assert!(mem.write_bytes(ASQ_BASE + 2 * SQ_ENTRY_SIZE, &aer));
        ctrl.mmio_write(REG_DOORBELL_BASE, 4, 3);
        ctrl.process(&mut mem);
        assert_eq!(ctrl.pending_async_event_requests, 1);
        ctrl.mmio_write(NVME_MSIX_TABLE_OFFSET.into(), 8, 0x0808_0000);
        ctrl.mmio_write(u64::from(NVME_MSIX_TABLE_OFFSET) + 8, 4, 35);
        assert_eq!(ctrl.raise_msix(0, true, false), None);
        assert!(!ctrl.recent_command_trace().is_empty());

        // When: the platform reboot path resets controller registers without
        // replacing namespace backing stores.
        ctrl.reset_registers_keep_disks();

        // Then: namespace contents survive but controller-visible volatile state
        // returns to power-on defaults.
        let ns1_start = 3 * LBA_SIZE;
        assert_eq!(
            &ctrl.disk_image()[ns1_start..ns1_start + LBA_SIZE],
            ns1_pattern.as_slice()
        );
        assert_eq!(
            ctrl.backend_for_nsid_mut(NSID2)
                .expect("NSID 2 attached")
                .read_at(0, LBA_SIZE)
                .unwrap(),
            ns2_pattern
        );
        assert_eq!(ctrl.mmio_read(REG_CC, 4), 0);
        assert_eq!(ctrl.mmio_read(REG_CSTS, 4) & u64::from(CSTS_RDY_BIT), 0);
        assert_eq!(ctrl.mmio_read(REG_AQA, 4), 0);
        assert_eq!(ctrl.mmio_read(REG_ASQ, 8), 0);
        assert_eq!(ctrl.mmio_read(REG_ACQ, 8), 0);
        assert_eq!(ctrl.sqs.len(), 1);
        assert!(ctrl.sqs[0].is_none());
        assert_eq!(ctrl.cqs.len(), 1);
        assert!(ctrl.cqs[0].is_none());
        assert_eq!(ctrl.pending_async_event_requests, 0);
        assert!(ctrl.recent_command_trace().is_empty());
        assert_eq!(
            ctrl.drain_pending_msix(true, false),
            Vec::<MsixMessage>::new()
        );
        assert!(ctrl.volatile_write_cache_enabled);
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
    fn get_log_page_firmware_slot_info_completes() {
        let (mut ctrl, mut mem) = enabled_controller();
        let numd = (512u32 / 4) - 1;
        let cdw10 = (numd << 16) | u32::from(LOG_PAGE_FIRMWARE_SLOT_INFO);
        let sqe = encode_sqe(
            ADMIN_OP_GET_LOG_PAGE,
            6,
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

        let log = mem.read_bytes(DATA_BASE, 512).unwrap();
        assert_eq!(log[0] & 0x7, 1, "active firmware slot is slot 1");
        assert!(
            log[8..72].starts_with(b"BridgeVM NVMe firmware slot 1"),
            "firmware revision slot string is present"
        );
    }

    #[test]
    fn get_log_page_command_effects_completes_with_supported_commands() {
        let (mut ctrl, mut mem) = enabled_controller();
        let numd = (PAGE_SIZE as u32 / 4) - 1;
        let cdw10 = (numd << 16) | u32::from(LOG_PAGE_COMMAND_EFFECTS);
        let sqe = encode_sqe(
            ADMIN_OP_GET_LOG_PAGE,
            7,
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

        let log = mem.read_bytes(DATA_BASE, PAGE_SIZE).unwrap();
        let effect_at = |base: usize, opcode: u8| {
            let off = base + usize::from(opcode) * 4;
            u32::from_le_bytes(log[off..off + 4].try_into().unwrap())
        };
        assert_eq!(effect_at(0, ADMIN_OP_GET_LOG_PAGE), CMD_EFFECT_CSUPP);
        assert_eq!(effect_at(0, ADMIN_OP_IDENTIFY), CMD_EFFECT_CSUPP);
        assert_eq!(effect_at(0, ADMIN_OP_GET_FEATURES), CMD_EFFECT_CSUPP);
        assert_eq!(effect_at(0, ADMIN_OP_SECURITY_SEND), CMD_EFFECT_CSUPP);
        assert_eq!(effect_at(0, ADMIN_OP_SECURITY_RECV), CMD_EFFECT_CSUPP);
        assert_eq!(
            effect_at(1024, NVM_OP_FLUSH),
            CMD_EFFECT_CSUPP | CMD_EFFECT_LBCC
        );
        assert_eq!(
            effect_at(1024, NVM_OP_WRITE),
            CMD_EFFECT_CSUPP | CMD_EFFECT_LBCC
        );
        assert_eq!(effect_at(1024, NVM_OP_READ), CMD_EFFECT_CSUPP);
    }

    #[test]
    fn get_log_page_vendor_logs_match_qemu_invalid_field_dnr() {
        let (mut ctrl, mut mem) = enabled_controller();
        let numd = (512u32 / 4) - 1;
        for (slot, lid) in [0xc0u8, 0xc1].into_iter().enumerate() {
            let sqe = encode_sqe(
                ADMIN_OP_GET_LOG_PAGE,
                0x80 + slot as u16,
                0xffff_ffff,
                DATA_BASE + slot as u64 * PAGE_SIZE_U64,
                (numd << 16) | u32::from(lid),
                0,
                0,
            );
            submit_admin(&mut ctrl, &mut mem, slot as u16, &sqe);
            assert_eq!(
                completion_status(&read_completion(&mem, ACQ_BASE, slot as u16)),
                SC_INVALID_FIELD_DNR,
                "vendor log page {lid:#x} matches QEMU's unsupported default with DNR"
            );
        }
    }

    #[test]
    fn security_receive_protocol_info_matches_qemu_no_spdm_default() {
        let (mut ctrl, mut mem) = enabled_controller();
        let cdw10 = u32::from(SECURITY_PROTOCOL_INFORMATION) << 24;
        let sqe = encode_sqe(
            ADMIN_OP_SECURITY_RECV,
            0x90,
            0,
            DATA_BASE,
            cdw10,
            SECURITY_PROTOCOL_INFO_LIST_LEN as u32,
            0,
        );
        submit_admin(&mut ctrl, &mut mem, 0, &sqe);
        assert_eq!(
            completion_status(&read_completion(&mem, ACQ_BASE, 0)),
            SC_SUCCESS
        );

        assert_eq!(
            mem.read_bytes(DATA_BASE, SECURITY_PROTOCOL_INFO_LIST_LEN)
                .unwrap(),
            vec![0, 0, 0, 0, 0, 0, 0, 2, SECURITY_PROTOCOL_INFORMATION, 0,]
        );
    }

    #[test]
    fn security_receive_rejects_short_or_unsupported_requests() {
        let (mut ctrl, mut mem) = enabled_controller();
        let cases = [
            (
                (u32::from(SECURITY_PROTOCOL_INFORMATION) << 24),
                (SECURITY_PROTOCOL_INFO_LIST_LEN - 1) as u32,
            ),
            (
                (u32::from(SECURITY_PROTOCOL_INFORMATION) << 24) | (1 << 8),
                SECURITY_PROTOCOL_INFO_LIST_LEN as u32,
            ),
            (
                u32::from(SECURITY_PROTOCOL_DMTF_SPDM) << 24,
                SECURITY_PROTOCOL_INFO_LIST_LEN as u32,
            ),
        ];
        for (slot, (cdw10, cdw11)) in cases.into_iter().enumerate() {
            let sqe = encode_sqe(
                ADMIN_OP_SECURITY_RECV,
                0x91 + slot as u16,
                0,
                DATA_BASE,
                cdw10,
                cdw11,
                0,
            );
            submit_admin(&mut ctrl, &mut mem, slot as u16, &sqe);
            assert_eq!(
                completion_status(&read_completion(&mem, ACQ_BASE, slot as u16)),
                SC_INVALID_FIELD_DNR
            );
        }
    }

    #[test]
    fn security_send_reports_invalid_field_without_spdm_socket() {
        let (mut ctrl, mut mem) = enabled_controller();
        let sqe = encode_sqe(
            ADMIN_OP_SECURITY_SEND,
            0x94,
            0,
            DATA_BASE,
            u32::from(SECURITY_PROTOCOL_DMTF_SPDM) << 24,
            0,
            0,
        );
        submit_admin(&mut ctrl, &mut mem, 0, &sqe);
        assert_eq!(
            completion_status(&read_completion(&mem, ACQ_BASE, 0)),
            SC_INVALID_FIELD_DNR
        );
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
    fn get_features_number_of_queues_reports_capacity_in_completion_dw0() {
        let (mut ctrl, mut mem) = enabled_controller();
        let sqe = encode_sqe(
            ADMIN_OP_GET_FEATURES,
            8,
            0,
            0,
            u32::from(FEATURE_NUMBER_OF_QUEUES),
            0,
            0,
        );
        submit_admin(&mut ctrl, &mut mem, 0, &sqe);
        let cqe = read_completion(&mem, ACQ_BASE, 0);
        assert_eq!(completion_status(&cqe), SC_SUCCESS);
        let granted = u32::from(MAX_IO_QUEUE_PAIRS - 1);
        assert_eq!(completion_dw0(&cqe), (granted << 16) | granted);
    }

    #[test]
    fn get_features_volatile_write_cache_matches_qemu_default() {
        let (mut ctrl, mut mem) = enabled_controller();
        let current = encode_sqe(
            ADMIN_OP_GET_FEATURES,
            0x70,
            0,
            0,
            u32::from(FEATURE_VOLATILE_WRITE_CACHE),
            0,
            0,
        );
        submit_admin(&mut ctrl, &mut mem, 0, &current);
        let cqe = read_completion(&mem, ACQ_BASE, 0);
        assert_eq!(completion_status(&cqe), SC_SUCCESS);
        assert_eq!(
            completion_dw0(&cqe),
            1,
            "QEMU reports volatile write cache enabled by default"
        );

        let caps = encode_sqe(
            ADMIN_OP_GET_FEATURES,
            0x71,
            0,
            0,
            u32::from(FEATURE_VOLATILE_WRITE_CACHE)
                | (GET_FEATURE_SELECT_CAPABILITIES << GET_FEATURE_SELECT_SHIFT),
            0,
            0,
        );
        submit_admin(&mut ctrl, &mut mem, 1, &caps);
        let cqe = read_completion(&mem, ACQ_BASE, 1);
        assert_eq!(completion_status(&cqe), SC_SUCCESS);
        assert_eq!(
            completion_dw0(&cqe),
            FEATURE_CAP_CHANGEABLE,
            "QEMU reports VWC as a changeable feature"
        );

        let default = encode_sqe(
            ADMIN_OP_GET_FEATURES,
            0x72,
            0,
            0,
            u32::from(FEATURE_VOLATILE_WRITE_CACHE)
                | (GET_FEATURE_SELECT_DEFAULT << GET_FEATURE_SELECT_SHIFT),
            0,
            0,
        );
        submit_admin(&mut ctrl, &mut mem, 2, &default);
        let cqe = read_completion(&mem, ACQ_BASE, 2);
        assert_eq!(completion_status(&cqe), SC_SUCCESS);
        assert_eq!(
            completion_dw0(&cqe),
            0,
            "QEMU reports VWC default as disabled even when current is enabled"
        );

        let saved = encode_sqe(
            ADMIN_OP_GET_FEATURES,
            0x73,
            0,
            0,
            u32::from(FEATURE_VOLATILE_WRITE_CACHE)
                | (GET_FEATURE_SELECT_SAVED << GET_FEATURE_SELECT_SHIFT),
            0,
            0,
        );
        submit_admin(&mut ctrl, &mut mem, 3, &saved);
        let cqe = read_completion(&mem, ACQ_BASE, 3);
        assert_eq!(completion_status(&cqe), SC_SUCCESS);
        assert_eq!(
            completion_dw0(&cqe),
            0,
            "QEMU falls saved VWC back to the default value"
        );
    }

    #[test]
    fn set_features_volatile_write_cache_updates_current_value() {
        let (mut ctrl, mut mem) = enabled_controller();
        let disable = encode_sqe(
            ADMIN_OP_SET_FEATURES,
            0x72,
            0,
            0,
            u32::from(FEATURE_VOLATILE_WRITE_CACHE),
            0,
            0,
        );
        submit_admin(&mut ctrl, &mut mem, 0, &disable);
        assert_eq!(
            completion_status(&read_completion(&mem, ACQ_BASE, 0)),
            SC_SUCCESS
        );

        let current = encode_sqe(
            ADMIN_OP_GET_FEATURES,
            0x73,
            0,
            0,
            u32::from(FEATURE_VOLATILE_WRITE_CACHE),
            0,
            0,
        );
        submit_admin(&mut ctrl, &mut mem, 1, &current);
        let cqe = read_completion(&mem, ACQ_BASE, 1);
        assert_eq!(completion_status(&cqe), SC_SUCCESS);
        assert_eq!(completion_dw0(&cqe), 0);

        let enable = encode_sqe(
            ADMIN_OP_SET_FEATURES,
            0x74,
            0,
            0,
            u32::from(FEATURE_VOLATILE_WRITE_CACHE),
            1,
            0,
        );
        submit_admin(&mut ctrl, &mut mem, 2, &enable);
        assert_eq!(
            completion_status(&read_completion(&mem, ACQ_BASE, 2)),
            SC_SUCCESS
        );

        let current = encode_sqe(
            ADMIN_OP_GET_FEATURES,
            0x75,
            0,
            0,
            u32::from(FEATURE_VOLATILE_WRITE_CACHE),
            0,
            0,
        );
        submit_admin(&mut ctrl, &mut mem, 3, &current);
        let cqe = read_completion(&mem, ACQ_BASE, 3);
        assert_eq!(completion_status(&cqe), SC_SUCCESS);
        assert_eq!(completion_dw0(&cqe), 1);
    }

    #[test]
    fn get_features_apst_returns_zero_table() {
        let (mut ctrl, mut mem) = enabled_controller();
        assert!(mem.write_bytes(DATA_BASE, &[0xaa; 256]));
        let sqe = encode_sqe(
            ADMIN_OP_GET_FEATURES,
            9,
            0,
            DATA_BASE,
            u32::from(FEATURE_AUTONOMOUS_POWER_STATE_TRANSITION),
            0,
            0,
        );
        submit_admin(&mut ctrl, &mut mem, 0, &sqe);
        let cqe = read_completion(&mem, ACQ_BASE, 0);
        assert_eq!(completion_status(&cqe), SC_SUCCESS);
        assert_eq!(completion_dw0(&cqe), 0);
        assert_eq!(mem.read_bytes(DATA_BASE, 256).unwrap(), vec![0u8; 256]);
    }

    #[test]
    fn get_features_unknown_feature_matches_qemu_invalid_field_dnr() {
        let (mut ctrl, mut mem) = enabled_controller();
        for (slot, fid) in [0xd0u8, 0x7f].into_iter().enumerate() {
            let sqe = encode_sqe(
                ADMIN_OP_GET_FEATURES,
                10 + slot as u16,
                0,
                0,
                u32::from(fid),
                0,
                0,
            );
            submit_admin(&mut ctrl, &mut mem, slot as u16, &sqe);
            assert_eq!(
                completion_status(&read_completion(&mem, ACQ_BASE, slot as u16)),
                SC_INVALID_FIELD_DNR,
                "feature {fid:#x} matches QEMU's unsupported default with DNR"
            );
        }
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

        let trace = ctrl.recent_command_trace();
        let write_trace = trace
            .iter()
            .find(|event| event.sqid == 1 && event.command_id == 0x10)
            .expect("I/O WRITE command trace is retained");
        assert_eq!(write_trace.opcode, NVM_OP_WRITE);
        assert_eq!(write_trace.status, SC_SUCCESS);
        assert_eq!(write_trace.cdw10, slba as u32);
        assert!(write_trace.completion_posted);
        assert_eq!(write_trace.completion, None);

        let read_trace = trace
            .iter()
            .find(|event| event.sqid == 1 && event.command_id == 0x11)
            .expect("I/O READ command trace is retained");
        assert_eq!(read_trace.opcode, NVM_OP_READ);
        assert_eq!(read_trace.status, SC_SUCCESS);
        assert_eq!(read_trace.cdw10, slba as u32);
        assert!(read_trace.completion_posted);
        assert_eq!(read_trace.completion, None);
    }

    #[test]
    fn flush_command_completes_for_namespace_and_broadcast_nsid() {
        let (mut ctrl, mut mem) = enabled_controller();
        create_io_queue_pair(&mut ctrl, &mut mem, 0, CREATE_IO_CQ_PC_BIT);

        let flush = encode_sqe(NVM_OP_FLUSH, 0x76, NSID, 0, 0, 0, 0);
        assert!(mem.write_bytes(IO_SQ_BASE, &flush));
        ctrl.mmio_write(REG_DOORBELL_BASE + 2 * 4, 4, 1);
        ctrl.process(&mut mem);
        assert_eq!(
            completion_status(&read_completion(&mem, IO_CQ_BASE, 0)),
            SC_SUCCESS
        );

        let broadcast_flush = encode_sqe(NVM_OP_FLUSH, 0x77, u32::MAX, 0, 0, 0, 0);
        assert!(mem.write_bytes(IO_SQ_BASE + SQ_ENTRY_SIZE, &broadcast_flush));
        ctrl.mmio_write(REG_DOORBELL_BASE + 2 * 4, 4, 2);
        ctrl.process(&mut mem);
        assert_eq!(
            completion_status(&read_completion(&mem, IO_CQ_BASE, 1)),
            SC_SUCCESS
        );
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
    fn create_io_completion_queue_accepts_all_advertised_io_vectors() {
        for vector in 1..NVME_MSIX_VECTOR_COUNT {
            let (mut ctrl, mut mem) = enabled_controller();
            let cdw10 = (u32::from(QDEPTH - 1) << 16) | 1;
            let cq_cdw11 = CREATE_IO_CQ_PC_BIT
                | CREATE_IO_CQ_IEN_BIT
                | (u32::from(vector) << CREATE_IO_CQ_IV_SHIFT);

            submit_admin(
                &mut ctrl,
                &mut mem,
                0,
                &encode_sqe(ADMIN_OP_CREATE_IO_CQ, 1, 0, IO_CQ_BASE, cdw10, cq_cdw11, 0),
            );
            assert_eq!(
                completion_status(&read_completion(&mem, ACQ_BASE, 0)),
                SC_SUCCESS,
                "CREATE IO CQ should accept MSI-X vector {vector}"
            );
        }
    }

    #[test]
    fn create_io_queues_reject_depth_beyond_advertised_mqes() {
        let (mut ctrl, mut mem) = enabled_controller();
        let oversized_cdw10 = (u32::from(MAX_QUEUE_ENTRIES) << 16) | 1;
        submit_admin(
            &mut ctrl,
            &mut mem,
            0,
            &encode_sqe(
                ADMIN_OP_CREATE_IO_CQ,
                1,
                0,
                IO_CQ_BASE,
                oversized_cdw10,
                CREATE_IO_CQ_PC_BIT,
                0,
            ),
        );
        assert_eq!(
            completion_status(&read_completion(&mem, ACQ_BASE, 0)),
            SC_INVALID_FIELD
        );

        let valid_cdw10 = (u32::from(QDEPTH - 1) << 16) | 1;
        submit_admin(
            &mut ctrl,
            &mut mem,
            1,
            &encode_sqe(
                ADMIN_OP_CREATE_IO_CQ,
                1,
                0,
                IO_CQ_BASE,
                valid_cdw10,
                CREATE_IO_CQ_PC_BIT,
                0,
            ),
        );
        assert_eq!(
            completion_status(&read_completion(&mem, ACQ_BASE, 1)),
            SC_SUCCESS
        );

        submit_admin(
            &mut ctrl,
            &mut mem,
            2,
            &encode_sqe(
                ADMIN_OP_CREATE_IO_SQ,
                2,
                0,
                IO_SQ_BASE,
                (u32::from(MAX_QUEUE_ENTRIES) << 16) | 2,
                1u32 << 16,
                0,
            ),
        );
        assert_eq!(
            completion_status(&read_completion(&mem, ACQ_BASE, 2)),
            SC_INVALID_FIELD
        );

        submit_admin(
            &mut ctrl,
            &mut mem,
            3,
            &encode_sqe(
                ADMIN_OP_CREATE_IO_CQ,
                3,
                0,
                IO_CQ_BASE,
                (u32::from(u16::MAX) << 16) | 1,
                CREATE_IO_CQ_PC_BIT,
                0,
            ),
        );
        assert_eq!(
            completion_status(&read_completion(&mem, ACQ_BASE, 3)),
            SC_INVALID_FIELD
        );

        submit_admin(
            &mut ctrl,
            &mut mem,
            4,
            &encode_sqe(
                ADMIN_OP_CREATE_IO_SQ,
                4,
                0,
                IO_SQ_BASE,
                (u32::from(u16::MAX) << 16) | 2,
                1u32 << 16,
                0,
            ),
        );
        assert_eq!(
            completion_status(&read_completion(&mem, ACQ_BASE, 4)),
            SC_INVALID_FIELD
        );
    }

    #[test]
    fn create_io_queues_reject_qids_beyond_doorbell_aperture() {
        let (mut ctrl, mut mem) = enabled_controller();
        let invalid_qid = u32::from(MAX_IO_QUEUE_PAIRS) + 1;
        let cdw10 = (u32::from(QDEPTH - 1) << 16) | invalid_qid;

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
        assert_eq!(
            completion_status(&read_completion(&mem, ACQ_BASE, 0)),
            SC_INVALID_FIELD
        );

        let valid_cdw10 = (u32::from(QDEPTH - 1) << 16) | 1;
        submit_admin(
            &mut ctrl,
            &mut mem,
            1,
            &encode_sqe(
                ADMIN_OP_CREATE_IO_CQ,
                2,
                0,
                IO_CQ_BASE,
                valid_cdw10,
                CREATE_IO_CQ_PC_BIT,
                0,
            ),
        );
        assert_eq!(
            completion_status(&read_completion(&mem, ACQ_BASE, 1)),
            SC_SUCCESS
        );
        submit_admin(
            &mut ctrl,
            &mut mem,
            2,
            &encode_sqe(
                ADMIN_OP_CREATE_IO_SQ,
                3,
                0,
                IO_SQ_BASE,
                cdw10,
                1u32 << 16,
                0,
            ),
        );
        assert_eq!(
            completion_status(&read_completion(&mem, ACQ_BASE, 2)),
            SC_INVALID_FIELD
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

    #[test]
    fn read_into_matches_read_bytes_and_rejects_unbacked() {
        let mut mem = FakeMem::new(MEM_BASE, 0x2000);
        let pattern: Vec<u8> = (0..0x400u32).map(|i| (i * 7) as u8).collect();
        assert!(mem.write_bytes(MEM_BASE + 0x100, &pattern));

        // Zero-copy fill matches the allocating accessor byte-for-byte.
        let mut dst = vec![0u8; pattern.len()];
        assert!(mem.read_into(MEM_BASE + 0x100, &mut dst));
        assert_eq!(
            dst,
            mem.read_bytes(MEM_BASE + 0x100, pattern.len()).unwrap()
        );
        assert_eq!(dst, pattern);

        // The default trait implementation (routed through read_bytes) agrees.
        let mut via_default = vec![0u8; pattern.len()];
        assert!(
            GuestMemoryMut::read_bytes(&mem, MEM_BASE + 0x100, pattern.len())
                .map(|bytes| via_default.copy_from_slice(&bytes))
                .is_some()
        );
        assert_eq!(via_default, pattern);

        // Out-of-range spans are rejected, not truncated.
        let mut oob = vec![0u8; 0x10];
        assert!(!mem.read_into(MEM_BASE + 0x1ff8, &mut oob));
    }

    // ---- Stage 3 DMA path: coalescing + direct DMA + persistence ----------

    #[test]
    fn coalesce_spans_merges_contiguous_and_splits_on_gaps() {
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
    fn scatter_read_gathered(pages: usize, expose_host_ptr: bool) -> (Vec<u8>, Vec<u8>) {
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
        for k in 1..pages {
            let off = (k - 1) * 8;
            list[off..off + 8].copy_from_slice(&data_gpas[k].to_le_bytes());
        }
        assert!(mem.write_bytes(list_base, &list));

        let blocks = pages as u32 * (PAGE_SIZE as u32 / LBA_SIZE as u32);
        let read_cmd = encode_sqe_with_prps(
            NVM_OP_READ,
            0x60,
            NSID,
            data_gpas[0],
            list_base,
            0,
            0,
            blocks - 1,
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

    #[test]
    fn scatter_read_buffered_is_byte_identical_to_disk() {
        let (gathered, disk) = scatter_read_gathered(5, false);
        assert_eq!(
            gathered, disk,
            "buffered scatter read must reproduce the disk exactly"
        );
    }

    #[test]
    fn scatter_read_direct_dma_is_byte_identical_to_disk() {
        let (gathered, disk) = scatter_read_gathered(5, true);
        assert_eq!(
            gathered, disk,
            "direct-DMA scatter read must reproduce the disk exactly"
        );
    }

    #[test]
    fn scatter_read_direct_and_buffered_agree() {
        let (buffered, _) = scatter_read_gathered(6, false);
        let (direct, _) = scatter_read_gathered(6, true);
        assert_eq!(
            buffered, direct,
            "direct DMA and buffered fallback must be byte-identical"
        );
    }

    #[test]
    fn io_read_write_reuses_prp_span_and_segment_scratch() {
        let pages = 3usize;
        let disk: Vec<u8> = (0..PAGE_SIZE * pages)
            .map(|i| (i.wrapping_mul(17).wrapping_add(3)) as u8)
            .collect();
        let mut ctrl = NvmeController::with_disk_image(disk.clone());
        let mut mem = FakeMem::new(MEM_BASE, 0x40000);
        let data_gpas = [
            DATA_BASE + 0x2000,
            DATA_BASE + 0x2000 + 2 * PAGE_SIZE_U64,
            DATA_BASE + 0x2000 + 4 * PAGE_SIZE_U64,
        ];
        let list_base = DATA_BASE;
        let mut list = [0u8; 16];
        list[0..8].copy_from_slice(&data_gpas[1].to_le_bytes());
        list[8..16].copy_from_slice(&data_gpas[2].to_le_bytes());
        assert!(mem.write_bytes(list_base, &list));

        let blocks = pages as u32 * (PAGE_SIZE as u32 / LBA_SIZE as u32);
        let read_cmd = encode_sqe_with_prps(
            NVM_OP_READ,
            0x70,
            NSID,
            data_gpas[0],
            list_base,
            0,
            0,
            blocks - 1,
        );
        let read_cmd = SubmissionEntry::from_bytes(&read_cmd);
        assert_eq!(ctrl.io_read(&read_cmd, &mut mem), SC_SUCCESS);
        assert!(ctrl.prp_spans_scratch.is_empty());
        assert!(ctrl.io_segments_scratch.is_empty());
        assert!(ctrl.prp_spans_scratch.capacity() >= pages);
        assert!(ctrl.io_segments_scratch.capacity() >= pages);
        let span_scratch = (
            ctrl.prp_spans_scratch.as_ptr(),
            ctrl.prp_spans_scratch.capacity(),
        );
        let segment_scratch = (
            ctrl.io_segments_scratch.as_ptr(),
            ctrl.io_segments_scratch.capacity(),
        );

        let mut gathered = Vec::with_capacity(PAGE_SIZE * pages);
        for &gpa in &data_gpas {
            gathered.extend_from_slice(&mem.read_bytes(gpa, PAGE_SIZE).unwrap());
        }
        assert_eq!(gathered, disk);

        let mut expected = Vec::with_capacity(PAGE_SIZE * pages);
        for (page, &gpa) in data_gpas.iter().enumerate() {
            let chunk: Vec<u8> = (0..PAGE_SIZE)
                .map(|i| (0x80 | (page as u8 & 0x0f)).wrapping_add((i % 0x20) as u8))
                .collect();
            assert!(mem.write_bytes(gpa, &chunk));
            expected.extend_from_slice(&chunk);
        }
        let write_cmd = encode_sqe_with_prps(
            NVM_OP_WRITE,
            0x71,
            NSID,
            data_gpas[0],
            list_base,
            0,
            0,
            blocks - 1,
        );
        let write_cmd = SubmissionEntry::from_bytes(&write_cmd);
        assert_eq!(ctrl.io_write(&write_cmd, &mut mem), SC_SUCCESS);
        assert!(ctrl.prp_spans_scratch.is_empty());
        assert!(ctrl.io_segments_scratch.is_empty());
        assert_eq!(
            (
                ctrl.prp_spans_scratch.as_ptr(),
                ctrl.prp_spans_scratch.capacity()
            ),
            span_scratch
        );
        assert_eq!(
            (
                ctrl.io_segments_scratch.as_ptr(),
                ctrl.io_segments_scratch.capacity()
            ),
            segment_scratch
        );
        assert_eq!(&ctrl.disk_image()[..PAGE_SIZE * pages], expected.as_slice());
    }

    /// Write `pages` distinct guest pages scattered across non-adjacent guest
    /// pages into a fresh disk, returning (disk_image, expected_concatenation).
    fn scatter_write_result(pages: usize, expose_host_ptr: bool) -> (Vec<u8>, Vec<u8>) {
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
        for k in 1..pages {
            let off = (k - 1) * 8;
            list[off..off + 8].copy_from_slice(&data_gpas[k].to_le_bytes());
        }
        assert!(mem.write_bytes(list_base, &list));

        let blocks = pages as u32 * (PAGE_SIZE as u32 / LBA_SIZE as u32);
        let write_cmd = encode_sqe_with_prps(
            NVM_OP_WRITE,
            0x61,
            NSID,
            data_gpas[0],
            list_base,
            0,
            0,
            blocks - 1,
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

    #[test]
    fn scatter_write_buffered_is_byte_identical() {
        let (disk, expected) = scatter_write_result(5, false);
        assert_eq!(
            disk, expected,
            "buffered scatter write must land byte-identical"
        );
    }

    #[test]
    fn scatter_write_direct_dma_is_byte_identical() {
        let (disk, expected) = scatter_write_result(5, true);
        assert_eq!(
            disk, expected,
            "direct-DMA scatter write must land byte-identical"
        );
    }

    #[test]
    fn single_sector_read_write_direct_dma_roundtrips() {
        let (mut ctrl, mut mem) =
            enabled_controller_with_disk_and_mem_len(vec![0u8; LBA_SIZE * 8], 0x10000);
        mem.enable_host_ptr();
        create_io_queue_pair(&mut ctrl, &mut mem, 0, CREATE_IO_CQ_PC_BIT);

        let payload: Vec<u8> = (0..LBA_SIZE).map(|i| 0xc0 | (i % 0x20) as u8).collect();
        assert!(mem.write_bytes(DATA_BASE, &payload));
        let write = encode_sqe(NVM_OP_WRITE, 0x62, NSID, DATA_BASE, 2, 0, 0);
        assert!(mem.write_bytes(IO_SQ_BASE, &write));
        ctrl.mmio_write(REG_DOORBELL_BASE + 2 * 4, 4, 1);
        ctrl.process(&mut mem);

        let read_gpa = DATA_BASE + PAGE_SIZE_U64;
        let read = encode_sqe(NVM_OP_READ, 0x63, NSID, read_gpa, 2, 0, 0);
        assert!(mem.write_bytes(IO_SQ_BASE + SQ_ENTRY_SIZE, &read));
        ctrl.mmio_write(REG_DOORBELL_BASE + 2 * 4, 4, 2);
        ctrl.process(&mut mem);
        assert_eq!(mem.read_bytes(read_gpa, LBA_SIZE).unwrap(), payload);
    }

    #[test]
    fn read_crossing_prp_list_page_boundary_reproduces_disk() {
        // A tiny PRP list page (offset near end of a page => 2 slots) forces the
        // list to chain into a second list page mid-transfer.
        let pages = 4usize;
        let disk: Vec<u8> = (0..PAGE_SIZE * pages).map(|i| (i % 0xf1) as u8).collect();
        let (mut ctrl, mut mem) = enabled_controller_with_disk_and_mem_len(disk.clone(), 0x40000);
        create_io_queue_pair(&mut ctrl, &mut mem, 0, CREATE_IO_CQ_PC_BIT);

        let data0 = DATA_BASE + 0x8000;
        let data1 = data0 + PAGE_SIZE_U64;
        let data2 = data1 + PAGE_SIZE_U64;
        let data3 = data2 + PAGE_SIZE_U64;
        // list A: 2 slots (16 bytes) at the tail of its page.
        let list_a = DATA_BASE + (PAGE_SIZE_U64 - 16);
        // list B: page-aligned second list page.
        let list_b = DATA_BASE + PAGE_SIZE_U64;
        // list A: [data1, ->list_b]; list B: [data2, data3].
        let mut a = [0u8; 16];
        a[0..8].copy_from_slice(&data1.to_le_bytes());
        a[8..16].copy_from_slice(&list_b.to_le_bytes());
        assert!(mem.write_bytes(list_a, &a));
        let mut b = [0u8; 16];
        b[0..8].copy_from_slice(&data2.to_le_bytes());
        b[8..16].copy_from_slice(&data3.to_le_bytes());
        assert!(mem.write_bytes(list_b, &b));

        let blocks = pages as u32 * (PAGE_SIZE as u32 / LBA_SIZE as u32);
        let read_cmd =
            encode_sqe_with_prps(NVM_OP_READ, 0x64, NSID, data0, list_a, 0, 0, blocks - 1);
        assert!(mem.write_bytes(IO_SQ_BASE, &read_cmd));
        ctrl.mmio_write(REG_DOORBELL_BASE + 2 * 4, 4, 1);
        ctrl.process(&mut mem);
        assert_eq!(
            completion_status(&read_completion(&mem, IO_CQ_BASE, 0)),
            SC_SUCCESS
        );
        for (page, &g) in [data0, data1, data2, data3].iter().enumerate() {
            let s = page * PAGE_SIZE;
            assert_eq!(
                mem.read_bytes(g, PAGE_SIZE).unwrap(),
                disk[s..s + PAGE_SIZE],
                "page {page} across the chained PRP list"
            );
        }
    }

    #[test]
    fn transfer_crossing_namespace_end_is_rejected_but_last_sector_succeeds() {
        let sectors = 8usize;
        let (mut ctrl, mut mem) =
            enabled_controller_with_disk_and_mem_len(vec![0u8; LBA_SIZE * sectors], 0x10000);
        create_io_queue_pair(&mut ctrl, &mut mem, 0, CREATE_IO_CQ_PC_BIT);

        // Reading the exact last sector is in range.
        let last = (sectors - 1) as u32;
        let ok = encode_sqe(NVM_OP_READ, 0x65, NSID, DATA_BASE, last, 0, 0);
        assert!(mem.write_bytes(IO_SQ_BASE, &ok));
        ctrl.mmio_write(REG_DOORBELL_BASE + 2 * 4, 4, 1);
        ctrl.process(&mut mem);
        assert_eq!(
            completion_status(&read_completion(&mem, IO_CQ_BASE, 0)),
            SC_SUCCESS
        );

        // Two sectors starting at the last sector runs one sector past the end.
        let over = encode_sqe(NVM_OP_READ, 0x66, NSID, DATA_BASE, last, 0, 1);
        assert!(mem.write_bytes(IO_SQ_BASE + SQ_ENTRY_SIZE, &over));
        ctrl.mmio_write(REG_DOORBELL_BASE + 2 * 4, 4, 2);
        ctrl.process(&mut mem);
        assert_eq!(
            completion_status(&read_completion(&mem, IO_CQ_BASE, 1)),
            SC_INVALID_FIELD
        );

        // Writes past the end are likewise rejected.
        assert!(mem.write_bytes(DATA_BASE, &vec![0xffu8; LBA_SIZE * 2]));
        let over_w = encode_sqe(NVM_OP_WRITE, 0x67, NSID, DATA_BASE, last, 0, 1);
        assert!(mem.write_bytes(IO_SQ_BASE + 2 * SQ_ENTRY_SIZE, &over_w));
        ctrl.mmio_write(REG_DOORBELL_BASE + 2 * 4, 4, 3);
        ctrl.process(&mut mem);
        assert_eq!(
            completion_status(&read_completion(&mem, IO_CQ_BASE, 2)),
            SC_INVALID_FIELD
        );
    }

    #[test]
    fn write_back_persists_to_source_file_synchronously_without_flush() {
        let source = temp_path("dma-writeback-sync");
        fs::write(&source, vec![0u8; LBA_SIZE * 64]).unwrap();
        let (mut ctrl, mut mem) = enabled_controller_with_raw_file(&source, true, 0x20000);
        create_io_queue_pair(&mut ctrl, &mut mem, 0, CREATE_IO_CQ_PC_BIT);

        // Two contiguous pages (coalesce to a single pwrite of 8 KiB).
        let payload: Vec<u8> = (0..PAGE_SIZE * 2)
            .map(|i| 0xa0u8.wrapping_add((i % 0x33) as u8))
            .collect();
        assert!(mem.write_bytes(DATA_BASE, &payload));
        let slba = 4u64;
        let blocks = (PAGE_SIZE * 2 / LBA_SIZE) as u32;
        let write = encode_sqe_with_prps(
            NVM_OP_WRITE,
            0x71,
            NSID,
            DATA_BASE,
            DATA_BASE + PAGE_SIZE_U64,
            slba as u32,
            (slba >> 32) as u32,
            blocks - 1,
        );
        assert!(mem.write_bytes(IO_SQ_BASE, &write));
        ctrl.mmio_write(REG_DOORBELL_BASE + 2 * 4, 4, 1);
        ctrl.process(&mut mem);
        assert_eq!(
            completion_status(&read_completion(&mem, IO_CQ_BASE, 0)),
            SC_SUCCESS
        );

        // Read the source through an independent handle WITHOUT flushing first:
        // the write-through contract requires bytes to already be on disk.
        let start = slba as usize * LBA_SIZE;
        let on_disk = fs::read(&source).unwrap();
        assert_eq!(
            &on_disk[start..start + payload.len()],
            payload.as_slice(),
            "write-back must reach the host file synchronously, before any flush"
        );
        fs::remove_file(source).ok();
    }

    #[test]
    fn overlay_write_merges_into_coalesced_read() {
        let source = temp_path("dma-overlay-merge");
        let sectors = 32usize;
        let base: Vec<u8> = (0..LBA_SIZE * sectors).map(|i| (i % 253) as u8).collect();
        fs::write(&source, &base).unwrap();
        let (mut ctrl, mut mem) = enabled_controller_with_raw_file(&source, false, 0x20000);
        create_io_queue_pair(&mut ctrl, &mut mem, 0, CREATE_IO_CQ_PC_BIT);

        // Overwrite sector 5 through the sparse overlay (read-only backend).
        let repl: Vec<u8> = (0..LBA_SIZE).map(|i| 0xf0 | (i % 0x0f) as u8).collect();
        assert!(mem.write_bytes(DATA_BASE, &repl));
        let w = encode_sqe(NVM_OP_WRITE, 0x81, NSID, DATA_BASE, 5, 0, 0);
        assert!(mem.write_bytes(IO_SQ_BASE, &w));
        ctrl.mmio_write(REG_DOORBELL_BASE + 2 * 4, 4, 1);
        ctrl.process(&mut mem);

        // Coalesced 2-page read over sectors 0..16 must reflect the overlay at 5.
        let read_gpa = DATA_BASE + 0x4000;
        let blocks = (PAGE_SIZE * 2 / LBA_SIZE) as u32;
        let read = encode_sqe_with_prps(
            NVM_OP_READ,
            0x82,
            NSID,
            read_gpa,
            read_gpa + PAGE_SIZE_U64,
            0,
            0,
            blocks - 1,
        );
        assert!(mem.write_bytes(IO_SQ_BASE + SQ_ENTRY_SIZE, &read));
        ctrl.mmio_write(REG_DOORBELL_BASE + 2 * 4, 4, 2);
        ctrl.process(&mut mem);
        assert_eq!(
            completion_status(&read_completion(&mem, IO_CQ_BASE, 1)),
            SC_SUCCESS
        );

        let mut expected = base[0..PAGE_SIZE * 2].to_vec();
        expected[5 * LBA_SIZE..6 * LBA_SIZE].copy_from_slice(&repl);
        assert_eq!(
            mem.read_bytes(read_gpa, PAGE_SIZE * 2).unwrap(),
            expected,
            "coalesced read must merge the sparse overlay over the whole span"
        );
        fs::remove_file(source).ok();
    }

    #[test]
    fn overlay_chunk_starting_before_read_offset_merges_into_partial_read() {
        let source = temp_path("dma-overlay-partial-read");
        let sectors = 16usize;
        let base: Vec<u8> = (0..LBA_SIZE * sectors)
            .map(|i| 0x10u8.wrapping_add((i % 0x6d) as u8))
            .collect();
        fs::write(&source, &base).unwrap();
        let mut backend = DiskBackend::raw_file(&source, false).unwrap();
        let offset = (LBA_SIZE * 5) as u64;
        let replacement: Vec<u8> = (0..LBA_SIZE)
            .map(|i| 0xc0u8.wrapping_add((i % 0x21) as u8))
            .collect();

        backend.write_at(offset, &replacement).unwrap();

        let mut readback = vec![0u8; LBA_SIZE];
        backend.read_at_into(offset, &mut readback).unwrap();

        assert_eq!(
            readback, replacement,
            "overlay chunk base is before the read offset and must still merge"
        );
        fs::remove_file(source).ok();
    }

    #[test]
    #[ignore = "micro-benchmark; run with `--ignored --nocapture`"]
    fn bench_dma_disk_read_coalescing() {
        let path = temp_path("dma-bench");
        let total = 4 * 1024 * 1024usize; // 4 MiB per transfer
        fs::write(&path, vec![0x5au8; total]).unwrap();
        let iters = 200usize;
        let mut backend = DiskBackend::raw_file(&path, false).unwrap();
        let mut dst = vec![0u8; total];

        // Old shape: one allocation + one pread + one copy per 4 KiB PRP page.
        let t0 = std::time::Instant::now();
        for _ in 0..iters {
            let mut off = 0u64;
            while (off as usize) < total {
                let page = backend.read_at(off, PAGE_SIZE).unwrap();
                let s = off as usize;
                dst[s..s + PAGE_SIZE].copy_from_slice(&page);
                off += PAGE_SIZE_U64;
            }
        }
        let old = t0.elapsed();

        // New shape: one coalesced pread into the destination, no allocations.
        let t1 = std::time::Instant::now();
        for _ in 0..iters {
            backend.read_at_into(0, &mut dst).unwrap();
        }
        let new = t1.elapsed();

        let mb = (total * iters) as f64 / (1024.0 * 1024.0);
        eprintln!(
            "nvme dma read {total_kib} KiB/xfer: old per-page {old_mbps:.0} MB/s ({pages} allocs+syscalls/xfer) -> new coalesced {new_mbps:.0} MB/s (1 syscall/xfer), speedup {ratio:.2}x",
            total_kib = total / 1024,
            old_mbps = mb / old.as_secs_f64(),
            pages = total / PAGE_SIZE,
            new_mbps = mb / new.as_secs_f64(),
            ratio = old.as_secs_f64() / new.as_secs_f64(),
        );
        fs::remove_file(path).ok();
    }
}
