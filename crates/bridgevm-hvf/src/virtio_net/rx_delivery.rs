//! RX buffer delivery after the device readiness check.
use super::*;
use crate::fwcfg::GuestMemoryMut;

impl<B: NetBackend> VirtioNet<B> {
    pub(crate) fn deliver_rx_frame(&mut self, frame: &[u8], mem: &mut dyn GuestMemoryMut) -> bool {
        let queue_index = QUEUE_RX;
        let queue = self.queues[queue_index];
        if !queue.ready || queue.size == 0 || queue.desc == 0 {
            return false;
        }
        let Some(avail_idx) = read_u16(mem, queue.driver, 2) else {
            return false;
        };
        if self.queues[queue_index].last_avail_idx == avail_idx {
            return false;
        }
        let last_avail_idx = self.queues[queue_index].last_avail_idx;
        let ring_off = 4 + u64::from(last_avail_idx % queue.size) * 2;
        let Some(head) = read_u16(mem, queue.driver, ring_off) else {
            return false;
        };
        let mut descs = std::mem::take(&mut self.descriptor_scratch);
        let delivered = Self::descriptor_chain_into(mem, &queue, head, &mut descs);
        if !delivered {
            self.descriptor_scratch = descs;
            return false;
        }
        let mut hdr = [0u8; VIRTIO_NET_HDR_LEN];
        hdr[10..12].copy_from_slice(&1u16.to_le_bytes());
        if !Self::scatter_write_slices(mem, &descs, &[&hdr, frame]) {
            self.descriptor_scratch = descs;
            return false;
        }
        let used_len =
            u32::try_from(VIRTIO_NET_HDR_LEN.saturating_add(frame.len())).unwrap_or(u32::MAX);
        descs.clear();
        self.descriptor_scratch = descs;
        Self::write_used(mem, &queue, head, used_len);
        self.queues[queue_index].last_avail_idx = last_avail_idx.wrapping_add(1);
        self.stats.rx_count = self.stats.rx_count.saturating_add(1);
        self.stats.rx_bytes = self.stats.rx_bytes.saturating_add(frame.len() as u64);
        self.mark_queue_interrupt(queue_index);
        true
    }
}
