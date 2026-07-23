//! BAR0 register map and guest MMIO read/write decode, including CC enable/reset and doorbell capture.

use super::*;

// ---- small helpers --------------------------------------------------------

/// Mask `value` to a 1/2/4/8-byte access width.
pub(crate) fn mask_to_size(value: u64, size: u8) -> u64 {
    match size {
        1 => value & 0xff,
        2 => value & 0xffff,
        4 => value & 0xffff_ffff,
        _ => value,
    }
}

pub(crate) fn is_modelled_doorbell(offset: u64) -> bool {
    (REG_DOORBELL_BASE..REG_DOORBELL_END).contains(&offset) && offset % 4 == 0
}

/// Merge a partial write into a 64-bit register. `high` selects the upper
/// 32-bit half (for split 32-bit accesses to a 64-bit register).
pub(crate) fn merge_u64(current: u64, value: u64, size: u8, high: bool) -> u64 {
    if size >= 8 {
        return value;
    }
    if high {
        (current & 0x0000_0000_ffff_ffff) | (u64::from(value as u32) << 32)
    } else {
        (current & 0xffff_ffff_0000_0000) | u64::from(value as u32)
    }
}

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

impl NvmeController {
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
}
