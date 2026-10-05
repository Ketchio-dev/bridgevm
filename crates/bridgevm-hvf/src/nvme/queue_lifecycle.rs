//! I/O queue deletion and implicit abort of commands not yet executed.

use super::*;

// NVMe 1.4 sections 5.5–5.6: command-specific status (SCT = 1).
const SC_INVALID_QUEUE_IDENTIFIER: u16 = 0x0101;
const SC_INVALID_QUEUE_DELETION: u16 = 0x010c;

impl NvmeController {
    pub(crate) fn admin_delete_io_queue(&mut self, cmd: &SubmissionEntry) -> u16 {
        let qid = (cmd.cdw10 & 0xffff) as usize;
        if qid == 0 {
            return SC_INVALID_QUEUE_IDENTIFIER;
        }
        match cmd.opcode {
            ADMIN_OP_DELETE_IO_SQ => {
                if !self.sqs.get(qid).is_some_and(Option::is_some) {
                    return SC_INVALID_QUEUE_IDENTIFIER;
                }
                // Execution is synchronous. Removing unfetched work implicitly
                // aborts it before the admin delete completion is posted.
                self.sqs[qid] = None;
                self.clear_sq_pending(qid);
            }
            ADMIN_OP_DELETE_IO_CQ => {
                if !self.cqs.get(qid).is_some_and(Option::is_some) {
                    return SC_INVALID_QUEUE_IDENTIFIER;
                }
                if self
                    .sqs
                    .iter()
                    .flatten()
                    .any(|sq| usize::from(sq.cqid) == qid)
                {
                    return SC_INVALID_QUEUE_DELETION;
                }
                self.cqs[qid] = None;
            }
            _ => return SC_INVALID_OPCODE,
        }
        SC_SUCCESS
    }
}
