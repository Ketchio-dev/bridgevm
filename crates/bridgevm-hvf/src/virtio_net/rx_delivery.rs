//! RX buffer delivery after the device readiness check.
use super::*;
use crate::fwcfg::GuestMemoryMut;

impl<B: NetBackend> VirtioNet<B> {
    pub(crate) fn deliver_rx_frame(&mut self, frame: &[u8], mem: &mut dyn GuestMemoryMut) -> bool {
        let queue_index = QUEUE_RX;
        let queue = self.pci.queues[queue_index];
        if !queue.ready || queue.size == 0 || queue.desc == 0 {
            return false;
        }
        let Some(avail_idx) = read_avail_idx(mem, queue.driver) else {
            return false;
        };
        if self.pci.queues[queue_index].last_avail_idx == avail_idx {
            return false;
        }
        let last_avail_idx = self.pci.queues[queue_index].last_avail_idx;
        let ring_off = 4 + u64::from(last_avail_idx % queue.size) * 2;
        let Some(head) = read_u16(mem, queue.driver, ring_off) else {
            return false;
        };
        let mut descs = std::mem::take(&mut self.descriptor_scratch);
        let delivered = descriptor_chain_into(mem, queue.desc, queue.size, head, &mut descs);
        if !delivered {
            self.descriptor_scratch = descs;
            return false;
        }
        // A frame larger than the buffer the guest posted can never be delivered: drop it, or it
        // waits for a bigger buffer forever and every later frame waits behind it.
        let capacity: usize = descs.iter().map(|desc| desc.len as usize).sum();
        if capacity < VIRTIO_NET_HDR_LEN + frame.len() {
            if !std::mem::replace(&mut self.rx_oversize_reported, true) {
                crate::host_log::out(format_args!(
                    "virtio-net: dropped a {}-byte frame larger than the guest's {}-byte receive buffer",
                    frame.len(),
                    capacity
                ));
            }
            self.descriptor_scratch = descs;
            return true;
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
        write_used(mem, queue.device, queue.size, head, used_len);
        self.pci.queues[queue_index].last_avail_idx = last_avail_idx.wrapping_add(1);
        self.stats.rx_count = self.stats.rx_count.saturating_add(1);
        self.stats.rx_bytes = self.stats.rx_bytes.saturating_add(frame.len() as u64);
        self.pci.mark_queue_interrupt(queue_index);
        true
    }
}
