//! The packet path: TX drain to the backend, RX delivery with the virtio-net header, doorbells.

use super::*;
use crate::fwcfg::GuestMemoryMut;

impl<B: NetBackend> VirtioNet<B> {
    pub(crate) fn notify_queue(&mut self, queue_index: u16, mem: &mut dyn GuestMemoryMut) {
        self.stats.notify_count = self.stats.notify_count.saturating_add(1);
        if usize::from(queue_index) == QUEUE_TX {
            self.process_tx_queue(mem);
        }
    }

    pub fn pump_receive(&mut self, mem: &mut dyn GuestMemoryMut) -> bool {
        let mut frame = if let Some(frame) = self.pending_rx_frame.take() {
            frame
        } else {
            let mut frame = std::mem::take(&mut self.rx_frame_scratch);
            if !self.backend.poll_receive_into(&mut frame) {
                self.rx_frame_scratch = frame;
                return false;
            }
            frame
        };
        if self.deliver_rx_frame(&frame, mem) {
            frame.clear();
            self.rx_frame_scratch = frame;
            return true;
        }
        self.pending_rx_frame = Some(frame);
        false
    }

    pub(crate) fn process_tx_queue(&mut self, mem: &mut dyn GuestMemoryMut) {
        let queue_index = QUEUE_TX;
        let queue = self.queues[queue_index];
        if !queue.ready || queue.size == 0 || queue.desc == 0 {
            return;
        }
        let Some(avail_idx) = read_u16(mem, queue.driver + 2) else {
            return;
        };
        let mut descs = std::mem::take(&mut self.descriptor_scratch);
        let mut packet = std::mem::take(&mut self.tx_packet_scratch);
        while self.queues[queue_index].last_avail_idx != avail_idx {
            let last_avail_idx = self.queues[queue_index].last_avail_idx;
            let ring_off = 4 + u64::from(last_avail_idx % queue.size) * 2;
            let Some(head) = read_u16(mem, queue.driver + ring_off) else {
                break;
            };
            if Self::tx_frame_from_chain_into(mem, &queue, head, &mut descs, &mut packet) {
                let frame = &packet[VIRTIO_NET_HDR_LEN..];
                self.stats.tx_count = self.stats.tx_count.saturating_add(1);
                self.stats.tx_bytes = self.stats.tx_bytes.saturating_add(frame.len() as u64);
                self.backend.transmit(frame);
            }
            Self::write_used(mem, &queue, head, 0);
            self.queues[queue_index].last_avail_idx = last_avail_idx.wrapping_add(1);
            self.mark_queue_interrupt(queue_index);
        }
        descs.clear();
        packet.clear();
        self.descriptor_scratch = descs;
        self.tx_packet_scratch = packet;
    }

    pub(crate) fn tx_frame_from_chain_into(
        mem: &dyn GuestMemoryMut,
        queue: &VirtioNetQueue,
        head: u16,
        descs: &mut Vec<Descriptor>,
        packet: &mut Vec<u8>,
    ) -> bool {
        packet.clear();
        if !Self::descriptor_chain_into(mem, queue, head, descs) {
            return false;
        }
        for desc in descs.iter() {
            if desc.flags & DESC_F_WRITE != 0 {
                return false;
            }
            let start = packet.len();
            let Some(end) = start.checked_add(desc.len as usize) else {
                return false;
            };
            if end > MAX_TX_PACKET_LEN {
                return false;
            }
            let Some(bytes) = mem.read_bytes(desc.addr, desc.len as usize) else {
                return false;
            };
            packet.extend_from_slice(&bytes);
        }
        packet.len() >= VIRTIO_NET_HDR_LEN
    }

    pub(crate) fn deliver_rx_frame(&mut self, frame: &[u8], mem: &mut dyn GuestMemoryMut) -> bool {
        let queue_index = QUEUE_RX;
        let queue = self.queues[queue_index];
        if !queue.ready || queue.size == 0 || queue.desc == 0 {
            return false;
        }
        let Some(avail_idx) = read_u16(mem, queue.driver + 2) else {
            return false;
        };
        if self.queues[queue_index].last_avail_idx == avail_idx {
            return false;
        }
        let last_avail_idx = self.queues[queue_index].last_avail_idx;
        let ring_off = 4 + u64::from(last_avail_idx % queue.size) * 2;
        let Some(head) = read_u16(mem, queue.driver + ring_off) else {
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
