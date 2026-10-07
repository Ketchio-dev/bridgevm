//! The deterministic outbound IPv4 handler used without host sockets.

use super::*;

use std::collections::VecDeque;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct QueuedOutboundIpv4Handler {
    pub(crate) packets: VecDeque<Vec<u8>>,
}

impl QueuedOutboundIpv4Handler {
    pub fn len(&self) -> usize {
        self.packets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.packets.is_empty()
    }

    pub fn pop_front(&mut self) -> Option<Vec<u8>> {
        self.packets.pop_front()
    }

    pub fn packets(&self) -> &VecDeque<Vec<u8>> {
        &self.packets
    }
}

impl OutboundIpv4Handler for QueuedOutboundIpv4Handler {
    fn handle_outbound_ipv4(&mut self, packet: &Ipv4Packet<'_>) {
        self.packets.push_back(packet.bytes.to_vec());
    }
}
