//! Split out of nvme.rs to keep files under 850 lines.

use super::*;
use crate::msix::MsixTable;
use crate::pcie::NVME_MSIX_VECTOR_COUNT;
use std::collections::BTreeMap;
use std::collections::VecDeque;
use std::fs::File;
use std::fs::OpenOptions;
use std::io;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;
use std::io::Write;
use std::path::Path;

/// Size of one submission-queue entry, in bytes (NVMe fixed: 64).
pub const SQ_ENTRY_SIZE: u64 = 64;
/// Size of one completion-queue entry, in bytes (NVMe fixed: 16).
pub const CQ_ENTRY_SIZE: u64 = 16;
/// Logical block (LBA) size this model exposes to the guest.
pub const LBA_SIZE: usize = 512;
/// Guest-visible memory page size assumed for PRP transfers (single page).
pub const PAGE_SIZE: usize = 4096;
pub(crate) type NvmePage = [u8; PAGE_SIZE];
pub(crate) const PAGE_SIZE_U64: u64 = PAGE_SIZE as u64;
pub(crate) const FILE_OVERLAY_CHUNK_SIZE: u64 = PAGE_SIZE_U64;
pub(crate) const EXPORT_CHUNK_SIZE: usize = 1024 * 1024;
pub(crate) const ZERO_APST_FEATURE_DATA: [u8; 256] = [0u8; 256];
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
pub(crate) const CAP_MQES_SHIFT: u64 = 0;
/// `CAP.CQR` (contiguous queues required), bit 16. We require contiguous queues.
pub(crate) const CAP_CQR_BIT: u64 = 1 << 16;
/// `CAP.TO` (timeout, 500 ms units), bits 31:24. Advertise a generous 1 s.
pub(crate) const CAP_TO_SHIFT: u64 = 24;
/// `CAP.DSTRD` (doorbell stride) bits 35:32 — 0 ⇒ 4-byte stride.
pub(crate) const CAP_DSTRD_SHIFT: u64 = 32;
/// `CAP.CSS` (command sets supported), bits 44:37. Bit 37 ⇒ NVM command set.
pub(crate) const CAP_CSS_NVM_BIT: u64 = 1 << 37;

// ---- VS (NVMe 1.4 §3.1.2) -------------------------------------------------
/// Version 1.4.0 encoded as `(MJR << 16) | (MNR << 8) | TER`.
pub const NVME_VERSION_1_4_0: u32 = 0x0001_0400;

// ---- CC fields (NVMe 1.4 §3.1.5) ------------------------------------------
pub(crate) const CC_EN_BIT: u32 = 1 << 0;

// ---- CSTS fields (NVMe 1.4 §3.1.6) ----------------------------------------
pub(crate) const CSTS_RDY_BIT: u32 = 1 << 0;

// ---- Admin opcodes (NVMe 1.4 §5, Figure 139) ------------------------------
pub(crate) const ADMIN_OP_DELETE_IO_SQ: u8 = 0x00;
pub(crate) const ADMIN_OP_CREATE_IO_SQ: u8 = 0x01;
pub(crate) const ADMIN_OP_GET_LOG_PAGE: u8 = 0x02;
pub(crate) const ADMIN_OP_DELETE_IO_CQ: u8 = 0x04;
pub(crate) const ADMIN_OP_CREATE_IO_CQ: u8 = 0x05;
pub(crate) const ADMIN_OP_IDENTIFY: u8 = 0x06;
pub(crate) const ADMIN_OP_SET_FEATURES: u8 = 0x09;
pub(crate) const ADMIN_OP_GET_FEATURES: u8 = 0x0a;
pub(crate) const ADMIN_OP_ASYNC_EVENT_REQUEST: u8 = 0x0c;
pub(crate) const ADMIN_OP_SECURITY_SEND: u8 = 0x81;
pub(crate) const ADMIN_OP_SECURITY_RECV: u8 = 0x82;

// ---- NVM (I/O) opcodes (NVMe NVM Command Set) -----------------------------
pub(crate) const NVM_OP_FLUSH: u8 = 0x00;
pub(crate) const NVM_OP_WRITE: u8 = 0x01;
pub(crate) const NVM_OP_READ: u8 = 0x02;

// ---- Command Set Identifiers (NVMe 1.4 §7.1) ------------------------------
pub(crate) const COMMAND_SET_NVM: u8 = 0x00;

