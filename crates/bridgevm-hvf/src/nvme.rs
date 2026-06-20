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
//!     disk image with 512-byte LBAs, using PRP1 for single-page transfers.
//!
//! The DMA/queue path mirrors `fwcfg.rs`: all guest-memory traffic flows through
//! the shared [`GuestMemoryMut`] trait, and completions are written straight
//! back into the guest's completion queue.
//!
//! References: NVM Express Base Specification 1.4, sections 3 (controller
//! registers), 5 (admin command set) and the NVM Command Set; QEMU `hw/nvme/`.

use crate::fwcfg::GuestMemoryMut;

/// Size of one submission-queue entry, in bytes (NVMe fixed: 64).
pub const SQ_ENTRY_SIZE: u64 = 64;
/// Size of one completion-queue entry, in bytes (NVMe fixed: 16).
pub const CQ_ENTRY_SIZE: u64 = 16;
/// Logical block (LBA) size this model exposes to the guest.
pub const LBA_SIZE: usize = 512;
/// Guest-visible memory page size assumed for PRP transfers (single page).
pub const PAGE_SIZE: usize = 4096;
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

// ---- SET FEATURES feature IDs (NVMe 1.4 §5.21.1) --------------------------
const FEATURE_NUMBER_OF_QUEUES: u8 = 0x07;

// ---- Completion status codes (NVMe 1.4 §4.6.1, generic command status) ----
/// Successful completion (status code type 0, code 0).
const SC_SUCCESS: u16 = 0x0000;
/// Invalid Field in Command.
const SC_INVALID_FIELD: u16 = 0x0002;
/// Invalid Opcode.
const SC_INVALID_OPCODE: u16 = 0x0001;

/// The single namespace's identifier (NSID 1).
pub const NSID: u32 = 1;

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
}

/// A modelled minimal NVMe controller.
#[derive(Debug, Clone)]
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
    /// Flat disk backing store, `LBA_SIZE`-byte logical blocks.
    disk: Vec<u8>,
    /// Negotiated maximum number of I/O queue pairs (SET FEATURES 0x07).
    max_io_queues: u16,
    /// Command-specific result for the *next* completion's DW0 (e.g. the queue
    /// count granted by SET FEATURES). Consumed when the completion is posted.
    last_feature_result: u32,
}

impl NvmeController {
    /// Create a controller with a `disk_bytes`-sized backing store. The size is
    /// rounded up to a whole number of `LBA_SIZE` blocks.
    pub fn new(disk_bytes: usize) -> Self {
        Self::with_disk_image(vec![0u8; rounded_disk_len(disk_bytes)])
    }

