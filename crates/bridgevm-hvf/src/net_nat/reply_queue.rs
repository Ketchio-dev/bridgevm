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

pub(crate) fn queue_udp_reply(
    reply_queue: &mut VecDeque<Vec<u8>>,
    guest_mac: MacAddr,
    src_ip: Ipv4Addr,
    dst_ip: Ipv4Addr,
    src_port: u16,
    dst_port: u16,
    payload: &[u8],
) {
    let Some(frame) = build_udp_reply_frame(
        guest_mac,
        GATEWAY_MAC,
        src_ip,
        dst_ip,
        src_port,
        dst_port,
        payload,
    ) else {
        return;
    };
    reply_queue.push_back(frame);
}

pub(crate) fn queue_tcp_reply(
    reply_queue: &mut VecDeque<Vec<u8>>,
    guest_mac: MacAddr,
    key: &TcpFlowKey,
    seq: u32,
    ack: u32,
    flags: u8,
    payload: &[u8],
) {
    queue_tcp_segment(
        reply_queue,
        guest_mac,
        key,
        seq,
        ack,
        flags,
        u16::MAX,
        payload,
    );
}

/// `queue_tcp_reply` with a flow's window field.
#[allow(clippy::too_many_arguments)]
pub(crate) fn queue_tcp_segment(
    reply_queue: &mut VecDeque<Vec<u8>>,
    guest_mac: MacAddr,
    key: &TcpFlowKey,
    seq: u32,
    ack: u32,
    flags: u8,
    window: u16,
    payload: &[u8],
) {
    let (source, destination) = tcp_reply_endpoints(guest_mac, key);
    let Some(frame) =
        build_tcp_reply_frame(source, destination, seq, ack, flags, window, &[], payload)
    else {
        return;
    };
    reply_queue.push_back(frame);
}

/// The NAT side of a flow (the remote host behind the gateway MAC), then the guest.
pub(crate) fn tcp_reply_endpoints(
    guest_mac: MacAddr,
    key: &TcpFlowKey,
) -> (EthernetIpv4Endpoint, EthernetIpv4Endpoint) {
    (
        EthernetIpv4Endpoint {
            mac: GATEWAY_MAC,
            network: Ipv4Endpoint::new(key.dst_ip, key.dst_port),
        },
        EthernetIpv4Endpoint {
            mac: guest_mac,
            network: Ipv4Endpoint::new(key.guest_ip, key.guest_port),
        },
    )
}
