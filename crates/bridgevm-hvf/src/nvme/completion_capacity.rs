//! Completion-ring backpressure before submission consumption or I/O effects.

use super::*;

impl NvmeController {
    pub(crate) fn cq_has_space(&self, cqid: u16) -> bool {
        let Some(Some(cq)) = self.cqs.get(usize::from(cqid)) else {
            return false;
        };
        // NVMe 1.4 section 4.1 reserves one slot: next tail == head is full.
        cq.size >= 2 && cq.tail < cq.size && cq.head < cq.size && (cq.tail + 1) % cq.size != cq.head
    }
}