    /// Create a controller from an existing raw disk image. The image is padded
    /// with zeros up to a whole number of `LBA_SIZE` blocks so namespace capacity
    /// and transfer bounds stay block-aligned.
    pub fn with_disk_image(mut disk: Vec<u8>) -> Self {
        let len = rounded_disk_len(disk.len());
        disk.resize(len, 0);
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
        }
    }

    /// Replace the backing disk image, padding to a full LBA. This resets queue
    /// and controller register state, mirroring a cold-plugged different device.
    pub fn load_disk_image(&mut self, disk: Vec<u8>) {
        *self = Self::with_disk_image(disk);
    }

    /// Snapshot of the current disk image, including guest writes that have been
    /// processed through the NVMe queues.
    pub fn disk_image(&self) -> &[u8] {
        &self.disk
    }

    /// Number of `LBA_SIZE`-byte logical blocks in the backing disk.
    pub fn block_count(&self) -> u64 {
        (self.disk.len() / LBA_SIZE) as u64
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
    pub fn process(&mut self, mem: &mut dyn GuestMemoryMut) {
        // Admin queue (index 0) first, then I/O queues.
        let sq_count = self.sqs.len();
        for qid in 0..sq_count {
            self.process_sq(qid, mem);
        }
    }

    /// Drain submission queue `qid` until its head catches the guest's tail.
    fn process_sq(&mut self, qid: usize, mem: &mut dyn GuestMemoryMut) {
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
            self.post_completion(cqid, qid as u16, &cmd, status, mem);
        }
    }

    /// Execute an admin command, returning the NVMe status field to report.
    fn execute_admin(&mut self, cmd: &SubmissionEntry, mem: &mut dyn GuestMemoryMut) -> u16 {
        match cmd.opcode {
            ADMIN_OP_IDENTIFY => self.admin_identify(cmd, mem),
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
        // SQES (512): min/max submission-queue entry size = 2^6 = 64 bytes.
        d[512] = 0x66;
        // CQES (513): min/max completion-queue entry size = 2^4 = 16 bytes.
        d[513] = 0x44;
        // NN (516..520): number of namespaces = 1.
        d[516..520].copy_from_slice(&1u32.to_le_bytes());
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
        // LBAF0 (128..132): MS=0, LBADS = log2(512) = 9 (bits 23:16), RP=0.
        let lbads: u32 = 9 << 16;
        d[128..132].copy_from_slice(&lbads.to_le_bytes());
        d
    }

    /// CREATE I/O COMPLETION QUEUE (NVMe 1.4 §5.3). CDW10: QID bits 15:0,
    /// QSIZE bits 31:16 (0-based). PRP1 is the queue base.
    fn admin_create_io_cq(&mut self, cmd: &SubmissionEntry) -> u16 {
        let qid = (cmd.cdw10 & 0xffff) as usize;
        let qsize = ((cmd.cdw10 >> 16) & 0xffff) as u16 + 1;
        if qid == 0 {
            return SC_INVALID_FIELD; // QID 0 is the admin queue
        }
        ensure_slot(&mut self.cqs, qid);
        self.cqs[qid] = Some(CompletionQueue {
            base: cmd.prp1,
            size: qsize,
            tail: 0,
            phase: true,
            head: 0,
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
    /// (0-based). Single-page PRP1 transfer ⇒ at most `PAGE_SIZE` bytes.
    fn io_read(&self, cmd: &SubmissionEntry, mem: &mut dyn GuestMemoryMut) -> u16 {
        let Some((start, len)) = self.transfer_range(cmd) else {
            return SC_INVALID_FIELD;
        };
        let data = self.disk[start..start + len].to_vec();
        if mem.write_bytes(cmd.prp1, &data) {
            SC_SUCCESS
        } else {
            SC_INVALID_FIELD
        }
    }

    /// NVM WRITE (0x01). Same addressing as READ; copies guest data into disk.
    fn io_write(&mut self, cmd: &SubmissionEntry, mem: &mut dyn GuestMemoryMut) -> u16 {
        let Some((start, len)) = self.transfer_range(cmd) else {
            return SC_INVALID_FIELD;
        };
        let Some(data) = mem.read_bytes(cmd.prp1, len) else {
            return SC_INVALID_FIELD;
        };
        self.disk[start..start + len].copy_from_slice(&data);
        SC_SUCCESS
    }

    /// Decode (SLBA, NLB) into a byte range into `self.disk`, validating it fits
    /// the disk and a single PRP page. Returns `(start_byte, len_bytes)`.
    fn transfer_range(&self, cmd: &SubmissionEntry) -> Option<(usize, usize)> {
        let slba = u64::from(cmd.cdw10) | (u64::from(cmd.cdw11) << 32);
        let nlb = u64::from(cmd.cdw12 & 0xffff) + 1; // 0-based count
        let len = (nlb as usize).checked_mul(LBA_SIZE)?;
        if len > PAGE_SIZE {
            return None; // single-page PRP1 transfers only
        }
        let start = (slba as usize).checked_mul(LBA_SIZE)?;
        if start.checked_add(len)? > self.disk.len() {
            return None; // out of range
        }
        Some((start, len))
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
    ) {
        let dw0 = std::mem::take(&mut self.last_feature_result);
        let (base, tail, size, phase, sq_head) = match self.cqs.get(cqid as usize) {
            Some(Some(cq)) => {
                let sq_head = match self.sqs.get(sqid as usize) {
                    Some(Some(sq)) => sq.head,
                    _ => 0,
                };
                (cq.base, cq.tail, cq.size, cq.phase, sq_head)
            }
            _ => return,
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
        let _ = mem.write_bytes(entry_gpa, &entry);

        // Advance the CQ tail, toggling the phase tag when it wraps.
        let new_tail = (tail + 1) % size;
        if let Some(Some(cq)) = self.cqs.get_mut(cqid as usize) {
            cq.tail = new_tail;
            if new_tail == 0 {
                cq.phase = !cq.phase;
            }
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

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
        let mut e = [0u8; 64];
        let cdw0 = u32::from(opcode) | (u32::from(command_id) << 16);
        e[0..4].copy_from_slice(&cdw0.to_le_bytes());
        e[4..8].copy_from_slice(&nsid.to_le_bytes());
        e[24..32].copy_from_slice(&prp1.to_le_bytes());
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

    /// Enable a fresh controller with admin queues installed.
    fn enabled_controller() -> (NvmeController, FakeMem) {
        let mut ctrl = NvmeController::new(1 << 20); // 1 MiB disk
        let mem = FakeMem::new(MEM_BASE, 0x8000);
        // Program AQA (0-based sizes), ASQ, ACQ, then set CC.EN.
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
        let cq_cmd = encode_sqe(ADMIN_OP_CREATE_IO_CQ, 1, 0, IO_CQ_BASE, cdw10, 0, 0);
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
    fn read_out_of_range_lba_fails() {
        let (mut ctrl, mut mem) = enabled_controller();
        // Create I/O CQ + SQ (QID 1) as above.
        let cdw10 = (u32::from(QDEPTH - 1) << 16) | 1;
        submit_admin(
            &mut ctrl,
            &mut mem,
            0,
            &encode_sqe(ADMIN_OP_CREATE_IO_CQ, 1, 0, IO_CQ_BASE, cdw10, 0, 0),
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