// ---- IDENTIFY CNS values (NVMe 1.4 §5.15.1) -------------------------------
pub(crate) const IDENTIFY_CNS_NAMESPACE: u32 = 0x00;
pub(crate) const IDENTIFY_CNS_CONTROLLER: u32 = 0x01;
pub(crate) const IDENTIFY_CNS_ACTIVE_NAMESPACE_LIST: u32 = 0x02;
pub(crate) const IDENTIFY_CNS_NAMESPACE_DESCRIPTOR_LIST: u32 = 0x03;
pub(crate) const IDENTIFY_CNS_COMMAND_SET_CONTROLLER: u32 = 0x06;

// ---- SET FEATURES feature IDs (NVMe 1.4 §5.21.1) --------------------------
pub(crate) const FEATURE_ARBITRATION: u8 = 0x01;
pub(crate) const FEATURE_POWER_MANAGEMENT: u8 = 0x02;
pub(crate) const FEATURE_TEMPERATURE_THRESHOLD: u8 = 0x04;
pub(crate) const FEATURE_ERROR_RECOVERY: u8 = 0x05;
pub(crate) const FEATURE_VOLATILE_WRITE_CACHE: u8 = 0x06;
pub(crate) const FEATURE_NUMBER_OF_QUEUES: u8 = 0x07;
pub(crate) const FEATURE_INTERRUPT_COALESCING: u8 = 0x08;
pub(crate) const FEATURE_INTERRUPT_VECTOR_CONFIGURATION: u8 = 0x09;
pub(crate) const FEATURE_WRITE_ATOMICITY_NORMAL: u8 = 0x0a;
pub(crate) const FEATURE_ASYNC_EVENT_CONFIGURATION: u8 = 0x0b;
pub(crate) const FEATURE_AUTONOMOUS_POWER_STATE_TRANSITION: u8 = 0x0c;
pub(crate) const GET_FEATURE_SELECT_SHIFT: u32 = 8;
pub(crate) const GET_FEATURE_SELECT_DEFAULT: u32 = 0x1;
pub(crate) const GET_FEATURE_SELECT_SAVED: u32 = 0x2;
pub(crate) const GET_FEATURE_SELECT_CAPABILITIES: u32 = 0x3;
pub(crate) const FEATURE_CAP_NAMESPACE_SPECIFIC: u32 = 1 << 1;
pub(crate) const FEATURE_CAP_CHANGEABLE: u32 = 1 << 2;

// ---- Identify Controller feature bits -------------------------------------
pub(crate) const VWC_PRESENT: u8 = 1 << 0;
pub(crate) const VWC_NSID_BROADCAST_SUPPORT: u8 = 3 << 1;
pub(crate) const VWC_QEMU_DEFAULT: u8 = VWC_PRESENT | VWC_NSID_BROADCAST_SUPPORT;

// ---- CREATE I/O COMPLETION QUEUE fields (NVMe 1.4 §5.3) -------------------
pub(crate) const CREATE_IO_CQ_PC_BIT: u32 = 1 << 0;
pub(crate) const CREATE_IO_CQ_IEN_BIT: u32 = 1 << 1;
pub(crate) const CREATE_IO_CQ_IV_SHIFT: u32 = 16;

// ---- GET LOG PAGE log identifiers (NVMe 1.4 §5.14.1) ----------------------
pub(crate) const LOG_PAGE_SMART_HEALTH: u8 = 0x02;
pub(crate) const LOG_PAGE_FIRMWARE_SLOT_INFO: u8 = 0x03;
pub(crate) const LOG_PAGE_COMMAND_EFFECTS: u8 = 0x05;

// ---- Command Effects log bits (NVMe 1.4 §5.14.1.5) ------------------------
pub(crate) const CMD_EFFECT_CSUPP: u32 = 1 << 0;
pub(crate) const CMD_EFFECT_LBCC: u32 = 1 << 1;

// ---- Security protocol values (NVMe 1.4 §5.22 / QEMU nvme_security_*) -----
pub(crate) const SECURITY_PROTOCOL_INFORMATION: u8 = 0x00;
pub(crate) const SECURITY_PROTOCOL_DMTF_SPDM: u8 = 0xe8;
pub(crate) const SECURITY_PROTOCOL_INFO_LIST_LEN: usize = 10;

