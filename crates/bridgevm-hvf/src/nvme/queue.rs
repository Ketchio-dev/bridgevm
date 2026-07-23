//! SQ/CQ state model, queue lifecycle, pending-doorbell bookkeeping, and the fetch-execute-complete drain engine.

use super::*;
use crate::fwcfg::GuestMemoryMut;
use crate::pcie::NVME_MSIX_VECTOR_COUNT;

/// Grow `slots` so index `idx` is addressable, filling new slots with `None`.
pub(crate) fn ensure_slot<T>(slots: &mut Vec<Option<T>>, idx: usize) {
    if idx >= slots.len() {
        slots.resize_with(idx + 1, || None);
    }
}

/// Size of one submission-queue entry, in bytes (NVMe fixed: 64).
pub const SQ_ENTRY_SIZE: u64 = 64;

/// Size of one completion-queue entry, in bytes (NVMe fixed: 16).
pub const CQ_ENTRY_SIZE: u64 = 16;

/// Maximum number of submission/completion queue entries we advertise
/// (`CAP.MQES` is 0-based, so the wire value is `MAX_QUEUE_ENTRIES - 1`).
pub const MAX_QUEUE_ENTRIES: u16 = 1024;

/// I/O queue-pair capacity advertised to SET FEATURES (NUMBER OF QUEUES). The
/// model only drives one, but exposes a small pool so a multi-queue guest gets
/// a sane non-zero allocation back.
pub const MAX_IO_QUEUE_PAIRS: u16 = 8;

// ---- CREATE I/O COMPLETION QUEUE fields (NVMe 1.4 §5.3) -------------------
pub(crate) const CREATE_IO_CQ_PC_BIT: u32 = 1 << 0;

pub(crate) const CREATE_IO_CQ_IEN_BIT: u32 = 1 << 1;

pub(crate) const CREATE_IO_CQ_IV_SHIFT: u32 = 16;

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

impl NvmeController {
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

    pub(crate) fn mark_sq_pending(&mut self, qid: usize) {
        let word_idx = qid / 64;
        if self.pending_sq_bits.len() <= word_idx {
            self.pending_sq_bits.resize(word_idx + 1, 0);
        }
        self.pending_sq_bits[word_idx] |= 1u64 << (qid % 64);
    }

    pub(crate) fn clear_sq_pending(&mut self, qid: usize) {
        let word_idx = qid / 64;
        if let Some(word) = self.pending_sq_bits.get_mut(word_idx) {
            *word &= !(1u64 << (qid % 64));
        }
    }

    pub(crate) fn sq_has_work(&self, qid: usize) -> bool {
        self.sqs
            .get(qid)
            .and_then(Option::as_ref)
            .is_some_and(|sq| sq.head != sq.tail_doorbell)
    }

    /// Drain submission queue `qid` until its head catches the guest's tail.
    pub(crate) fn process_sq(
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

    /// CREATE I/O COMPLETION QUEUE (NVMe 1.4 §5.3). CDW10: QID bits 15:0,
    /// QSIZE bits 31:16 (0-based). CDW11: PC bit 0, IEN bit 1, interrupt
    /// vector bits 31:16. PRP1 is the queue base.
    pub(crate) fn admin_create_io_cq(&mut self, cmd: &SubmissionEntry) -> u16 {
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
    pub(crate) fn admin_create_io_sq(&mut self, cmd: &SubmissionEntry) -> u16 {
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

    /// Post a 16-byte completion-queue entry for `cmd` into completion queue
    /// `cqid`, advancing its tail and toggling the phase tag on wrap.
    pub(crate) fn post_completion(
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
}
