//! Fence creation, backend polling, and completed-fence draining.

use super::*;

impl VirtioGpu3d {
    pub fn drain_completed_fences(&mut self) -> Vec<CompletedFence> {
        let mut completed = Vec::new();
        self.drain_completed_fences_into(&mut completed);
        completed
    }

    pub fn drain_completed_fences_into(&mut self, out: &mut Vec<CompletedFence>) {
        self.drain_completed_fences_inner(out, false);
    }

    pub fn drain_completed_fences_after_queue_into(&mut self, out: &mut Vec<CompletedFence>) {
        self.drain_completed_fences_inner(out, true);
    }

    pub(crate) fn drain_completed_fences_inner(
        &mut self,
        out: &mut Vec<CompletedFence>,
        after_queue: bool,
    ) {
        let Some(backend) = self.backend.as_mut() else {
            return;
        };
        // Venus on macOS retires fences synchronously: polling the backend may
        // invoke the fence callback inline, then drain_completed_fences takes
        // the callbacks queued by that poll.
        if after_queue {
            backend.poll_fences_after_queue();
        } else {
            backend.poll_fences();
        }
        let start = out.len();
        backend.drain_completed_fences_into(out);
        self.fences_completed = self
            .fences_completed
            .saturating_add((out.len() - start) as u64);
    }

    pub fn create_fence(&mut self, fence: CompletedFence) -> bool {
        let Some(backend) = self.backend.as_mut() else {
            return false;
        };
        backend.create_fence(fence.ctx_id, fence.ring_idx, fence.fence_id)
    }
}