// ---- Completion status codes (NVMe 1.4 §4.6.1, generic command status) ----
/// Successful completion (status code type 0, code 0).
pub(crate) const SC_SUCCESS: u16 = 0x0000;
/// Invalid Field in Command.
pub(crate) const SC_INVALID_FIELD: u16 = 0x0002;
/// Internal Device Error. Used when a valid command reaches the backend but
/// the host cannot complete the requested I/O operation.
pub(crate) const SC_INTERNAL_DEVICE_ERROR: u16 = 0x0006;
/// Do Not Retry bit, carried in the NVMe completion status code field.
pub(crate) const SC_DNR: u16 = 0x4000;
/// QEMU's default for unsupported optional/vendor command surfaces.
pub(crate) const SC_INVALID_FIELD_DNR: u16 = SC_INVALID_FIELD | SC_DNR;
/// Invalid Opcode.
pub(crate) const SC_INVALID_OPCODE: u16 = 0x0001;

/// The primary namespace's identifier (NSID 1).
pub const NSID: u32 = 1;
/// Optional second namespace (NSID 2), used as a blank Windows install target
/// alongside the NSID-1 installer source.
pub const NSID2: u32 = 2;

pub(crate) const NS_EUI64: [u8; 8] = *b"BVMNVME1";
pub(crate) const NS_NGUID: [u8; 16] = *b"BridgeVM-NVMeNS1";
pub(crate) const NS_UUID: [u8; 16] = [
    0x42, 0x56, 0x4d, 0x00, 0x20, 0x26, 0x06, 0x20, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01,
];
pub(crate) const NS2_EUI64: [u8; 8] = *b"BVMNVME2";
pub(crate) const NS2_NGUID: [u8; 16] = *b"BridgeVM-NVMeNS2";
pub(crate) const NS2_UUID: [u8; 16] = [
    0x42, 0x56, 0x4d, 0x00, 0x20, 0x26, 0x06, 0x20, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02,
];

