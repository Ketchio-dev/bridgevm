//! The packet path: TX drain to the backend, RX delivery with the virtio-net header, doorbells.

use super::queue_pending::pending_entries;
use super::*;
use crate::fwcfg::GuestMemoryMut;

const DRIVER_OK: u32 = 4;

impl<B: NetBackend> VirtioNet<B> {
    pub(crate) fn notify_queue(&mut self, queue_index: u16, mem: &mut dyn GuestMemoryMut) {
        self.stats.notify_count = self.stats.notify_count.saturating_add(1);
        if usize::from(queue_index) == QUEUE_TX {
            self.process_tx_queue(mem);
        }
    }

    pub fn pump_receive(&mut self, mem: &mut dyn GuestMemoryMut) -> bool {
        if self.status & DRIVER_OK == 0 {
            return false;
        }
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
        if self.status & DRIVER_OK == 0 {
            return;
        }
        let queue_index = QUEUE_TX;
        let queue = self.queues[queue_index];
        if !queue.ready || queue.size == 0 || queue.desc == 0 {
            return;
        }
        let Some(avail_idx) = read_u16(mem, queue.driver, 2) else {
            return;
        };
        let mut descs = std::mem::take(&mut self.descriptor_scratch);
        let mut packet = std::mem::take(&mut self.tx_packet_scratch);
        for _ in 0..pending_entries(queue.last_avail_idx, avail_idx, queue.size) {
            let last_avail_idx = self.queues[queue_index].last_avail_idx;
            let ring_off = 4 + u64::from(last_avail_idx % queue.size) * 2;
            let Some(head) = read_u16(mem, queue.driver, ring_off) else {
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
}
