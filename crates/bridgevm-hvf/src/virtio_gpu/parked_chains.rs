//! Bounds on control-queue chains held back from the used ring.

use super::*;
use crate::fwcfg::GuestMemoryMut;

impl VirtioGpu {
    /// Vblank-paced and fence-parked control responses keep their chain in use.
    /// A driver owns at most `queue_size` heads, so more parked chains means
    /// it resubmitted heads still in flight. QEMU v11.0.0 `virtqueue_pop`
    /// pops nothing once `inuse >= vring.num`; stop consuming the same way.
    pub(crate) fn parked_chains_fill_queue(&self, queue_size: u16) -> bool {
        self.pending_vblank.len() + self.pending_fenced.len() >= usize::from(queue_size)
    }

    pub(crate) fn write_used(
        mem: &mut dyn GuestMemoryMut,
        queue: &VirtioGpuQueue,
        id: u16,
        len: u32,
    ) {
        if queue.device == 0 {
            return;
        }
        let queue_size = queue.effective_size();
        write_used(mem, queue.device, queue_size, id, len);
    }
}
