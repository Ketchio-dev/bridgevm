//! Queue-index validation and pending-work updates for decoded doorbell writes.

use super::*;

impl NvmeController {
    /// Record a doorbell write. The doorbell layout (DSTRD = 0, 4-byte stride)
    /// is `SQ0TDBL, CQ0HDBL, SQ1TDBL, CQ1HDBL, …` — i.e. for doorbell index
    /// `n`, even `n` is `SQ(n/2)` tail and odd `n` is `CQ(n/2)` head.
    pub(super) fn write_doorbell(&mut self, offset: u64, value: u32) {
        let idx = ((offset - REG_DOORBELL_BASE) / 4) as usize;
        let qid = idx / 2;
        let is_cq = idx % 2 == 1;
        let val = value as u16;
        if is_cq {
            if let Some(Some(cq)) = self.cqs.get_mut(qid) {
                if val >= cq.size {
                    return;
                }
                cq.head = val;
            }
        } else {
            let mut has_work = None;
            if let Some(Some(sq)) = self.sqs.get_mut(qid) {
                // A wrapped SQ head can never reach a tail outside its queue.
                if val >= sq.size {
                    return;
                }
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