/// Per-namespace stable identifiers (NGUID, EUI64, UUID) for NSID 1 and 2.
pub(crate) fn namespace_identifiers(nsid: u32) -> ([u8; 16], [u8; 8], [u8; 16]) {
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
pub(crate) struct SubmissionQueue {
    /// Guest-physical base address of the (contiguous) queue.
    pub(crate) base: u64,
    /// Number of entries (queue depth).
    pub(crate) size: u16,
    /// Consumer-side head; the controller advances it as it fetches entries.
    pub(crate) head: u16,
    /// Producer-side tail last reported by the guest through the SQ doorbell.
    pub(crate) tail_doorbell: u16,
    /// The completion queue this SQ posts completions to.
    pub(crate) cqid: u16,
}

/// State for one completion queue created by the guest.
#[derive(Debug, Clone)]
pub(crate) struct CompletionQueue {
    /// Guest-physical base address of the (contiguous) queue.
    pub(crate) base: u64,
    /// Number of entries (queue depth).
    pub(crate) size: u16,
    /// Producer-side tail; the controller advances it as it posts completions.
    pub(crate) tail: u16,
    /// Phase tag; toggles every time the tail wraps (NVMe 1.4 §4.6).
    pub(crate) phase: bool,
    /// Last head the guest reported through the CQ doorbell.
    pub(crate) head: u16,
    /// MSI-X vector to signal when this completion queue receives an entry.
    pub(crate) interrupt_vector: u16,
    /// Whether completions on this CQ should generate an interrupt.
    pub(crate) interrupts_enabled: bool,
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
pub(crate) struct CommandResult {
    pub(crate) status: u16,
    pub(crate) complete: bool,
}

impl CommandResult {
    pub(crate) const fn complete(status: u16) -> Self {
        Self {
            status,
            complete: true,
        }
    }

    pub(crate) const fn pending() -> Self {
        Self {
            status: SC_SUCCESS,
            complete: false,
        }
    }
}

#[derive(Debug)]
pub(crate) enum DiskBackend {
    Memory(Vec<u8>),
    RawFile(RawFileDisk),
}

#[derive(Debug)]
pub(crate) struct RawFileDisk {
    pub(crate) file: File,
    pub(crate) len: u64,
    pub(crate) overlay: BTreeMap<u64, Vec<u8>>,
    pub(crate) write_back: bool,
    #[cfg(test)]
    pub(crate) sync_failure: Option<io::ErrorKind>,
    #[cfg(test)]
    pub(crate) sync_attempts: usize,
}

impl DiskBackend {
    pub(crate) fn memory(mut disk: Vec<u8>) -> Self {
        let len = rounded_disk_len(disk.len());
        disk.resize(len, 0);
        Self::Memory(disk)
    }

    pub(crate) fn raw_file(path: impl AsRef<Path>, write_back: bool) -> io::Result<Self> {
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
            #[cfg(test)]
            sync_failure: None,
            #[cfg(test)]
            sync_attempts: 0,
        }))
    }

    pub(crate) fn byte_len(&self) -> u64 {
        match self {
            Self::Memory(disk) => disk.len() as u64,
            Self::RawFile(disk) => disk.len,
        }
    }

    pub(crate) fn memory_image(&self) -> Option<&[u8]> {
        match self {
            Self::Memory(disk) => Some(disk),
            Self::RawFile(_) => None,
        }
    }

    pub(crate) fn read_at(&mut self, offset: u64, len: usize) -> io::Result<Vec<u8>> {
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
    pub(crate) fn read_at_into(&mut self, offset: u64, dst: &mut [u8]) -> io::Result<()> {
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

    pub(crate) fn write_at(&mut self, offset: u64, data: &[u8]) -> io::Result<()> {
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

    pub(crate) fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Memory(_) => Ok(()),
            Self::RawFile(disk) => disk.flush(),
        }
    }

    pub(crate) fn export_to_path(&mut self, path: impl AsRef<Path>) -> io::Result<u64> {
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

    pub(crate) fn validate_range(&self, offset: u64, len: usize) -> io::Result<()> {
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
    /// Make prior write-through writes durable on the backing storage. A
    /// read-only raw backend keeps guest writes in its volatile COW overlay, so
    /// it has no host-file data to synchronize.
    pub(crate) fn flush(&mut self) -> io::Result<()> {
        if !self.write_back {
            return Ok(());
        }

        #[cfg(test)]
        {
            self.sync_attempts += 1;
            if let Some(kind) = self.sync_failure {
                return Err(io::Error::new(kind, "injected raw-file sync failure"));
            }
        }

        self.file.sync_data()
    }

    /// Read `dst.len()` bytes at `offset`: one `pread` of the whole span from the
    /// backing file, then the sparse write overlay merged on top. Merging over the
    /// whole span (rather than page-by-page) keeps the coalesced read a single
    /// syscall while preserving the COW-overlay read semantics exactly.
    pub(crate) fn read_at_into(&mut self, offset: u64, dst: &mut [u8]) -> io::Result<()> {
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

    pub(crate) fn write_at(&mut self, offset: u64, data: &[u8]) -> io::Result<()> {
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

    pub(crate) fn chunk_len(&self, chunk_base: u64) -> io::Result<usize> {
        if chunk_base >= self.len {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "overlay chunk starts past the NVMe image",
            ));
        }
        Ok((self.len - chunk_base).min(FILE_OVERLAY_CHUNK_SIZE) as usize)
    }
}

pub(crate) fn second_namespace_missing() -> io::Error {
    io::Error::new(io::ErrorKind::NotFound, "NVMe NSID 2 is not attached")
}

/// A modelled minimal NVMe controller.
#[derive(Debug)]
pub struct NvmeController {
    // --- BAR0 register backing store ---
    pub(crate) cc: u32,
    pub(crate) csts: u32,
    pub(crate) aqa: u32,
    pub(crate) asq: u64,
    pub(crate) acq: u64,
    pub(crate) intms: u32,

    // --- Queues. Index 0 is the admin queue; 1.. are I/O queues. ---
    pub(crate) sqs: Vec<Option<SubmissionQueue>>,
    pub(crate) cqs: Vec<Option<CompletionQueue>>,
    pub(crate) pending_sq_bits: Vec<u64>,

    // --- Backend ---
    /// Raw disk backing store for NSID 1, `LBA_SIZE`-byte logical blocks.
    pub(crate) disk: DiskBackend,
    /// Optional NSID-2 backing store (blank Windows install target).
    pub(crate) disk2: Option<DiskBackend>,
    /// Negotiated maximum number of I/O queue pairs (SET FEATURES 0x07).
    pub(crate) max_io_queues: u16,
    /// Command-specific result for the *next* completion's DW0 (e.g. the queue
    /// count granted by SET FEATURES). Consumed when the completion is posted.
    pub(crate) last_feature_result: u32,
    /// Current volatile write cache state. QEMU's default NVMe endpoint
    /// advertises a present cache and boots with it enabled.
    pub(crate) volatile_write_cache_enabled: bool,
    /// BAR-backed MSI-X table and PBA for this endpoint.
    pub(crate) msix: MsixTable,
    /// Recent command/completion history for live Windows bring-up.
    pub(crate) command_trace: VecDeque<NvmeCommandTrace>,
    /// Reusable staging buffer for the data path's buffered fallback (used when
    /// the guest-memory view exposes no stable host pointer for direct DMA). Kept
    /// at its high-water mark across commands so a steady stream of IO reuses one
    /// allocation instead of allocating per PRP page. Holds no state between
    /// commands; each command fully overwrites the prefix it reads.
    pub(crate) io_scratch: Vec<u8>,
    /// Whether the zero-copy host-pointer data path is allowed. The default is
    /// enabled; `BRIDGEVM_NVME_BUFFERED_IO=1` forces the byte-identical buffered
    /// path for one-process storage-integrity A/B diagnostics.
    pub(crate) direct_dma_enabled: bool,
    /// Reusable PRP decode output for NVM READ/WRITE. The hot I/O path fills this
    /// per command instead of allocating a span vector for every transfer.
    pub(crate) prp_spans_scratch: Vec<(u64, usize)>,
    /// Reusable physically-contiguous guest segments derived from PRP spans.
    pub(crate) io_segments_scratch: Vec<(u64, usize)>,
    /// Outstanding AER commands that should complete only when an async event is
    /// raised. This minimal controller does not raise events yet, so it only
    /// tracks the advertised limit and leaves accepted requests pending.
    pub(crate) pending_async_event_requests: u8,
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

    pub(crate) fn with_disk_backend(disk: DiskBackend) -> Self {
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
            direct_dma_enabled: true,
            prp_spans_scratch: Vec::new(),
            io_segments_scratch: Vec::new(),
            pending_async_event_requests: 0,
        }
    }

    /// Replace the backing disk image, padding to a full LBA. This resets queue
    /// and controller register state, mirroring a cold-plugged different device.
    pub fn load_disk_image(&mut self, disk: Vec<u8>) {
        let direct_dma_enabled = self.direct_dma_enabled;
        *self = Self::with_disk_image(disk);
        self.direct_dma_enabled = direct_dma_enabled;
    }

    /// Replace the backing disk with a host raw file and reset controller state.
    pub fn load_raw_file(&mut self, path: impl AsRef<Path>, write_back: bool) -> io::Result<()> {
        let direct_dma_enabled = self.direct_dma_enabled;
        *self = Self::with_raw_file(path, write_back)?;
        self.direct_dma_enabled = direct_dma_enabled;
        Ok(())
    }

    pub fn reset_registers_keep_disks(&mut self) {
        let direct_dma_enabled = self.direct_dma_enabled;
        let disk = std::mem::replace(&mut self.disk, DiskBackend::memory(Vec::new()));
        let disk2 = self.disk2.take();
        *self = Self::with_disk_backend(disk);
        self.disk2 = disk2;
        self.direct_dma_enabled = direct_dma_enabled;
    }

    /// Select the NVMe data path without changing queue or media state. This is
    /// primarily useful for deterministic diagnostics and tests.
    pub fn set_direct_dma_enabled(&mut self, enabled: bool) {
        self.direct_dma_enabled = enabled;
    }

    pub fn direct_dma_enabled(&self) -> bool {
        self.direct_dma_enabled
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
    pub(crate) fn namespace_count(&self) -> u32 {
        if self.disk2.is_some() {
            2
        } else {
            1
        }
    }

    /// Immutable backing store for `nsid`, if that namespace is active.
    pub(crate) fn backend_for_nsid(&self, nsid: u32) -> Option<&DiskBackend> {
        match nsid {
            NSID => Some(&self.disk),
            NSID2 => self.disk2.as_ref(),
            _ => None,
        }
    }

    /// Mutable backing store for `nsid`, if that namespace is active.
    pub(crate) fn backend_for_nsid_mut(&mut self, nsid: u32) -> Option<&mut DiskBackend> {
        match nsid {
            NSID => Some(&mut self.disk),
            NSID2 => self.disk2.as_mut(),
            _ => None,
        }
    }

    /// Block count for a specific namespace.
    pub(crate) fn block_count_for(&self, nsid: u32) -> u64 {
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
}
