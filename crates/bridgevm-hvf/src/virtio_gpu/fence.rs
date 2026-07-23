//! Backend fence retirement and the responses parked on it.

use super::*;
use crate::fwcfg::GuestMemoryMut;
use crate::virtio_gpu_3d::CompletedFence;

#[derive(Debug, Clone)]
pub(crate) struct PendingFencedResponse {
    pub(crate) queue_index: usize,
    pub(crate) queue: VirtioGpuQueue,
    pub(crate) head: u16,
    pub(crate) descs: Vec<Descriptor>,
    pub(crate) response: Vec<u8>,
    pub(crate) fence: CompletedFence,
}

impl VirtioGpu {
    pub fn drain_completed_fences(&mut self, mem: &mut dyn GuestMemoryMut) {
        self.drain_completed_fences_inner(mem, false);
    }

    pub(crate) fn drain_completed_fences_after_queue(&mut self, mem: &mut dyn GuestMemoryMut) {
        self.drain_completed_fences_inner(mem, true);
    }

    pub(crate) fn drain_completed_fences_inner(
        &mut self,
        mem: &mut dyn GuestMemoryMut,
        after_queue: bool,
    ) {
        let mut completed = std::mem::take(&mut self.completed_fences_scratch);
        completed.clear();
        if after_queue {
            self.three_d
                .drain_completed_fences_after_queue_into(&mut completed);
        } else {
            self.three_d.drain_completed_fences_into(&mut completed);
        }
        if completed.is_empty() || self.pending_fenced.is_empty() {
            for fence in &completed {
                self.trace_fence_complete(*fence);
            }
            completed.clear();
            self.completed_fences_scratch = completed;
            return;
        }
        for fence in &completed {
            self.trace_fence_complete(*fence);
        }
        let mut index = 0;
        while index < self.pending_fenced.len() {
            let ready = completed.iter().any(|completed| {
                let pending_response = &self.pending_fenced[index];
                completed.ctx_id == pending_response.fence.ctx_id
                    && completed.ring_idx == pending_response.fence.ring_idx
                    && completed.fence_id >= pending_response.fence.fence_id
            });
            if !ready {
                index += 1;
                continue;
            }

            let pending_response = self.pending_fenced.remove(index);
            let used_len =
                Self::scatter_write(mem, &pending_response.descs, &pending_response.response);
            self.trace_fence_delivery(pending_response.fence, used_len);
            Self::write_used(
                mem,
                &pending_response.queue,
                pending_response.head,
                used_len,
            );
            self.mark_queue_interrupt(pending_response.queue_index);
            self.recycle_parked_response_buffers(pending_response.descs, pending_response.response);
        }
        completed.clear();
        self.completed_fences_scratch = completed;
    }
}
