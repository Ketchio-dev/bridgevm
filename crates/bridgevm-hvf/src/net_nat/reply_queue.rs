//! The bounded host-to-guest reply queue.

use super::*;

use std::collections::VecDeque;

/// Frames that may wait for guest RX buffers. QEMU v11.0.0 `net/queue.c`
/// drops a packet once this many are queued (`nq_maxlen`).
pub(crate) const MAX_PENDING_REPLY_FRAMES: usize = 10_000;

/// Guest-to-host TCP bytes buffered while the host peer is not reading.
pub(crate) const MAX_TCP_WRITE_BACKLOG: usize = 4 * 1024 * 1024;

/// A guest RX ring's worth of frames (virtio-net's 256-entry queue). With this
/// many waiting, the next delivery can fill an empty ring, so the host sockets
/// can wait: a sweep costs a syscall per socket on a vCPU thread holding the
/// platform lock (live: about 70 sockets on every exit halved downloads once
/// guest windows kept the queue short).
pub(crate) const REPLY_BACKLOG_SKIPS_SWEEP: usize = 256;

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
