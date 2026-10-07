//! The bounded host-to-guest reply queue.

use super::*;

use std::collections::VecDeque;

/// Frames that may wait for guest RX buffers. QEMU v11.0.0 `net/queue.c`
/// drops a packet once this many are queued (`nq_maxlen`).
pub(crate) const MAX_PENDING_REPLY_FRAMES: usize = 10_000;

/// Guest-to-host TCP bytes buffered while the host peer is not reading.
pub(crate) const MAX_TCP_WRITE_BACKLOG: usize = 4 * 1024 * 1024;

pub(crate) fn reply_queue_full(queue: &VecDeque<Vec<u8>>) -> bool {
    queue.len() >= MAX_PENDING_REPLY_FRAMES
}

impl<H> NatBackend<H> {
    /// Queue a reply synthesized from a guest request (ARP, DHCP, gateway
    /// ICMP). A guest that keeps asking without posting RX buffers loses the
    /// excess replies, as with a full QEMU net queue.
    pub(crate) fn queue_reply(&mut self, frame: Vec<u8>) {
        if !reply_queue_full(&self.reply_queue) {
            self.reply_queue.push_back(frame);
        }
    }
}
