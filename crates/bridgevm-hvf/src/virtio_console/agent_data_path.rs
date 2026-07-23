//! The host-to-guest agent byte channel: send buffers, agent RX delivery, agent TX drain.

use super::*;
use crate::fwcfg::GuestMemoryMut;

impl VirtioConsole {
    pub fn agent_send(&mut self, data: &[u8]) {
        self.host_to_guest.extend(data.iter().copied());
        // Every host->guest send is also a retry heartbeat: if the channel has
        // not been confirmed yet, re-assert PORT_OPEN(host) so a driver whose
        // port stabilized after our initial burst still latches HostConnected.
        // Delivered on the next poll()/control-RX notify; self-terminates once
        // the guest sends any TX byte (see maybe_reassert_host_open).
        self.maybe_reassert_host_open();
    }

    pub fn take_inbound(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.host_inbound)
    }

    pub fn drain_inbound_into(&mut self, out: &mut Vec<u8>) {
        out.append(&mut self.host_inbound);
    }

    pub(crate) fn deliver_agent_rx(&mut self, mem: &mut dyn GuestMemoryMut) -> bool {
        // Deliver host->guest bytes whenever the guest has posted receive
        // buffers on the agent RX queue. We intentionally do NOT gate on
        // ports[1].guest_open: the real vioser driver does not reliably emit a
        // guest PORT_OPEN we can observe (its VIOSerialPortCreate can
        // short-circuit), yet it still posts RX buffers once the app opens the
        // port. Gating on an unobservable guest_open deadlocked the channel.
        if self.host_to_guest.is_empty() {
            return false;
        }
        let (front, back) = self.host_to_guest.as_slices();
        let Some(written) = Self::deliver_partial_slices_to_rx_queue(
            RxQueueDeliveryState {
                queues: &mut self.queues,
                pending_msix_queue_bits: &mut self.pending_msix_queue_bits,
                interrupt_status: &mut self.interrupt_status,
                descriptor_scratch: &mut self.descriptor_scratch,
            },
            QUEUE_AGENT_RX,
            front,
            back,
            mem,
        ) else {
            return false;
        };
        self.host_to_guest.drain(..written);
        true
    }

    pub(crate) fn process_agent_tx_queue(&mut self, mem: &mut dyn GuestMemoryMut) -> bool {
        let queue_index = QUEUE_AGENT_TX;
        let queue = self.queues[queue_index];
        if !queue.ready || queue.size == 0 || queue.desc == 0 {
            return false;
        }
        let Some(avail_idx) = read_u16(mem, queue.driver + 2) else {
            return false;
        };
        self.queues[queue_index].last_avail_seen = avail_idx;
        let mut progressed = false;
        let mut descs = std::mem::take(&mut self.descriptor_scratch);
        let mut bytes = std::mem::take(&mut self.read_scratch);
        while self.queues[queue_index].last_avail_idx != avail_idx {
            let last_avail_idx = self.queues[queue_index].last_avail_idx;
            let ring_off = 4 + u64::from(last_avail_idx % queue.size) * 2;
            let Some(head) = read_u16(mem, queue.driver + ring_off) else {
                break;
            };
            if Self::read_chain_into(
                mem,
                &queue,
                head,
                &mut descs,
                &mut bytes,
                MAX_AGENT_MESSAGE_LEN,
            ) {
                // Any guest TX proves vioser latched HostConnected (its
                // WillWriteBlock gate blocks writes until then), so we can stop
                // re-asserting PORT_OPEN from here on.
                if !bytes.is_empty() {
                    self.agent_connected_confirmed = true;
                }
                console_trace!("agent-tx<-guest len={}", bytes.len());
                self.host_inbound.extend_from_slice(&bytes);
            }
            Self::write_used(mem, &queue, head, 0);
            self.queues[queue_index].last_avail_idx = last_avail_idx.wrapping_add(1);
            self.queues[queue_index].used_produced =
                self.queues[queue_index].used_produced.saturating_add(1);
            self.mark_queue_interrupt(queue_index);
            progressed = true;
        }
        descs.clear();
        bytes.clear();
        self.descriptor_scratch = descs;
        self.read_scratch = bytes;
        progressed
    }
}
