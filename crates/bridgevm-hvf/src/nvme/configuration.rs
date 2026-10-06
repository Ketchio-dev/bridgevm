//! CC enable/reset and shutdown notification with backend error propagation.

use super::*;

pub(super) const CSTS_SHST_MASK: u32 = 3 << 2;
const CSTS_SHST_PROCESSING: u32 = 1 << 2;
const CSTS_SHST_COMPLETE: u32 = 2 << 2;
const CSTS_CFS_BIT: u32 = 1 << 1;

impl NvmeController {
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
            self.csts = (self.csts & !(CSTS_SHST_MASK | CSTS_CFS_BIT)) | CSTS_RDY_BIT;
        } else if !now_enabled && was_enabled {
            // Reset: drop all queues and clear ready.
            self.sqs = vec![None];
            self.cqs = vec![None];
            self.pending_sq_bits.clear();
            self.pending_sq_bits.push(0);
            self.csts &= !(CSTS_RDY_BIT | CSTS_SHST_MASK | CSTS_CFS_BIT);
            self.pending_async_event_requests = 0;
        }
        self.process_shutdown_notification(value);
    }

    fn process_shutdown_notification(&mut self, value: u32) {
        if !matches!((value >> 14) & 3, 1 | 2) || self.csts & CSTS_SHST_MASK != 0 {
            return;
        }
        // A combined EN-clear/SHN write resets first, then shuts down. SHN=0
        // does not cancel shutdown; another attempt requires a controller reset.
        self.csts = (self.csts & !CSTS_SHST_MASK) | CSTS_SHST_PROCESSING;
        if self.flush_all_namespaces() == SC_SUCCESS {
            self.csts = (self.csts & !CSTS_SHST_MASK) | CSTS_SHST_COMPLETE;
        } else {
            // MMIO shutdown has no command completion in which to report a
            // failed sync. Keep shutdown incomplete and signal a fatal error.
            self.csts |= CSTS_CFS_BIT;
        }
    }
}
