//! I/O queue creation without replacing live queue identities.

use super::queue_geometry::io_queue_size;
use super::queue_lifecycle::SC_INVALID_QUEUE_IDENTIFIER;
use super::*;
use crate::pcie::NVME_MSIX_VECTOR_COUNT;

impl NvmeController {
    /// CREATE I/O COMPLETION QUEUE (NVMe 1.4 §5.3). CDW10: QID bits 15:0,
    /// QSIZE bits 31:16 (0-based). CDW11: PC bit 0, IEN bit 1, interrupt
    /// vector bits 31:16. PRP1 is the queue base.
    pub(crate) fn admin_create_io_cq(&mut self, cmd: &SubmissionEntry) -> u16 {
        let qid = (cmd.cdw10 & 0xffff) as usize;
        let interrupt_vector = ((cmd.cdw11 >> CREATE_IO_CQ_IV_SHIFT) & 0xffff) as u16;
        let interrupts_enabled = cmd.cdw11 & CREATE_IO_CQ_IEN_BIT != 0;
        if qid == 0 || qid > usize::from(self.max_io_queues) {
            return SC_INVALID_FIELD; // QID 0 is admin; higher QIDs lack doorbells.
        }
        let qsize = match io_queue_size(cmd) {
            Ok(size) => size,
            Err(status) => return status,
        };
        if interrupts_enabled && interrupt_vector >= NVME_MSIX_VECTOR_COUNT {
            return SC_INVALID_FIELD;
        }
        if self.cqs.get(qid).is_some_and(Option::is_some) {
            return SC_INVALID_QUEUE_IDENTIFIER;
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
        let cqid = ((cmd.cdw11 >> 16) & 0xffff) as u16;
        if qid == 0 || qid > usize::from(self.max_io_queues) {
            return SC_INVALID_FIELD;
        }
        let qsize = match io_queue_size(cmd) {
            Ok(size) => size,
            Err(status) => return status,
        };
        // The completion queue this SQ targets must already exist.
        if self.cqs.get(cqid as usize).map(Option::is_some) != Some(true) {
            return SC_INVALID_FIELD;
        }
        if cqid == 0 || self.sqs.get(qid).is_some_and(Option::is_some) {
            return SC_INVALID_QUEUE_IDENTIFIER;
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
}
