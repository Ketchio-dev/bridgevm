//! Queue-doorbell and host-poll dispatch into the per-queue handlers.

use super::*;
use crate::fwcfg::GuestMemoryMut;

impl VirtioConsole {
    pub fn poll(&mut self, mem: &mut dyn GuestMemoryMut) -> bool {
        let mut progressed = false;
        progressed |= self.process_control_tx_queue(mem);
        progressed |= self.flush_pending_control(mem);
        progressed |= self.deliver_agent_rx(mem);
        progressed |= self.process_agent_tx_queue(mem);
        progressed
    }

    pub(crate) fn notify_queue(&mut self, queue_index: u16, mem: &mut dyn GuestMemoryMut) {
        if let Some(queue) = self.queues.get_mut(usize::from(queue_index)) {
            queue.notify_count = queue.notify_count.saturating_add(1);
        }
        if console_trace_enabled() {
            let avail = self
                .queues
                .get(usize::from(queue_index))
                .and_then(|queue| read_u16(mem, queue.driver + 2));
            let last = self
                .queues
                .get(usize::from(queue_index))
                .map(|queue| queue.last_avail_idx);
            eprintln!("[vcon] notify q{queue_index} avail_idx={avail:?} last_consumed={last:?}");
        }
        match usize::from(queue_index) {
            QUEUE_CONTROL_RX => {
                // Only drain what is genuinely queued. We must NOT synthesize a
                // PORT_OPEN re-assert here: delivering one makes vioser consume
                // and refill the control-RX ring, which kicks this very queue,
                // which would re-assert again -> a self-sustaining MSI-X storm
                // that livelocks the guest. Re-assertion is driven only by the
                // bounded triggers (agent_send heartbeat, PORT_READY epoch).
                self.flush_pending_control(mem);
            }
            QUEUE_CONTROL_TX => {
                self.process_control_tx_queue(mem);
                self.flush_pending_control(mem);
            }
            QUEUE_AGENT_RX => {
                self.deliver_agent_rx(mem);
            }
            QUEUE_AGENT_TX => {
                self.process_agent_tx_queue(mem);
            }
            _ => {}
        }
    }
}
