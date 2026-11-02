//! Completion-ring backpressure before submission consumption or I/O effects.

use super::*;

impl NvmeController {
    pub(crate) fn cq_has_space(&self, cqid: u16) -> bool {
        let Some(Some(cq)) = self.cqs.get(usize::from(cqid)) else {
            return false;
        };
        // NVMe 1.4 section 4.1 reserves one slot: next tail == head is full.
        // Flushes in flight hold a slot each for their later completion.
        let (size, tail, head) = (u32::from(cq.size), u32::from(cq.tail), u32::from(cq.head));
        let used = (tail + size - head) % size.max(1);
        let held = u32::from(self.flush_reserved_slots(cqid));
        size >= 2 && tail < size && head < size && used + 1 + held < size
    }
}
