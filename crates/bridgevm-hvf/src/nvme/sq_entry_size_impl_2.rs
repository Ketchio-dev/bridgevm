//! Continuation of the `sq_entry_size` impl block, split for the 1000-line rule.

use super::*;

use crate::fwcfg::GuestMemoryMut;
use std::io;
use std::path::Path;

impl NvmeController {
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
    pub(crate) fn cap(&self) -> u64 {
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
    pub(crate) fn write_cc(&mut self, value: u32) {
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
    pub(crate) fn write_doorbell(&mut self, offset: u64, value: u32) {
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
}
