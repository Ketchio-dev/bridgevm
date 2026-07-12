//! Deterministic userspace NAT control plane for virtio-net.
//!
//! Stage 2a is deliberately socket-free: guest Ethernet frames enter through
//! `NetBackend::transmit`, local control-plane replies are queued for
//! `poll_receive`, and non-local TCP/UDP IPv4 packets are handed to the
//! `OutboundIpv4Handler` seam below. Stage 2b can replace the default queued
//! handler with a socket-backed handler without changing the virtio-net device
//! model.

use std::{
    collections::{hash_map::Entry, HashMap, VecDeque},
    io::{self, Read, Write},
    net::{IpAddr, Ipv4Addr as StdIpv4Addr, Shutdown, SocketAddrV4, TcpStream, UdpSocket},
    os::fd::{FromRawFd, RawFd},
    path::Path,
    sync::OnceLock,
};

use crate::virtio_net::NetBackend;

pub type MacAddr = [u8; 6];
pub type Ipv4Addr = [u8; 4];

pub const ETHERTYPE_IPV4: u16 = 0x0800;
pub const ETHERTYPE_ARP: u16 = 0x0806;

pub const IPV4_PROTOCOL_ICMP: u8 = 1;
pub const IPV4_PROTOCOL_TCP: u8 = 6;
pub const IPV4_PROTOCOL_UDP: u8 = 17;

pub const GUEST_IP: Ipv4Addr = [10, 0, 2, 15];
pub const GATEWAY_IP: Ipv4Addr = [10, 0, 2, 2];
pub const DNS_IP: Ipv4Addr = [10, 0, 2, 3];
pub const DHCP_SERVER_IP: Ipv4Addr = GATEWAY_IP;
pub const SUBNET_MASK: Ipv4Addr = [255, 255, 255, 0];
pub const IPV4_BROADCAST: Ipv4Addr = [255, 255, 255, 255];
pub const GUEST_SUBNET_BROADCAST: Ipv4Addr = [10, 0, 2, 255];
pub const GATEWAY_MAC: MacAddr = [0x52, 0x55, 0x0a, 0x00, 0x02, 0x02];

const ARP_HARDWARE_ETHERNET: u16 = 1;
const ARP_OPCODE_REQUEST: u16 = 1;
const ARP_OPCODE_REPLY: u16 = 2;

const DHCP_CLIENT_PORT: u16 = 68;
const DHCP_SERVER_PORT: u16 = 67;
const DHCP_MAGIC_COOKIE: [u8; 4] = [99, 130, 83, 99];
const DHCP_FLAG_BROADCAST: u16 = 0x8000;
const DHCP_OPT_SUBNET_MASK: u8 = 1;
const DHCP_OPT_ROUTER: u8 = 3;
const DHCP_OPT_DNS: u8 = 6;
const DHCP_OPT_LEASE_TIME: u8 = 51;
const DHCP_OPT_MESSAGE_TYPE: u8 = 53;
const DHCP_OPT_SERVER_ID: u8 = 54;
const DHCP_OPT_END: u8 = 255;
const DHCP_DISCOVER: u8 = 1;
const DHCP_OFFER: u8 = 2;
const DHCP_REQUEST: u8 = 3;
const DHCP_ACK: u8 = 5;
const DHCP_LEASE_SECONDS: u32 = 86_400;
const DHCP_REPLY_FIXED_LEN: usize = 240;
const DHCP_REPLY_OPTIONS_LEN: usize = 3 + (5 * 6) + 1;
const DHCP_REPLY_PAYLOAD_LEN: usize = DHCP_REPLY_FIXED_LEN + DHCP_REPLY_OPTIONS_LEN;
const MAX_RESOLV_CONF_BYTES: u64 = 64 * 1024;
#[cfg(test)]
const DHCP_OPT_REQUESTED_IP: u8 = 50;

/// Stage 2b extension seam for internet-bound IPv4 traffic.
///
/// `packet.bytes` is the guest's IPv4 datagram with the Ethernet header
/// already stripped. The stage 2a implementation uses
/// `QueuedOutboundIpv4Handler`, which stores those datagrams for deterministic
/// tests. Stage 2b should implement this trait by translating outbound TCP/UDP
/// flows to host sockets and feeding socket completions back into the NAT
/// receive queue as Ethernet-framed IPv4 packets.
pub trait OutboundIpv4Handler {
    fn handle_outbound_ipv4(&mut self, packet: &Ipv4Packet<'_>);
    fn poll_host_sockets(
        &mut self,
        _guest_mac: Option<MacAddr>,
        _reply_queue: &mut VecDeque<Vec<u8>>,
        _stats: &mut NatStats,
    ) {
    }
    fn active_flow_counts(&self) -> (usize, usize) {
        (0, 0)
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct QueuedOutboundIpv4Handler {
    packets: VecDeque<Vec<u8>>,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NatBackend<H = QueuedOutboundIpv4Handler> {
    guest_mac: Option<MacAddr>,
    reply_queue: VecDeque<Vec<u8>>,
    outbound_ipv4: H,
    stats: NatStats,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NatStats {
    pub guest_frames: u64,
    pub arp_requests: u64,
    pub dhcp_discover: u64,
    pub dhcp_request: u64,
    pub dns_queries: u64,
    pub icmp_echo: u64,
    pub tcp_segments: u64,
    pub udp_datagrams: u64,
    pub other: u64,
    pub arp_replies: u64,
    pub dhcp_offers: u64,
    pub dhcp_acks: u64,
    pub dns_replies: u64,
    pub icmp_replies: u64,
    pub icmp_forwarded: u64,
    pub icmp_external_replies: u64,
    pub tcp_segments_out: u64,
    pub udp_datagrams_out: u64,
    pub dhcp_lease_ip: Ipv4Addr,
    pub tcp_flow_count: usize,
    pub udp_flow_count: usize,
    pub pending_replies: usize,
    pub dropped_malformed_frames: u64,
    pub dropped_no_guest_mac: u64,
    pub udp_recv_again: u64,
    pub tcp_connect_again: u64,
    pub tcp_read_again: u64,
    pub tcp_write_again: u64,
    pub socket_errors: u64,
}

impl Default for NatBackend<QueuedOutboundIpv4Handler> {
    fn default() -> Self {
        Self::new()
    }
}

impl NatBackend<QueuedOutboundIpv4Handler> {
    pub fn new() -> Self {
        Self::with_outbound_handler(QueuedOutboundIpv4Handler::default())
    }

    pub fn poll_outbound_ipv4(&mut self) -> Option<Vec<u8>> {
        self.outbound_ipv4.pop_front()
    }

    pub fn queued_outbound_ipv4_len(&self) -> usize {
        self.outbound_ipv4.len()
    }
}

impl<H> NatBackend<H> {
    pub fn with_outbound_handler(outbound_ipv4: H) -> Self {
        Self {
            guest_mac: None,
            reply_queue: VecDeque::new(),
            outbound_ipv4,
            stats: NatStats::default(),
        }
    }

    pub fn guest_mac(&self) -> Option<MacAddr> {
        self.guest_mac
    }

    pub fn pending_receive_len(&self) -> usize {
        self.reply_queue.len()
    }

    pub fn outbound_ipv4_handler(&self) -> &H {
        &self.outbound_ipv4
    }

    pub fn outbound_ipv4_handler_mut(&mut self) -> &mut H {
        &mut self.outbound_ipv4
    }

    pub fn stats(&self) -> NatStats
    where
        H: OutboundIpv4Handler,
    {
        let (tcp_flow_count, udp_flow_count) = self.outbound_ipv4.active_flow_counts();
        NatStats {
            tcp_flow_count,
            udp_flow_count,
            pending_replies: self.reply_queue.len(),
            ..self.stats
        }
    }
}

impl<H: OutboundIpv4Handler + Send> NetBackend for NatBackend<H> {
    fn transmit(&mut self, frame: &[u8]) {
        self.stats.guest_frames = self.stats.guest_frames.saturating_add(1);
        let Some(eth) = EthernetFrame::parse(frame) else {
            self.stats.dropped_malformed_frames =
                self.stats.dropped_malformed_frames.saturating_add(1);
            return;
        };
        if self.guest_mac.is_none() {
            self.guest_mac = Some(eth.src);
        }

        match eth.ethertype {
            ETHERTYPE_ARP => self.handle_arp(&eth),
            ETHERTYPE_IPV4 => self.handle_ipv4(&eth),
            _ => {
                self.stats.other = self.stats.other.saturating_add(1);
            }
        }
    }

    fn poll_receive(&mut self) -> Option<Vec<u8>> {
        self.reply_queue.pop_front()
    }

    fn poll_receive_into(&mut self, out: &mut Vec<u8>) -> bool {
        let Some(mut frame) = self.reply_queue.pop_front() else {
            return false;
        };
        out.clear();
        out.append(&mut frame);
        true
    }

    fn poll_host_sockets(&mut self) {
        self.outbound_ipv4.poll_host_sockets(
            self.guest_mac,
            &mut self.reply_queue,
            &mut self.stats,
        );
    }
}

impl<H: OutboundIpv4Handler> NatBackend<H> {
    fn handle_arp(&mut self, eth: &EthernetFrame<'_>) {
        let Some(request) = ArpPacket::parse(eth.payload) else {
            self.stats.dropped_malformed_frames =
                self.stats.dropped_malformed_frames.saturating_add(1);
            return;
        };
        if request.opcode != ARP_OPCODE_REQUEST {
            self.stats.other = self.stats.other.saturating_add(1);
            return;
        }
        self.stats.arp_requests = self.stats.arp_requests.saturating_add(1);
        if request.target_ip != GATEWAY_IP && request.target_ip != DNS_IP {
            return;
        }
        let reply_ip = request.target_ip;

        let Some(dst_mac) = self.guest_mac else {
            self.stats.dropped_no_guest_mac = self.stats.dropped_no_guest_mac.saturating_add(1);
            return;
        };
        self.reply_queue.push_back(build_arp_reply_frame(
            dst_mac,
            GATEWAY_MAC,
            reply_ip,
            request.sender_mac,
            request.sender_ip,
        ));
        self.stats.arp_replies = self.stats.arp_replies.saturating_add(1);
    }

    fn handle_ipv4(&mut self, eth: &EthernetFrame<'_>) {
        let Some(packet) = Ipv4Packet::parse(eth.payload) else {
            self.stats.dropped_malformed_frames =
                self.stats.dropped_malformed_frames.saturating_add(1);
            return;
        };

        if packet.protocol == IPV4_PROTOCOL_UDP {
            let Some(udp) = UdpDatagram::parse(packet.payload) else {
                self.stats.dropped_malformed_frames =
                    self.stats.dropped_malformed_frames.saturating_add(1);
                return;
            };
            if udp.src_port == DHCP_CLIENT_PORT && udp.dst_port == DHCP_SERVER_PORT {
                self.handle_dhcp(&packet, &udp);
                return;
            }
            if packet.dst == DNS_IP && udp.dst_port == 53 {
                self.stats.dns_queries = self.stats.dns_queries.saturating_add(1);
            } else {
                self.stats.udp_datagrams = self.stats.udp_datagrams.saturating_add(1);
            }
            if (packet.dst == DNS_IP && udp.dst_port == 53)
                || is_non_local_ipv4_destination(packet.dst)
            {
                self.outbound_ipv4.handle_outbound_ipv4(&packet);
            } else {
                self.stats.other = self.stats.other.saturating_add(1);
            }
            return;
        }

        if packet.protocol == IPV4_PROTOCOL_ICMP {
            self.handle_icmp(&packet);
            return;
        }

        if packet.protocol == IPV4_PROTOCOL_TCP && is_non_local_ipv4_destination(packet.dst) {
            self.stats.tcp_segments = self.stats.tcp_segments.saturating_add(1);
            self.outbound_ipv4.handle_outbound_ipv4(&packet);
        } else {
            self.stats.other = self.stats.other.saturating_add(1);
        }
    }

    fn handle_dhcp(&mut self, packet: &Ipv4Packet<'_>, udp: &UdpDatagram<'_>) {
        let Some(request) = DhcpRequest::parse(udp.payload) else {
            return;
        };
        let reply_type = match request.message_type {
            DHCP_DISCOVER => {
                self.stats.dhcp_discover = self.stats.dhcp_discover.saturating_add(1);
                DHCP_OFFER
            }
            DHCP_REQUEST => {
                self.stats.dhcp_request = self.stats.dhcp_request.saturating_add(1);
                DHCP_ACK
            }
            _ => return,
        };

        let dst_ip = dhcp_reply_destination(&request);
        let Some(dst_mac) = self.guest_mac else {
            self.stats.dropped_no_guest_mac = self.stats.dropped_no_guest_mac.saturating_add(1);
            return;
        };
        let Some(frame) = build_dhcp_reply_frame(
            dst_mac,
            GATEWAY_MAC,
            dst_ip,
            &request,
            reply_type,
            packet.identification,
        ) else {
            return;
        };
        self.reply_queue.push_back(frame);
        self.stats.dhcp_lease_ip = GUEST_IP;
        if reply_type == DHCP_OFFER {
            self.stats.dhcp_offers = self.stats.dhcp_offers.saturating_add(1);
        } else {
            self.stats.dhcp_acks = self.stats.dhcp_acks.saturating_add(1);
        }
    }

    fn handle_icmp(&mut self, packet: &Ipv4Packet<'_>) {
        match classify_icmp_echo(packet.dst, packet.payload) {
            IcmpEchoRoute::Gateway => {
                self.stats.icmp_echo = self.stats.icmp_echo.saturating_add(1);
                if self.queue_gateway_icmp_echo_reply(packet) {
                    self.stats.icmp_replies = self.stats.icmp_replies.saturating_add(1);
                }
            }
            IcmpEchoRoute::External => {
                self.stats.icmp_echo = self.stats.icmp_echo.saturating_add(1);
                self.stats.icmp_forwarded = self.stats.icmp_forwarded.saturating_add(1);
                self.outbound_ipv4.handle_outbound_ipv4(packet);
            }
            IcmpEchoRoute::Other => {
                self.stats.other = self.stats.other.saturating_add(1);
            }
        }
    }

    fn queue_gateway_icmp_echo_reply(&mut self, packet: &Ipv4Packet<'_>) -> bool {
        let Some(dst_mac) = self.guest_mac else {
            self.stats.dropped_no_guest_mac = self.stats.dropped_no_guest_mac.saturating_add(1);
            return false;
        };
        let Some(frame) = build_icmp_echo_reply_frame(
            dst_mac,
            GATEWAY_MAC,
            GATEWAY_IP,
            packet.src,
            packet.payload,
            packet.identification,
        ) else {
            return false;
        };
        self.reply_queue.push_back(frame);
        true
    }
}

impl NatBackend<HostSocketOutboundIpv4Handler> {
    pub fn new_host_socket() -> Self {
        Self::with_outbound_handler(HostSocketOutboundIpv4Handler::new())
    }
}

#[derive(Debug)]
pub struct HostSocketOutboundIpv4Handler {
    udp_flows: HashMap<UdpFlowKey, UdpFlow>,
    tcp_flows: HashMap<TcpFlowKey, TcpFlow>,
    icmp_flows: HashMap<IcmpFlowKey, IcmpFlow>,
    pending_tcp_resets: VecDeque<PendingTcpReset>,
    tcp_remove_scratch: Vec<TcpFlowKey>,
    udp_recv_scratch: [u8; HOST_SOCKET_UDP_RECV_SCRATCH_LEN],
    tcp_read_scratch: [u8; HOST_SOCKET_TCP_READ_SCRATCH_LEN],
    icmp_recv_scratch: [u8; HOST_SOCKET_ICMP_RECV_SCRATCH_LEN],
    pending_socket_errors: u64,
    dns_resolver: StdIpv4Addr,
    tick: u64,
    idle_timeout_ticks: u64,
    max_flows: usize,
    max_icmp_flows: usize,
    tcp_isn_counter: u32,
}

const HOST_SOCKET_UDP_RECV_SCRATCH_LEN: usize = 2048;
const HOST_SOCKET_TCP_READ_SCRATCH_LEN: usize = 1460;
const HOST_SOCKET_ICMP_RECV_SCRATCH_LEN: usize = 2048;

impl Default for HostSocketOutboundIpv4Handler {
    fn default() -> Self {
        Self::new()
    }
}

impl HostSocketOutboundIpv4Handler {
    const DEFAULT_IDLE_TIMEOUT_TICKS: u64 = 30_000;
    const DEFAULT_MAX_FLOWS: usize = 256;
    const DEFAULT_MAX_ICMP_FLOWS: usize = 32;
    const MAX_ICMP_RECV_PER_POLL: usize = 64;

    pub fn new() -> Self {
        Self::with_dns_resolver(
            first_resolv_conf_nameserver().unwrap_or(StdIpv4Addr::new(1, 1, 1, 1)),
        )
    }

    pub fn with_dns_resolver(dns_resolver: StdIpv4Addr) -> Self {
        Self {
            udp_flows: HashMap::new(),
            tcp_flows: HashMap::new(),
            icmp_flows: HashMap::new(),
            pending_tcp_resets: VecDeque::new(),
            tcp_remove_scratch: Vec::new(),
            udp_recv_scratch: [0; HOST_SOCKET_UDP_RECV_SCRATCH_LEN],
            tcp_read_scratch: [0; HOST_SOCKET_TCP_READ_SCRATCH_LEN],
            icmp_recv_scratch: [0; HOST_SOCKET_ICMP_RECV_SCRATCH_LEN],
            pending_socket_errors: 0,
            dns_resolver,
            tick: 0,
            idle_timeout_ticks: Self::DEFAULT_IDLE_TIMEOUT_TICKS,
            max_flows: Self::DEFAULT_MAX_FLOWS,
            max_icmp_flows: Self::DEFAULT_MAX_ICMP_FLOWS,
            tcp_isn_counter: 0x4256_0000,
        }
    }

    #[cfg(test)]
    fn with_idle_timeout_ticks(mut self, ticks: u64) -> Self {
        self.idle_timeout_ticks = ticks;
        self
    }

    #[cfg(test)]
    fn udp_flow_count(&self) -> usize {
        self.udp_flows.len()
    }

    fn bump_tick(&mut self) -> u64 {
        self.tick = self.tick.saturating_add(1);
        self.tick
    }

    fn next_tcp_isn(&mut self) -> u32 {
        self.tcp_isn_counter = self.tcp_isn_counter.wrapping_add(0x1f3d_5b79);
        self.tcp_isn_counter
    }

    fn evict_idle_flows(&mut self) {
        let now = self.tick;
        let timeout = self.idle_timeout_ticks;
        self.udp_flows
            .retain(|_, flow| now.saturating_sub(flow.last_activity) <= timeout);
        self.tcp_flows
            .retain(|_, flow| now.saturating_sub(flow.last_activity) <= timeout);
        self.icmp_flows
            .retain(|_, flow| now.saturating_sub(flow.last_activity) <= timeout);
        evict_lru(&mut self.udp_flows, self.max_flows);
        evict_lru(&mut self.tcp_flows, self.max_flows);
        evict_lru(&mut self.icmp_flows, self.max_icmp_flows);
    }

    fn get_or_create_icmp_flow(
        &mut self,
        key: IcmpFlowKey,
        guest_ip: Ipv4Addr,
        now: u64,
    ) -> io::Result<&mut IcmpFlow> {
        get_or_insert_lru(&mut self.icmp_flows, key, self.max_icmp_flows, || {
            RawIcmpSocket::new_nonblocking().map(|socket| IcmpFlow {
                socket,
                guest_ip,
                last_activity: now,
            })
        })?
        .ok_or_else(|| io::Error::other("icmp flow missing after insert"))
    }

    fn handle_icmp(&mut self, packet: &Ipv4Packet<'_>) -> io::Result<()> {
        let now = self.bump_tick();
        let guest_identifier = read_u16_be(packet.payload, 4)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "short icmp echo"))?;
        let key = IcmpFlowKey {
            guest_identifier,
            dst_ip: packet.dst,
        };
        let flow = self.get_or_create_icmp_flow(key, packet.src, now)?;
        flow.guest_ip = packet.src;
        flow.last_activity = now;
        flow.socket
            .send_to(packet.payload, StdIpv4Addr::from(packet.dst))
            .inspect(|bytes_sent| {
                if trace_icmp_enabled() {
                    eprintln!(
                        "bridgevm icmp forward dst={} guest_id=0x{guest_identifier:04x} bytes={bytes_sent}",
                        StdIpv4Addr::from(packet.dst),
                    );
                }
            })?;
        self.evict_idle_flows();
        Ok(())
    }

    fn handle_udp(&mut self, packet: &Ipv4Packet<'_>, udp: &UdpDatagram<'_>) {
        let now = self.bump_tick();
        let mut public_dst = packet.dst;
        let mut socket_dst = packet.dst;
        let mut dst_port = udp.dst_port;
        if packet.dst == DNS_IP && udp.dst_port == 53 {
            public_dst = DNS_IP;
            socket_dst = self.dns_resolver.octets();
            dst_port = 53;
        }
        let key = UdpFlowKey {
            guest_ip: packet.src,
            guest_port: udp.src_port,
            public_dst,
            public_dst_port: udp.dst_port,
            socket_dst,
            socket_dst_port: dst_port,
        };
        let flow = match self.udp_flows.entry(key) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => {
                let socket = match UdpSocket::bind(SocketAddrV4::new(StdIpv4Addr::UNSPECIFIED, 0))
                    .and_then(|socket| {
                        socket.set_nonblocking(true)?;
                        socket
                            .connect(SocketAddrV4::new(StdIpv4Addr::from(socket_dst), dst_port))?;
                        Ok(socket)
                    }) {
                    Ok(socket) => socket,
                    Err(_) => return,
                };
                entry.insert(UdpFlow {
                    socket,
                    last_activity: now,
                })
            }
        };
        flow.last_activity = now;
        let _ = flow.socket.send(udp.payload);
        self.evict_idle_flows();
    }

    fn handle_tcp(&mut self, packet: &Ipv4Packet<'_>, tcp: &TcpSegment<'_>) {
        let now = self.bump_tick();
        let key = TcpFlowKey {
            guest_ip: packet.src,
            guest_port: tcp.src_port,
            dst_ip: packet.dst,
            dst_port: tcp.dst_port,
        };
        if tcp.flags & TCP_FLAG_RST != 0 {
            self.tcp_flows.remove(&key);
            return;
        }
        if tcp.flags & TCP_FLAG_SYN != 0 && !self.tcp_flows.contains_key(&key) {
            let our_seq = self.next_tcp_isn();
            let stream = match nonblocking_tcp_connect(StdIpv4Addr::from(packet.dst), tcp.dst_port)
            {
                Ok(stream) => stream,
                Err(_) => {
                    self.pending_tcp_resets.push_back(PendingTcpReset {
                        key,
                        seq: our_seq,
                        ack: tcp.seq.wrapping_add(1),
                    });
                    return;
                }
            };
            self.tcp_flows.insert(
                key,
                TcpFlow::new(stream, tcp.seq.wrapping_add(1), our_seq, now),
            );
            self.evict_idle_flows();
            return;
        }
        let Some(flow) = self.tcp_flows.get_mut(&key) else {
            return;
        };
        flow.last_activity = now;
        if tcp.flags & TCP_FLAG_ACK != 0 {
            flow.observe_guest_ack(tcp.ack);
        }
        if !tcp.payload.is_empty() && tcp.seq == flow.guest_next {
            flow.guest_next = flow.guest_next.wrapping_add(tcp.payload.len() as u32);
            flow.write_buf.extend(tcp.payload);
            flow.pending_ack = true;
            flow.flush_host_write();
            // Out-of-order payload is intentionally dropped; the guest TCP stack
            // will retransmit from the last ACKed byte.
        }
        if tcp.flags & TCP_FLAG_FIN != 0 && tcp.seq == flow.guest_next {
            flow.guest_next = flow.guest_next.wrapping_add(1);
            flow.guest_fin = true;
            flow.pending_ack = true;
            let _ = flow.stream.shutdown(Shutdown::Write);
        }
        self.evict_idle_flows();
    }

    fn poll_udp(
        &mut self,
        guest_mac: Option<MacAddr>,
        reply_queue: &mut VecDeque<Vec<u8>>,
        stats: &mut NatStats,
    ) {
        let Some(guest_mac) = guest_mac else {
            return;
        };
        let now = self.tick;
        let recv_scratch = &mut self.udp_recv_scratch;
        for (key, flow) in &mut self.udp_flows {
            loop {
                match flow.socket.recv(recv_scratch) {
                    Ok(len) => {
                        flow.last_activity = now;
                        queue_udp_reply(
                            reply_queue,
                            guest_mac,
                            key.public_dst,
                            key.guest_ip,
                            key.public_dst_port,
                            key.guest_port,
                            &recv_scratch[..len],
                        );
                        if key.public_dst == DNS_IP && key.public_dst_port == 53 {
                            stats.dns_replies = stats.dns_replies.saturating_add(1);
                        } else {
                            stats.udp_datagrams_out = stats.udp_datagrams_out.saturating_add(1);
                        }
                    }
                    Err(e) if would_block(&e) => {
                        stats.udp_recv_again = stats.udp_recv_again.saturating_add(1);
                        break;
                    }
                    Err(_) => {
                        stats.socket_errors = stats.socket_errors.saturating_add(1);
                        break;
                    }
                }
            }
        }
    }

    fn poll_tcp(
        &mut self,
        guest_mac: Option<MacAddr>,
        reply_queue: &mut VecDeque<Vec<u8>>,
        stats: &mut NatStats,
    ) {
        let Some(guest_mac) = guest_mac else {
            return;
        };
        while let Some(reset) = self.pending_tcp_resets.pop_front() {
            queue_tcp_reply(
                reply_queue,
                guest_mac,
                &reset.key,
                reset.seq,
                reset.ack,
                TCP_FLAG_RST | TCP_FLAG_ACK,
                &[],
            );
            stats.tcp_segments_out = stats.tcp_segments_out.saturating_add(1);
        }
        let now = self.tick;
        {
            let remove_scratch = &mut self.tcp_remove_scratch;
            let read_scratch = &mut self.tcp_read_scratch;
            remove_scratch.clear();
            for (key, flow) in &mut self.tcp_flows {
                flow.last_activity = now;
                if flow.state == TcpProxyState::Connecting {
                    match tcp_connect_error(&flow.stream) {
                        Ok(Some(0)) => {
                            flow.state = TcpProxyState::Established;
                            queue_tcp_reply(
                                reply_queue,
                                guest_mac,
                                key,
                                flow.our_seq,
                                flow.guest_next,
                                TCP_FLAG_SYN | TCP_FLAG_ACK,
                                &[],
                            );
                            stats.tcp_segments_out = stats.tcp_segments_out.saturating_add(1);
                        }
                        Ok(Some(_)) | Err(_) => {
                            queue_tcp_reply(
                                reply_queue,
                                guest_mac,
                                key,
                                flow.our_seq,
                                flow.guest_next,
                                TCP_FLAG_RST | TCP_FLAG_ACK,
                                &[],
                            );
                            stats.tcp_segments_out = stats.tcp_segments_out.saturating_add(1);
                            stats.socket_errors = stats.socket_errors.saturating_add(1);
                            remove_scratch.push(*key);
                            continue;
                        }
                        Ok(None) => {
                            stats.tcp_connect_again = stats.tcp_connect_again.saturating_add(1);
                        }
                    }
                }
                if flow.state != TcpProxyState::Connecting {
                    flow.flush_host_write();
                    if flow.pending_ack {
                        queue_tcp_reply(
                            reply_queue,
                            guest_mac,
                            key,
                            flow.our_next,
                            flow.guest_next,
                            TCP_FLAG_ACK,
                            &[],
                        );
                        stats.tcp_segments_out = stats.tcp_segments_out.saturating_add(1);
                        flow.pending_ack = false;
                    }
                    loop {
                        match flow.stream.read(read_scratch) {
                            Ok(0) => {
                                if !flow.host_fin_sent {
                                    queue_tcp_reply(
                                        reply_queue,
                                        guest_mac,
                                        key,
                                        flow.our_next,
                                        flow.guest_next,
                                        TCP_FLAG_FIN | TCP_FLAG_ACK,
                                        &[],
                                    );
                                    stats.tcp_segments_out =
                                        stats.tcp_segments_out.saturating_add(1);
                                    flow.our_next = flow.our_next.wrapping_add(1);
                                    flow.host_fin_sent = true;
                                }
                                break;
                            }
                            Ok(len) => {
                                queue_tcp_reply(
                                    reply_queue,
                                    guest_mac,
                                    key,
                                    flow.our_next,
                                    flow.guest_next,
                                    TCP_FLAG_PSH | TCP_FLAG_ACK,
                                    &read_scratch[..len],
                                );
                                stats.tcp_segments_out = stats.tcp_segments_out.saturating_add(1);
                                flow.our_next = flow.our_next.wrapping_add(len as u32);
                            }
                            Err(e) if would_block(&e) => {
                                stats.tcp_read_again = stats.tcp_read_again.saturating_add(1);
                                break;
                            }
                            Err(_) => {
                                queue_tcp_reply(
                                    reply_queue,
                                    guest_mac,
                                    key,
                                    flow.our_next,
                                    flow.guest_next,
                                    TCP_FLAG_RST | TCP_FLAG_ACK,
                                    &[],
                                );
                                stats.tcp_segments_out = stats.tcp_segments_out.saturating_add(1);
                                stats.socket_errors = stats.socket_errors.saturating_add(1);
                                remove_scratch.push(*key);
                                break;
                            }
                        }
                    }
                }
                if flow.closed() {
                    remove_scratch.push(*key);
                }
            }
        }
        for key in self.tcp_remove_scratch.drain(..) {
            self.tcp_flows.remove(&key);
        }
    }

    fn poll_icmp(
        &mut self,
        guest_mac: Option<MacAddr>,
        reply_queue: &mut VecDeque<Vec<u8>>,
        stats: &mut NatStats,
    ) {
        let Some(guest_mac) = guest_mac else {
            return;
        };
        let now = self.tick;
        let recv_scratch = &mut self.icmp_recv_scratch;
        for (key, flow) in &mut self.icmp_flows {
            for _ in 0..Self::MAX_ICMP_RECV_PER_POLL {
                match flow.socket.recv_from(recv_scratch) {
                    Ok((len, _)) => {
                        flow.last_activity = now;
                        let recv = &recv_scratch[..len];
                        let first_byte = recv.first().copied().unwrap_or(0);
                        let trace_icmp = trace_icmp_enabled();
                        if let Some(frame) = build_rewritten_icmp_echo_reply_frame(
                            guest_mac,
                            GATEWAY_MAC,
                            key.dst_ip,
                            flow.guest_ip,
                            recv,
                            key.guest_identifier,
                        ) {
                            if trace_icmp {
                                trace_icmp_recv(len, first_byte, "accepted");
                            }
                            reply_queue.push_back(frame);
                            stats.icmp_external_replies =
                                stats.icmp_external_replies.saturating_add(1);
                        } else if trace_icmp {
                            trace_icmp_recv(len, first_byte, icmp_reply_rejection_reason(recv));
                        }
                    }
                    Err(e) if would_block(&e) => break,
                    Err(_) => {
                        stats.socket_errors = stats.socket_errors.saturating_add(1);
                        break;
                    }
                }
            }
        }
    }
}

impl OutboundIpv4Handler for HostSocketOutboundIpv4Handler {
    fn handle_outbound_ipv4(&mut self, packet: &Ipv4Packet<'_>) {
        match packet.protocol {
            IPV4_PROTOCOL_UDP => {
                if let Some(udp) = UdpDatagram::parse(packet.payload) {
                    self.handle_udp(packet, &udp);
                }
            }
            IPV4_PROTOCOL_TCP => {
                if let Some(tcp) = TcpSegment::parse(packet.payload) {
                    self.handle_tcp(packet, &tcp);
                }
            }
            IPV4_PROTOCOL_ICMP => {
                if self.handle_icmp(packet).is_err() {
                    self.pending_socket_errors = self.pending_socket_errors.saturating_add(1);
                }
            }
            _ => {}
        }
    }

    fn poll_host_sockets(
        &mut self,
        guest_mac: Option<MacAddr>,
        reply_queue: &mut VecDeque<Vec<u8>>,
        stats: &mut NatStats,
    ) {
        if self.pending_socket_errors != 0 {
            stats.socket_errors = stats
                .socket_errors
                .saturating_add(self.pending_socket_errors);
            self.pending_socket_errors = 0;
        }
        self.bump_tick();
        self.poll_udp(guest_mac, reply_queue, stats);
        self.poll_tcp(guest_mac, reply_queue, stats);
        self.poll_icmp(guest_mac, reply_queue, stats);
        self.evict_idle_flows();
    }

    fn active_flow_counts(&self) -> (usize, usize) {
        (self.tcp_flows.len(), self.udp_flows.len())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct UdpFlowKey {
    guest_ip: Ipv4Addr,
    guest_port: u16,
    public_dst: Ipv4Addr,
    public_dst_port: u16,
    socket_dst: Ipv4Addr,
    socket_dst_port: u16,
}

#[derive(Debug)]
struct UdpFlow {
    socket: UdpSocket,
    last_activity: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct IcmpFlowKey {
    guest_identifier: u16,
    dst_ip: Ipv4Addr,
}

#[derive(Debug)]
struct IcmpFlow {
    socket: RawIcmpSocket,
    guest_ip: Ipv4Addr,
    last_activity: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct TcpFlowKey {
    guest_ip: Ipv4Addr,
    guest_port: u16,
    dst_ip: Ipv4Addr,
    dst_port: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PendingTcpReset {
    key: TcpFlowKey,
    seq: u32,
    ack: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TcpProxyState {
    Connecting,
    Established,
}

#[derive(Debug)]
struct TcpFlow {
    stream: TcpStream,
    state: TcpProxyState,
    guest_next: u32,
    our_seq: u32,
    our_next: u32,
    write_buf: VecDeque<u8>,
    pending_ack: bool,
    guest_fin: bool,
    host_fin_sent: bool,
    host_fin_acked: bool,
    last_activity: u64,
}

impl TcpFlow {
    fn new(stream: TcpStream, guest_next: u32, our_seq: u32, last_activity: u64) -> Self {
        Self {
            stream,
            state: TcpProxyState::Connecting,
            guest_next,
            our_seq,
            our_next: our_seq.wrapping_add(1),
            write_buf: VecDeque::new(),
            pending_ack: false,
            guest_fin: false,
            host_fin_sent: false,
            host_fin_acked: false,
            last_activity,
        }
    }

    fn observe_guest_ack(&mut self, ack: u32) {
        if self.host_fin_sent && ack == self.our_next {
            self.host_fin_acked = true;
        }
    }

    fn flush_host_write(&mut self) {
        while !self.write_buf.is_empty() {
            let contiguous = self.write_buf.make_contiguous();
            match self.stream.write(contiguous) {
                Ok(0) => break,
                Ok(len) => {
                    self.write_buf.drain(..len);
                }
                Err(e) if would_block(&e) => break,
                Err(_) => break,
            }
        }
    }

    fn closed(&self) -> bool {
        self.guest_fin && self.host_fin_sent && self.host_fin_acked
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TcpSegment<'a> {
    pub src_port: u16,
    pub dst_port: u16,
    pub seq: u32,
    pub ack: u32,
    pub flags: u8,
    pub window: u16,
    pub segment: &'a [u8],
    pub payload: &'a [u8],
}

const TCP_FLAG_FIN: u8 = 0x01;
const TCP_FLAG_SYN: u8 = 0x02;
const TCP_FLAG_RST: u8 = 0x04;
const TCP_FLAG_PSH: u8 = 0x08;
const TCP_FLAG_ACK: u8 = 0x10;

impl<'a> TcpSegment<'a> {
    pub fn parse(bytes: &'a [u8]) -> Option<Self> {
        if bytes.len() < 20 {
            return None;
        }
        let data_offset = usize::from(bytes[12] >> 4) * 4;
        if data_offset < 20 || data_offset > bytes.len() {
            return None;
        }
        Some(Self {
            src_port: read_u16_be(bytes, 0)?,
            dst_port: read_u16_be(bytes, 2)?,
            seq: u32::from_be_bytes(read_array(bytes, 4)?),
            ack: u32::from_be_bytes(read_array(bytes, 8)?),
            flags: bytes[13],
            window: read_u16_be(bytes, 14)?,
            segment: bytes,
            payload: &bytes[data_offset..],
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EthernetFrame<'a> {
    pub dst: MacAddr,
    pub src: MacAddr,
    pub ethertype: u16,
    pub payload: &'a [u8],
}

impl<'a> EthernetFrame<'a> {
    pub fn parse(frame: &'a [u8]) -> Option<Self> {
        Some(Self {
            dst: read_array(frame, 0)?,
            src: read_array(frame, 6)?,
            ethertype: read_u16_be(frame, 12)?,
            payload: frame.get(14..)?,
        })
    }

    pub fn build(dst: MacAddr, src: MacAddr, ethertype: u16, payload: &[u8]) -> Vec<u8> {
        let mut frame = Vec::with_capacity(14 + payload.len());
        frame.extend_from_slice(&dst);
        frame.extend_from_slice(&src);
        frame.extend_from_slice(&ethertype.to_be_bytes());
        frame.extend_from_slice(payload);
        frame
    }
}

fn build_arp_reply_frame(
    dst_mac: MacAddr,
    src_mac: MacAddr,
    sender_ip: Ipv4Addr,
    target_mac: MacAddr,
    target_ip: Ipv4Addr,
) -> Vec<u8> {
    let mut frame = Vec::with_capacity(42);
    frame.extend_from_slice(&dst_mac);
    frame.extend_from_slice(&src_mac);
    frame.extend_from_slice(&ETHERTYPE_ARP.to_be_bytes());
    frame.extend_from_slice(&ARP_HARDWARE_ETHERNET.to_be_bytes());
    frame.extend_from_slice(&ETHERTYPE_IPV4.to_be_bytes());
    frame.push(6);
    frame.push(4);
    frame.extend_from_slice(&ARP_OPCODE_REPLY.to_be_bytes());
    frame.extend_from_slice(&src_mac);
    frame.extend_from_slice(&sender_ip);
    frame.extend_from_slice(&target_mac);
    frame.extend_from_slice(&target_ip);
    frame
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ArpPacket {
    opcode: u16,
    sender_mac: MacAddr,
    sender_ip: Ipv4Addr,
    target_ip: Ipv4Addr,
}

impl ArpPacket {
    fn parse(payload: &[u8]) -> Option<Self> {
        if read_u16_be(payload, 0)? != ARP_HARDWARE_ETHERNET
            || read_u16_be(payload, 2)? != ETHERTYPE_IPV4
            || *payload.get(4)? != 6
            || *payload.get(5)? != 4
        {
            return None;
        }

        Some(Self {
            opcode: read_u16_be(payload, 6)?,
            sender_mac: read_array(payload, 8)?,
            sender_ip: read_array(payload, 14)?,
            target_ip: read_array(payload, 24)?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ipv4Packet<'a> {
    pub bytes: &'a [u8],
    pub header_len: usize,
    pub total_len: usize,
    pub identification: u16,
    pub flags_fragment: u16,
    pub ttl: u8,
    pub protocol: u8,
    pub src: Ipv4Addr,
    pub dst: Ipv4Addr,
    pub payload: &'a [u8],
}

impl<'a> Ipv4Packet<'a> {
    pub fn parse(bytes: &'a [u8]) -> Option<Self> {
        let version_ihl = *bytes.first()?;
        if version_ihl >> 4 != 4 {
            return None;
        }
        let header_len = usize::from(version_ihl & 0x0f) * 4;
        if header_len < 20 || bytes.len() < header_len {
            return None;
        }
        let total_len = usize::from(read_u16_be(bytes, 2)?);
        if total_len < header_len || total_len > bytes.len() {
            return None;
        }

        Some(Self {
            bytes: &bytes[..total_len],
            header_len,
            total_len,
            identification: read_u16_be(bytes, 4)?,
            flags_fragment: read_u16_be(bytes, 6)?,
            ttl: *bytes.get(8)?,
            protocol: *bytes.get(9)?,
            src: read_array(bytes, 12)?,
            dst: read_array(bytes, 16)?,
            payload: &bytes[header_len..total_len],
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UdpDatagram<'a> {
    pub src_port: u16,
    pub dst_port: u16,
    pub length: u16,
    pub segment: &'a [u8],
    pub payload: &'a [u8],
}

impl<'a> UdpDatagram<'a> {
    pub fn parse(bytes: &'a [u8]) -> Option<Self> {
        if bytes.len() < 8 {
            return None;
        }
        let length = read_u16_be(bytes, 4)?;
        let length_usize = usize::from(length);
        if length_usize < 8 || length_usize > bytes.len() {
            return None;
        }
        Some(Self {
            src_port: read_u16_be(bytes, 0)?,
            dst_port: read_u16_be(bytes, 2)?,
            length,
            segment: &bytes[..length_usize],
            payload: &bytes[8..length_usize],
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DhcpRequest {
    htype: u8,
    hlen: u8,
    xid: [u8; 4],
    flags: u16,
    ciaddr: Ipv4Addr,
    chaddr: [u8; 16],
    message_type: u8,
}

impl DhcpRequest {
    fn parse(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 240 || bytes[0] != 1 || bytes[236..240] != DHCP_MAGIC_COOKIE {
            return None;
        }
        let message_type = dhcp_option(&bytes[240..], DHCP_OPT_MESSAGE_TYPE)
            .and_then(|value| value.first())
            .copied()?;

        Some(Self {
            htype: bytes[1],
            hlen: bytes[2],
            xid: read_array(bytes, 4)?,
            flags: read_u16_be(bytes, 10)?,
            ciaddr: read_array(bytes, 12)?,
            chaddr: read_array(bytes, 28)?,
            message_type,
        })
    }
}

pub fn build_ipv4_packet(src: Ipv4Addr, dst: Ipv4Addr, protocol: u8, payload: &[u8]) -> Vec<u8> {
    build_ipv4_packet_with_id(src, dst, protocol, payload, 0)
}

fn build_ipv4_packet_with_id(
    src: Ipv4Addr,
    dst: Ipv4Addr,
    protocol: u8,
    payload: &[u8],
    identification: u16,
) -> Vec<u8> {
    let total_len = 20usize
        .checked_add(payload.len())
        .and_then(|len| u16::try_from(len).ok())
        .expect("IPv4 payload is too large");

    let mut packet = Vec::with_capacity(usize::from(total_len));
    packet.push(0x45);
    packet.push(0);
    packet.extend_from_slice(&total_len.to_be_bytes());
    packet.extend_from_slice(&identification.to_be_bytes());
    packet.extend_from_slice(&0u16.to_be_bytes());
    packet.push(64);
    packet.push(protocol);
    packet.extend_from_slice(&0u16.to_be_bytes());
    packet.extend_from_slice(&src);
    packet.extend_from_slice(&dst);
    let checksum = ipv4_header_checksum(&packet);
    packet[10..12].copy_from_slice(&checksum.to_be_bytes());
    packet.extend_from_slice(payload);
    packet
}

fn build_icmp_echo_reply_frame(
    dst_mac: MacAddr,
    src_mac: MacAddr,
    src_ip: Ipv4Addr,
    dst_ip: Ipv4Addr,
    request_payload: &[u8],
    identification: u16,
) -> Option<Vec<u8>> {
    if request_payload.len() < 8 {
        return None;
    }
    let ipv4_len = 20usize.checked_add(request_payload.len())?;
    let total_len = u16::try_from(ipv4_len).ok()?;
    let mut frame = Vec::with_capacity(14 + ipv4_len);
    frame.extend_from_slice(&dst_mac);
    frame.extend_from_slice(&src_mac);
    frame.extend_from_slice(&ETHERTYPE_IPV4.to_be_bytes());
    frame.push(0x45);
    frame.push(0);
    frame.extend_from_slice(&total_len.to_be_bytes());
    frame.extend_from_slice(&identification.to_be_bytes());
    frame.extend_from_slice(&0u16.to_be_bytes());
    frame.push(64);
    frame.push(IPV4_PROTOCOL_ICMP);
    frame.extend_from_slice(&0u16.to_be_bytes());
    frame.extend_from_slice(&src_ip);
    frame.extend_from_slice(&dst_ip);
    let ipv4_header_start = 14;
    let ipv4_payload_start = ipv4_header_start + 20;
    let checksum = ipv4_header_checksum(&frame[ipv4_header_start..ipv4_payload_start]);
    frame[ipv4_header_start + 10..ipv4_header_start + 12].copy_from_slice(&checksum.to_be_bytes());

    frame.extend_from_slice(request_payload);
    frame[ipv4_payload_start] = 0;
    frame[ipv4_payload_start + 2] = 0;
    frame[ipv4_payload_start + 3] = 0;
    let checksum = icmp_checksum(&frame[ipv4_payload_start..]);
    frame[ipv4_payload_start + 2..ipv4_payload_start + 4].copy_from_slice(&checksum.to_be_bytes());
    Some(frame)
}

fn build_rewritten_icmp_echo_reply_frame(
    dst_mac: MacAddr,
    src_mac: MacAddr,
    src_ip: Ipv4Addr,
    dst_ip: Ipv4Addr,
    reply: &[u8],
    guest_identifier: u16,
) -> Option<Vec<u8>> {
    let offset = icmp_reply_payload_offset(reply)?;
    let icmp = &reply[offset..];
    if icmp[0] != 0 || icmp[1] != 0 {
        return None;
    }
    let ipv4_len = 20usize.checked_add(icmp.len())?;
    let total_len = u16::try_from(ipv4_len).ok()?;
    let mut frame = Vec::with_capacity(14 + ipv4_len);
    frame.extend_from_slice(&dst_mac);
    frame.extend_from_slice(&src_mac);
    frame.extend_from_slice(&ETHERTYPE_IPV4.to_be_bytes());
    frame.push(0x45);
    frame.push(0);
    frame.extend_from_slice(&total_len.to_be_bytes());
    frame.extend_from_slice(&0u16.to_be_bytes());
    frame.extend_from_slice(&0u16.to_be_bytes());
    frame.push(64);
    frame.push(IPV4_PROTOCOL_ICMP);
    frame.extend_from_slice(&0u16.to_be_bytes());
    frame.extend_from_slice(&src_ip);
    frame.extend_from_slice(&dst_ip);
    let ipv4_header_start = 14;
    let ipv4_payload_start = ipv4_header_start + 20;
    let checksum = ipv4_header_checksum(&frame[ipv4_header_start..ipv4_payload_start]);
    frame[ipv4_header_start + 10..ipv4_header_start + 12].copy_from_slice(&checksum.to_be_bytes());

    frame.extend_from_slice(icmp);
    frame[ipv4_payload_start + 2] = 0;
    frame[ipv4_payload_start + 3] = 0;
    frame[ipv4_payload_start + 4..ipv4_payload_start + 6]
        .copy_from_slice(&guest_identifier.to_be_bytes());
    let checksum = icmp_checksum(&frame[ipv4_payload_start..]);
    frame[ipv4_payload_start + 2..ipv4_payload_start + 4].copy_from_slice(&checksum.to_be_bytes());
    Some(frame)
}

fn build_udp_reply_frame(
    dst_mac: MacAddr,
    src_mac: MacAddr,
    src_ip: Ipv4Addr,
    dst_ip: Ipv4Addr,
    src_port: u16,
    dst_port: u16,
    payload: &[u8],
) -> Option<Vec<u8>> {
    build_udp_reply_frame_with_id(
        dst_mac, src_mac, src_ip, dst_ip, src_port, dst_port, payload, 0,
    )
}

fn build_udp_reply_frame_with_id(
    dst_mac: MacAddr,
    src_mac: MacAddr,
    src_ip: Ipv4Addr,
    dst_ip: Ipv4Addr,
    src_port: u16,
    dst_port: u16,
    payload: &[u8],
    identification: u16,
) -> Option<Vec<u8>> {
    let udp_len = 8usize.checked_add(payload.len())?;
    let udp_len_u16 = u16::try_from(udp_len).ok()?;
    let ipv4_len = 20usize.checked_add(udp_len)?;
    let total_len = u16::try_from(ipv4_len).ok()?;
    let mut frame = Vec::with_capacity(14 + ipv4_len);
    frame.extend_from_slice(&dst_mac);
    frame.extend_from_slice(&src_mac);
    frame.extend_from_slice(&ETHERTYPE_IPV4.to_be_bytes());
    frame.push(0x45);
    frame.push(0);
    frame.extend_from_slice(&total_len.to_be_bytes());
    frame.extend_from_slice(&identification.to_be_bytes());
    frame.extend_from_slice(&0u16.to_be_bytes());
    frame.push(64);
    frame.push(IPV4_PROTOCOL_UDP);
    frame.extend_from_slice(&0u16.to_be_bytes());
    frame.extend_from_slice(&src_ip);
    frame.extend_from_slice(&dst_ip);
    let ipv4_header_start = 14;
    let ipv4_payload_start = ipv4_header_start + 20;
    let checksum = ipv4_header_checksum(&frame[ipv4_header_start..ipv4_payload_start]);
    frame[ipv4_header_start + 10..ipv4_header_start + 12].copy_from_slice(&checksum.to_be_bytes());

    frame.extend_from_slice(&src_port.to_be_bytes());
    frame.extend_from_slice(&dst_port.to_be_bytes());
    frame.extend_from_slice(&udp_len_u16.to_be_bytes());
    frame.extend_from_slice(&0u16.to_be_bytes());
    frame.extend_from_slice(payload);
    let checksum = match udp_checksum(src_ip, dst_ip, &frame[ipv4_payload_start..]) {
        0 => 0xffff,
        checksum => checksum,
    };
    frame[ipv4_payload_start + 6..ipv4_payload_start + 8].copy_from_slice(&checksum.to_be_bytes());
    Some(frame)
}

fn build_dhcp_reply_frame(
    dst_mac: MacAddr,
    src_mac: MacAddr,
    dst_ip: Ipv4Addr,
    request: &DhcpRequest,
    message_type: u8,
    identification: u16,
) -> Option<Vec<u8>> {
    let udp_len = 8usize.checked_add(DHCP_REPLY_PAYLOAD_LEN)?;
    let udp_len_u16 = u16::try_from(udp_len).ok()?;
    let ipv4_len = 20usize.checked_add(udp_len)?;
    let total_len = u16::try_from(ipv4_len).ok()?;
    let mut frame = Vec::with_capacity(14 + ipv4_len);
    frame.extend_from_slice(&dst_mac);
    frame.extend_from_slice(&src_mac);
    frame.extend_from_slice(&ETHERTYPE_IPV4.to_be_bytes());
    frame.push(0x45);
    frame.push(0);
    frame.extend_from_slice(&total_len.to_be_bytes());
    frame.extend_from_slice(&identification.to_be_bytes());
    frame.extend_from_slice(&0u16.to_be_bytes());
    frame.push(64);
    frame.push(IPV4_PROTOCOL_UDP);
    frame.extend_from_slice(&0u16.to_be_bytes());
    frame.extend_from_slice(&DHCP_SERVER_IP);
    frame.extend_from_slice(&dst_ip);
    let ipv4_header_start = 14;
    let ipv4_payload_start = ipv4_header_start + 20;
    let checksum = ipv4_header_checksum(&frame[ipv4_header_start..ipv4_payload_start]);
    frame[ipv4_header_start + 10..ipv4_header_start + 12].copy_from_slice(&checksum.to_be_bytes());

    frame.extend_from_slice(&DHCP_SERVER_PORT.to_be_bytes());
    frame.extend_from_slice(&DHCP_CLIENT_PORT.to_be_bytes());
    frame.extend_from_slice(&udp_len_u16.to_be_bytes());
    frame.extend_from_slice(&0u16.to_be_bytes());

    let dhcp_start = frame.len();
    frame.resize(dhcp_start + DHCP_REPLY_FIXED_LEN, 0);
    frame[dhcp_start] = 2;
    frame[dhcp_start + 1] = request.htype;
    frame[dhcp_start + 2] = request.hlen;
    frame[dhcp_start + 4..dhcp_start + 8].copy_from_slice(&request.xid);
    frame[dhcp_start + 10..dhcp_start + 12].copy_from_slice(&request.flags.to_be_bytes());
    frame[dhcp_start + 16..dhcp_start + 20].copy_from_slice(&GUEST_IP);
    frame[dhcp_start + 20..dhcp_start + 24].copy_from_slice(&DHCP_SERVER_IP);
    frame[dhcp_start + 28..dhcp_start + 44].copy_from_slice(&request.chaddr);
    frame[dhcp_start + 236..dhcp_start + 240].copy_from_slice(&DHCP_MAGIC_COOKIE);
    push_dhcp_option(&mut frame, DHCP_OPT_MESSAGE_TYPE, &[message_type]);
    push_dhcp_option(&mut frame, DHCP_OPT_SERVER_ID, &DHCP_SERVER_IP);
    push_dhcp_option(
        &mut frame,
        DHCP_OPT_LEASE_TIME,
        &DHCP_LEASE_SECONDS.to_be_bytes(),
    );
    push_dhcp_option(&mut frame, DHCP_OPT_SUBNET_MASK, &SUBNET_MASK);
    push_dhcp_option(&mut frame, DHCP_OPT_ROUTER, &GATEWAY_IP);
    push_dhcp_option(&mut frame, DHCP_OPT_DNS, &DNS_IP);
    frame.push(DHCP_OPT_END);
    debug_assert_eq!(frame.len(), 14 + ipv4_len);

    let checksum = match udp_checksum(DHCP_SERVER_IP, dst_ip, &frame[ipv4_payload_start..]) {
        0 => 0xffff,
        checksum => checksum,
    };
    frame[ipv4_payload_start + 6..ipv4_payload_start + 8].copy_from_slice(&checksum.to_be_bytes());
    Some(frame)
}

fn build_tcp_reply_frame(
    dst_mac: MacAddr,
    src_mac: MacAddr,
    src_ip: Ipv4Addr,
    dst_ip: Ipv4Addr,
    src_port: u16,
    dst_port: u16,
    seq: u32,
    ack: u32,
    flags: u8,
    payload: &[u8],
) -> Option<Vec<u8>> {
    let tcp_len = 20usize.checked_add(payload.len())?;
    let ipv4_len = 20usize.checked_add(tcp_len)?;
    let total_len = u16::try_from(ipv4_len).ok()?;
    let mut frame = Vec::with_capacity(14 + ipv4_len);
    frame.extend_from_slice(&dst_mac);
    frame.extend_from_slice(&src_mac);
    frame.extend_from_slice(&ETHERTYPE_IPV4.to_be_bytes());
    frame.push(0x45);
    frame.push(0);
    frame.extend_from_slice(&total_len.to_be_bytes());
    frame.extend_from_slice(&0u16.to_be_bytes());
    frame.extend_from_slice(&0u16.to_be_bytes());
    frame.push(64);
    frame.push(IPV4_PROTOCOL_TCP);
    frame.extend_from_slice(&0u16.to_be_bytes());
    frame.extend_from_slice(&src_ip);
    frame.extend_from_slice(&dst_ip);
    let ipv4_header_start = 14;
    let ipv4_payload_start = ipv4_header_start + 20;
    let checksum = ipv4_header_checksum(&frame[ipv4_header_start..ipv4_payload_start]);
    frame[ipv4_header_start + 10..ipv4_header_start + 12].copy_from_slice(&checksum.to_be_bytes());

    frame.extend_from_slice(&src_port.to_be_bytes());
    frame.extend_from_slice(&dst_port.to_be_bytes());
    frame.extend_from_slice(&seq.to_be_bytes());
    frame.extend_from_slice(&ack.to_be_bytes());
    frame.push(5 << 4);
    frame.push(flags);
    frame.extend_from_slice(&65535u16.to_be_bytes());
    frame.extend_from_slice(&0u16.to_be_bytes());
    frame.extend_from_slice(&0u16.to_be_bytes());
    frame.extend_from_slice(payload);
    let checksum = tcp_checksum(src_ip, dst_ip, &frame[ipv4_payload_start..]);
    frame[ipv4_payload_start + 16..ipv4_payload_start + 18]
        .copy_from_slice(&checksum.to_be_bytes());
    Some(frame)
}

pub fn build_udp_datagram(
    src_ip: Ipv4Addr,
    dst_ip: Ipv4Addr,
    src_port: u16,
    dst_port: u16,
    payload: &[u8],
) -> Vec<u8> {
    let length = 8usize
        .checked_add(payload.len())
        .and_then(|len| u16::try_from(len).ok())
        .expect("UDP payload is too large");
    let mut segment = Vec::with_capacity(usize::from(length));
    segment.extend_from_slice(&src_port.to_be_bytes());
    segment.extend_from_slice(&dst_port.to_be_bytes());
    segment.extend_from_slice(&length.to_be_bytes());
    segment.extend_from_slice(&0u16.to_be_bytes());
    segment.extend_from_slice(payload);
    let checksum = match udp_checksum(src_ip, dst_ip, &segment) {
        0 => 0xffff,
        checksum => checksum,
    };
    segment[6..8].copy_from_slice(&checksum.to_be_bytes());
    segment
}

pub fn build_tcp_segment(
    src_ip: Ipv4Addr,
    dst_ip: Ipv4Addr,
    src_port: u16,
    dst_port: u16,
    seq: u32,
    ack: u32,
    flags: u8,
    payload: &[u8],
) -> Vec<u8> {
    let mut segment = Vec::with_capacity(20 + payload.len());
    segment.extend_from_slice(&src_port.to_be_bytes());
    segment.extend_from_slice(&dst_port.to_be_bytes());
    segment.extend_from_slice(&seq.to_be_bytes());
    segment.extend_from_slice(&ack.to_be_bytes());
    segment.push(5 << 4);
    segment.push(flags);
    segment.extend_from_slice(&65535u16.to_be_bytes());
    segment.extend_from_slice(&0u16.to_be_bytes());
    segment.extend_from_slice(&0u16.to_be_bytes());
    segment.extend_from_slice(payload);
    let checksum = tcp_checksum(src_ip, dst_ip, &segment);
    segment[16..18].copy_from_slice(&checksum.to_be_bytes());
    segment
}

pub fn ipv4_header_checksum(header: &[u8]) -> u16 {
    internet_checksum(header)
}

pub fn udp_checksum(src_ip: Ipv4Addr, dst_ip: Ipv4Addr, udp_segment: &[u8]) -> u16 {
    let mut sum = 0u32;
    sum = checksum_add_bytes(sum, &src_ip);
    sum = checksum_add_bytes(sum, &dst_ip);
    sum = checksum_add_bytes(sum, &[0, IPV4_PROTOCOL_UDP]);
    sum = checksum_add_bytes(sum, &(udp_segment.len() as u16).to_be_bytes());
    sum = checksum_add_bytes(sum, udp_segment);
    checksum_finalize(sum)
}

pub fn tcp_checksum(src_ip: Ipv4Addr, dst_ip: Ipv4Addr, tcp_segment: &[u8]) -> u16 {
    let mut sum = 0u32;
    sum = checksum_add_bytes(sum, &src_ip);
    sum = checksum_add_bytes(sum, &dst_ip);
    sum = checksum_add_bytes(sum, &[0, IPV4_PROTOCOL_TCP]);
    sum = checksum_add_bytes(sum, &(tcp_segment.len() as u16).to_be_bytes());
    sum = checksum_add_bytes(sum, tcp_segment);
    checksum_finalize(sum)
}

pub fn icmp_checksum(message: &[u8]) -> u16 {
    internet_checksum(message)
}

pub fn internet_checksum(bytes: &[u8]) -> u16 {
    checksum_finalize(checksum_add_bytes(0, bytes))
}

fn checksum_add_bytes(mut sum: u32, bytes: &[u8]) -> u32 {
    let mut chunks = bytes.chunks_exact(2);
    for chunk in &mut chunks {
        sum = sum.wrapping_add(u32::from(u16::from_be_bytes([chunk[0], chunk[1]])));
        sum = (sum & 0xffff) + (sum >> 16);
    }
    let rem = chunks.remainder();
    if let Some(byte) = rem.first() {
        sum = sum.wrapping_add(u32::from(*byte) << 8);
        sum = (sum & 0xffff) + (sum >> 16);
    }
    sum
}

fn checksum_finalize(mut sum: u32) -> u16 {
    while sum >> 16 != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !(sum as u16)
}

fn push_dhcp_option(msg: &mut Vec<u8>, code: u8, value: &[u8]) {
    let len = u8::try_from(value.len()).expect("DHCP option too long");
    msg.push(code);
    msg.push(len);
    msg.extend_from_slice(value);
}

fn dhcp_reply_destination(request: &DhcpRequest) -> Ipv4Addr {
    if request.ciaddr != [0, 0, 0, 0] {
        request.ciaddr
    } else if request.flags & DHCP_FLAG_BROADCAST != 0 {
        IPV4_BROADCAST
    } else {
        GUEST_IP
    }
}

fn dhcp_option(options: &[u8], code: u8) -> Option<&[u8]> {
    let mut offset = 0usize;
    while offset < options.len() {
        let option = options[offset];
        offset += 1;
        match option {
            0 => {}
            DHCP_OPT_END => break,
            _ => {
                let len = usize::from(*options.get(offset)?);
                offset += 1;
                let value = options.get(offset..offset.checked_add(len)?)?;
                if option == code {
                    return Some(value);
                }
                offset += len;
            }
        }
    }
    None
}

fn is_non_local_ipv4_destination(dst: Ipv4Addr) -> bool {
    if dst == [0, 0, 0, 0]
        || dst == IPV4_BROADCAST
        || dst == GUEST_SUBNET_BROADCAST
        || (224..=239).contains(&dst[0])
    {
        return false;
    }
    dst[0..3] != [10, 0, 2]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IcmpEchoRoute {
    Gateway,
    External,
    Other,
}

fn classify_icmp_echo(dst: Ipv4Addr, payload: &[u8]) -> IcmpEchoRoute {
    if payload.len() < 8 || payload[0] != 8 || payload[1] != 0 {
        return IcmpEchoRoute::Other;
    }
    if dst == GATEWAY_IP {
        IcmpEchoRoute::Gateway
    } else if is_non_local_ipv4_destination(dst) {
        IcmpEchoRoute::External
    } else {
        IcmpEchoRoute::Other
    }
}

fn icmp_reply_payload_offset(buf: &[u8]) -> Option<usize> {
    let first = *buf.first()?;
    if first >> 4 != 4 {
        return (buf.len() >= 8).then_some(0);
    }

    let header_len = usize::from(first & 0x0f) * 4;
    if header_len < 20 || buf.len() < header_len + 8 {
        return None;
    }
    Some(header_len)
}

#[cfg(test)]
fn rewrite_icmp_echo_reply_identifier(reply: &[u8], guest_identifier: u16) -> Option<Vec<u8>> {
    let offset = icmp_reply_payload_offset(reply)?;
    let icmp = &reply[offset..];
    if icmp[0] != 0 || icmp[1] != 0 {
        return None;
    }
    let mut rewritten = icmp.to_vec();
    rewritten[2] = 0;
    rewritten[3] = 0;
    rewritten[4..6].copy_from_slice(&guest_identifier.to_be_bytes());
    let checksum = icmp_checksum(&rewritten);
    rewritten[2..4].copy_from_slice(&checksum.to_be_bytes());
    Some(rewritten)
}

fn icmp_reply_rejection_reason(reply: &[u8]) -> &'static str {
    let Some(offset) = icmp_reply_payload_offset(reply) else {
        return "malformed";
    };
    let icmp = &reply[offset..];
    if icmp[0] != 0 {
        return "not_echo_reply";
    }
    if icmp[1] != 0 {
        return "nonzero_code";
    }
    "rejected"
}

fn trace_icmp_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("BRIDGEVM_TRACE_ICMP").is_some())
}

fn trace_icmp_recv(len: usize, first_byte: u8, outcome: &str) {
    eprintln!("bridgevm icmp recv bytes={len} first=0x{first_byte:02x} {outcome}");
}

fn first_resolv_conf_nameserver() -> Option<StdIpv4Addr> {
    first_nameserver_from_path(Path::new("/etc/resolv.conf"))
}

fn first_nameserver_from_path(path: &Path) -> Option<StdIpv4Addr> {
    let file = std::fs::File::open(path).ok()?;
    if file.metadata().ok()?.len() > MAX_RESOLV_CONF_BYTES {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(MAX_RESOLV_CONF_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 > MAX_RESOLV_CONF_BYTES {
        return None;
    }
    let contents = String::from_utf8(bytes).ok()?;
    for line in contents.lines() {
        let line = line.trim();
        if line.starts_with('#') || !line.starts_with("nameserver") {
            continue;
        }
        let addr = line.split_whitespace().nth(1)?;
        if let Ok(IpAddr::V4(ip)) = addr.parse::<IpAddr>() {
            return Some(ip);
        }
    }
    None
}

fn queue_udp_reply(
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

fn queue_tcp_reply(
    reply_queue: &mut VecDeque<Vec<u8>>,
    guest_mac: MacAddr,
    key: &TcpFlowKey,
    seq: u32,
    ack: u32,
    flags: u8,
    payload: &[u8],
) {
    let Some(frame) = build_tcp_reply_frame(
        guest_mac,
        GATEWAY_MAC,
        key.dst_ip,
        key.guest_ip,
        key.dst_port,
        key.guest_port,
        seq,
        ack,
        flags,
        payload,
    ) else {
        return;
    };
    reply_queue.push_back(frame);
}

fn evict_lru<K: Copy + Eq + std::hash::Hash, V: HasLastActivity>(
    map: &mut HashMap<K, V>,
    cap: usize,
) {
    while map.len() > cap {
        let Some((&key, _)) = map.iter().min_by_key(|(_, flow)| flow.last_activity()) else {
            return;
        };
        map.remove(&key);
    }
}

fn get_or_insert_lru<K, V, E, F>(
    map: &mut HashMap<K, V>,
    key: K,
    cap: usize,
    create: F,
) -> Result<Option<&mut V>, E>
where
    K: Copy + Eq + std::hash::Hash,
    V: HasLastActivity,
    F: FnOnce() -> Result<V, E>,
{
    if let Entry::Vacant(entry) = map.entry(key) {
        entry.insert(create()?);
        evict_lru(map, cap);
    }
    Ok(map.get_mut(&key))
}

trait HasLastActivity {
    fn last_activity(&self) -> u64;
}

impl HasLastActivity for UdpFlow {
    fn last_activity(&self) -> u64 {
        self.last_activity
    }
}

impl HasLastActivity for TcpFlow {
    fn last_activity(&self) -> u64 {
        self.last_activity
    }
}

impl HasLastActivity for IcmpFlow {
    fn last_activity(&self) -> u64 {
        self.last_activity
    }
}

fn would_block(err: &io::Error) -> bool {
    matches!(
        err.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
    )
}

fn tcp_connect_error(stream: &TcpStream) -> io::Result<Option<i32>> {
    match stream.take_error()? {
        Some(err) => Ok(Some(err.raw_os_error().unwrap_or(1))),
        None => {
            let mut byte = [0u8; 0];
            match stream.peek(&mut byte) {
                Ok(_) => Ok(Some(0)),
                Err(err) if would_block(&err) => Ok(None),
                Err(err) if err.kind() == io::ErrorKind::NotConnected => Ok(None),
                Err(err) => Ok(Some(err.raw_os_error().unwrap_or(1))),
            }
        }
    }
}

#[cfg(target_os = "macos")]
#[derive(Debug)]
struct RawIcmpSocket {
    fd: RawFd,
}

#[cfg(target_os = "macos")]
impl RawIcmpSocket {
    fn new_nonblocking() -> io::Result<Self> {
        // SAFETY: socket is called with valid constants; errors are returned via errno.
        let fd = unsafe { socket(AF_INET, SOCK_DGRAM, IPPROTO_ICMP) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        if let Err(err) = set_raw_icmp_socket_nonblocking(fd) {
            let _ = raw_close(fd);
            return Err(err);
        }
        Ok(Self { fd })
    }

    fn send_to(&self, message: &[u8], dst: StdIpv4Addr) -> io::Result<usize> {
        let addr = RawSockAddrIn::new(dst, 0);
        // SAFETY: message and addr point to valid memory for the duration of the call.
        let rc = unsafe {
            sendto(
                self.fd,
                message.as_ptr().cast(),
                message.len(),
                0,
                (&addr as *const RawSockAddrIn).cast::<RawSockAddr>(),
                std::mem::size_of::<RawSockAddrIn>() as RawSockLen,
            )
        };
        if rc >= 0 {
            Ok(rc as usize)
        } else {
            Err(io::Error::last_os_error())
        }
    }

    fn recv_from(&self, buf: &mut [u8]) -> io::Result<(usize, StdIpv4Addr)> {
        // SAFETY: zeroed sockaddr_in is a valid writable buffer for recvfrom.
        let mut addr = unsafe { std::mem::zeroed::<RawSockAddrIn>() };
        let mut len = std::mem::size_of::<RawSockAddrIn>() as RawSockLen;
        // SAFETY: buf and addr point to valid writable memory for the duration of the call.
        let rc = unsafe {
            recvfrom(
                self.fd,
                buf.as_mut_ptr().cast(),
                buf.len(),
                0,
                (&mut addr as *mut RawSockAddrIn).cast::<RawSockAddr>(),
                &mut len,
            )
        };
        if rc >= 0 {
            Ok((rc as usize, StdIpv4Addr::from(addr.sin_addr.to_ne_bytes())))
        } else {
            Err(io::Error::last_os_error())
        }
    }
}

#[cfg(target_os = "macos")]
fn raw_icmp_nonblocking_flags(flags: i32) -> i32 {
    flags | O_NONBLOCK
}

#[cfg(target_os = "macos")]
fn set_raw_icmp_socket_nonblocking(fd: RawFd) -> io::Result<()> {
    // SAFETY: this ICMP-only fcntl binding is declared with C's variadic ABI,
    // which is required for F_SETFL on arm64 macOS.
    let flags = unsafe { fcntl_icmp(fd, F_GETFL) };
    if flags < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: fcntl operates on the socket fd created above.
    if unsafe { fcntl_icmp(fd, F_SETFL, raw_icmp_nonblocking_flags(flags)) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(target_os = "macos")]
impl Drop for RawIcmpSocket {
    fn drop(&mut self) {
        let _ = raw_close(self.fd);
    }
}

#[cfg(not(target_os = "macos"))]
#[derive(Debug)]
struct RawIcmpSocket;

#[cfg(not(target_os = "macos"))]
impl RawIcmpSocket {
    fn new_nonblocking() -> io::Result<Self> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "unprivileged ICMP datagram sockets are only enabled on macOS",
        ))
    }

    fn send_to(&self, _message: &[u8], _dst: StdIpv4Addr) -> io::Result<usize> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "icmp unsupported",
        ))
    }

    fn recv_from(&self, _buf: &mut [u8]) -> io::Result<(usize, StdIpv4Addr)> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "icmp unsupported",
        ))
    }
}

#[cfg(unix)]
fn nonblocking_tcp_connect(dst: StdIpv4Addr, port: u16) -> io::Result<TcpStream> {
    let fd = raw_nonblocking_tcp_socket()?;
    let connect_result = raw_connect_ipv4(fd, dst, port);
    if let Err(err) = connect_result {
        if !raw_connect_in_progress(&err) {
            let _ = raw_close(fd);
            return Err(err);
        }
    }
    // SAFETY: fd is a valid TCP socket created above and ownership is moved into TcpStream.
    let stream = unsafe { TcpStream::from_raw_fd(fd) };
    stream.set_nonblocking(true)?;
    Ok(stream)
}

#[cfg(not(unix))]
fn nonblocking_tcp_connect(dst: StdIpv4Addr, port: u16) -> io::Result<TcpStream> {
    let stream = TcpStream::connect(SocketAddrV4::new(dst, port))?;
    stream.set_nonblocking(true)?;
    Ok(stream)
}

#[cfg(unix)]
fn raw_nonblocking_tcp_socket() -> io::Result<RawFd> {
    // SAFETY: socket is called with valid constants; errors are returned via errno.
    let fd = unsafe { socket(AF_INET, SOCK_STREAM, 0) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: fcntl operates on the socket fd created above.
    let flags = unsafe { fcntl(fd, F_GETFL, 0) };
    if flags < 0 || unsafe { fcntl(fd, F_SETFL, flags | O_NONBLOCK) } < 0 {
        let err = io::Error::last_os_error();
        let _ = raw_close(fd);
        return Err(err);
    }
    Ok(fd)
}

#[cfg(unix)]
fn raw_connect_ipv4(fd: RawFd, dst: StdIpv4Addr, port: u16) -> io::Result<()> {
    let addr = RawSockAddrIn::new(dst, port);
    // SAFETY: addr points to a properly initialized IPv4 sockaddr for this platform.
    let rc = unsafe {
        connect(
            fd,
            (&addr as *const RawSockAddrIn).cast::<RawSockAddr>(),
            std::mem::size_of::<RawSockAddrIn>() as RawSockLen,
        )
    };
    if rc == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(unix)]
fn raw_connect_in_progress(err: &io::Error) -> bool {
    matches!(
        err.raw_os_error(),
        Some(ERRNO_EINPROGRESS) | Some(ERRNO_EALREADY) | Some(ERRNO_EWOULDBLOCK)
    )
}

#[cfg(unix)]
fn raw_close(fd: RawFd) -> io::Result<()> {
    // SAFETY: close is called with a raw fd; errors are surfaced through errno.
    let rc = unsafe { close(fd) };
    if rc == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(target_os = "macos")]
type RawSockLen = u32;
#[cfg(target_os = "macos")]
#[repr(C)]
struct RawSockAddr {
    sa_len: u8,
    sa_family: u8,
    sa_data: [u8; 14],
}
#[cfg(target_os = "macos")]
#[repr(C)]
struct RawSockAddrIn {
    sin_len: u8,
    sin_family: u8,
    sin_port: u16,
    sin_addr: u32,
    sin_zero: [u8; 8],
}
#[cfg(target_os = "macos")]
impl RawSockAddrIn {
    fn new(dst: StdIpv4Addr, port: u16) -> Self {
        Self {
            sin_len: std::mem::size_of::<Self>() as u8,
            sin_family: AF_INET as u8,
            sin_port: port.to_be(),
            sin_addr: u32::from_ne_bytes(dst.octets()),
            sin_zero: [0; 8],
        }
    }
}

#[cfg(target_os = "linux")]
type RawSockLen = u32;
#[cfg(target_os = "linux")]
#[repr(C)]
struct RawSockAddr {
    sa_family: u16,
    sa_data: [u8; 14],
}
#[cfg(target_os = "linux")]
#[repr(C)]
struct RawSockAddrIn {
    sin_family: u16,
    sin_port: u16,
    sin_addr: u32,
    sin_zero: [u8; 8],
}
#[cfg(target_os = "linux")]
impl RawSockAddrIn {
    fn new(dst: StdIpv4Addr, port: u16) -> Self {
        Self {
            sin_family: AF_INET as u16,
            sin_port: port.to_be(),
            sin_addr: u32::from_ne_bytes(dst.octets()),
            sin_zero: [0; 8],
        }
    }
}

#[cfg(target_os = "macos")]
const AF_INET: i32 = 2;
#[cfg(target_os = "linux")]
const AF_INET: i32 = 2;
#[cfg(unix)]
const SOCK_STREAM: i32 = 1;
#[cfg(target_os = "macos")]
const SOCK_DGRAM: i32 = 2;
#[cfg(target_os = "macos")]
const IPPROTO_ICMP: i32 = 1;
#[cfg(unix)]
const F_GETFL: i32 = 3;
#[cfg(unix)]
const F_SETFL: i32 = 4;
#[cfg(target_os = "macos")]
const O_NONBLOCK: i32 = 0x0004;
#[cfg(target_os = "linux")]
const O_NONBLOCK: i32 = 0o4000;
#[cfg(target_os = "macos")]
const ERRNO_EINPROGRESS: i32 = 36;
#[cfg(target_os = "linux")]
const ERRNO_EINPROGRESS: i32 = 115;
#[cfg(target_os = "macos")]
const ERRNO_EALREADY: i32 = 37;
#[cfg(target_os = "linux")]
const ERRNO_EALREADY: i32 = 114;
#[cfg(target_os = "macos")]
const ERRNO_EWOULDBLOCK: i32 = 35;
#[cfg(target_os = "linux")]
const ERRNO_EWOULDBLOCK: i32 = 11;

#[cfg(unix)]
unsafe extern "C" {
    fn socket(domain: i32, ty: i32, protocol: i32) -> RawFd;
    fn connect(fd: RawFd, addr: *const RawSockAddr, len: RawSockLen) -> i32;
    fn fcntl(fd: RawFd, cmd: i32, arg: i32) -> i32;
    #[cfg(target_os = "macos")]
    #[allow(clashing_extern_declarations)]
    #[link_name = "fcntl"]
    fn fcntl_icmp(fd: RawFd, cmd: i32, ...) -> i32;
    fn close(fd: RawFd) -> i32;
    #[cfg(target_os = "macos")]
    fn sendto(
        fd: RawFd,
        buf: *const std::ffi::c_void,
        len: usize,
        flags: i32,
        addr: *const RawSockAddr,
        addr_len: RawSockLen,
    ) -> isize;
    #[cfg(target_os = "macos")]
    fn recvfrom(
        fd: RawFd,
        buf: *mut std::ffi::c_void,
        len: usize,
        flags: i32,
        addr: *mut RawSockAddr,
        addr_len: *mut RawSockLen,
    ) -> isize;
}

fn read_u16_be(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_be_bytes(read_array(bytes, offset)?))
}

fn read_array<const N: usize>(bytes: &[u8], offset: usize) -> Option<[u8; N]> {
    let end = offset.checked_add(N)?;
    bytes.get(offset..end)?.try_into().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const GUEST_MAC: MacAddr = [0x52, 0x54, 0x00, 0x12, 0x34, 0x56];
    const OTHER_GUEST_MAC: MacAddr = [0x52, 0x54, 0x00, 0xaa, 0xbb, 0xcc];
    const BROADCAST_MAC: MacAddr = [0xff; 6];

    fn temp_resolv_path(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "bridgevm-resolv-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn resolver_reader_parses_first_ipv4_nameserver() {
        let path = temp_resolv_path("valid");
        std::fs::write(
            &path,
            "# generated\nnameserver 2001:db8::1\nnameserver 9.8.7.6\n",
        )
        .unwrap();

        let resolver = first_nameserver_from_path(&path);
        let _ = std::fs::remove_file(&path);

        assert_eq!(resolver, Some(StdIpv4Addr::new(9, 8, 7, 6)));
    }

    #[test]
    fn resolver_reader_rejects_sparse_oversized_input_before_allocation() {
        let path = temp_resolv_path("oversized");
        let file = std::fs::File::create(&path).unwrap();
        file.set_len(512 * 1024 * 1024).unwrap();

        let resolver = first_nameserver_from_path(&path);
        let _ = std::fs::remove_file(&path);

        assert_eq!(resolver, None);
    }

    #[test]
    fn resolver_reader_rejects_invalid_utf8() {
        let path = temp_resolv_path("invalid-utf8");
        std::fs::write(&path, [0xff, 0xfe]).unwrap();

        let resolver = first_nameserver_from_path(&path);
        let _ = std::fs::remove_file(&path);

        assert_eq!(resolver, None);
    }

    fn arp_request(src_mac: MacAddr, sender_ip: Ipv4Addr, target_ip: Ipv4Addr) -> Vec<u8> {
        let mut payload = Vec::with_capacity(28);
        payload.extend_from_slice(&ARP_HARDWARE_ETHERNET.to_be_bytes());
        payload.extend_from_slice(&ETHERTYPE_IPV4.to_be_bytes());
        payload.push(6);
        payload.push(4);
        payload.extend_from_slice(&ARP_OPCODE_REQUEST.to_be_bytes());
        payload.extend_from_slice(&src_mac);
        payload.extend_from_slice(&sender_ip);
        payload.extend_from_slice(&[0; 6]);
        payload.extend_from_slice(&target_ip);
        EthernetFrame::build(BROADCAST_MAC, src_mac, ETHERTYPE_ARP, &payload)
    }

    fn icmp_echo_request() -> Vec<u8> {
        icmp_echo_frame(GATEWAY_IP, 0x1234)
    }

    fn icmp_echo_frame(dst_ip: Ipv4Addr, identifier: u16) -> Vec<u8> {
        let mut icmp = vec![8, 0, 0, 0, 0x12, 0x34, 0x00, 0x01];
        icmp[4..6].copy_from_slice(&identifier.to_be_bytes());
        icmp.extend_from_slice(b"hello");
        let checksum = icmp_checksum(&icmp);
        icmp[2..4].copy_from_slice(&checksum.to_be_bytes());
        let ipv4 = build_ipv4_packet(GUEST_IP, dst_ip, IPV4_PROTOCOL_ICMP, &icmp);
        EthernetFrame::build(GATEWAY_MAC, GUEST_MAC, ETHERTYPE_IPV4, &ipv4)
    }

    fn dhcp_payload(message_type: u8, xid: [u8; 4], chaddr: MacAddr) -> Vec<u8> {
        let mut payload = vec![0u8; 240];
        payload[0] = 1;
        payload[1] = 1;
        payload[2] = 6;
        payload[4..8].copy_from_slice(&xid);
        payload[28..34].copy_from_slice(&chaddr);
        payload[236..240].copy_from_slice(&DHCP_MAGIC_COOKIE);
        push_dhcp_option(&mut payload, DHCP_OPT_MESSAGE_TYPE, &[message_type]);
        if message_type == DHCP_REQUEST {
            push_dhcp_option(&mut payload, DHCP_OPT_REQUESTED_IP, &GUEST_IP);
            push_dhcp_option(&mut payload, DHCP_OPT_SERVER_ID, &DHCP_SERVER_IP);
        }
        payload.push(DHCP_OPT_END);
        payload
    }

    fn dhcp_frame(message_type: u8, xid: [u8; 4], chaddr: MacAddr) -> Vec<u8> {
        dhcp_frame_with_ipv4_id(message_type, xid, chaddr, 0)
    }

    fn dhcp_frame_with_ipv4_id(
        message_type: u8,
        xid: [u8; 4],
        chaddr: MacAddr,
        identification: u16,
    ) -> Vec<u8> {
        let payload = dhcp_payload(message_type, xid, chaddr);
        let udp = build_udp_datagram(
            [0, 0, 0, 0],
            IPV4_BROADCAST,
            DHCP_CLIENT_PORT,
            DHCP_SERVER_PORT,
            &payload,
        );
        let ipv4 = build_ipv4_packet_with_id(
            [0, 0, 0, 0],
            IPV4_BROADCAST,
            IPV4_PROTOCOL_UDP,
            &udp,
            identification,
        );
        EthernetFrame::build(BROADCAST_MAC, chaddr, ETHERTYPE_IPV4, &ipv4)
    }

    fn parse_ipv4_udp_payload(
        frame: &[u8],
    ) -> (EthernetFrame<'_>, Ipv4Packet<'_>, UdpDatagram<'_>) {
        let eth = EthernetFrame::parse(frame).unwrap();
        let ip = Ipv4Packet::parse(eth.payload).unwrap();
        let udp = UdpDatagram::parse(ip.payload).unwrap();
        (eth, ip, udp)
    }

    fn udp_guest_frame(dst_ip: Ipv4Addr, dst_port: u16, src_port: u16, payload: &[u8]) -> Vec<u8> {
        let udp = build_udp_datagram(GUEST_IP, dst_ip, src_port, dst_port, payload);
        let ipv4 = build_ipv4_packet(GUEST_IP, dst_ip, IPV4_PROTOCOL_UDP, &udp);
        EthernetFrame::build(GATEWAY_MAC, GUEST_MAC, ETHERTYPE_IPV4, &ipv4)
    }

    fn tcp_guest_frame(
        dst_ip: Ipv4Addr,
        dst_port: u16,
        src_port: u16,
        seq: u32,
        ack: u32,
        flags: u8,
        payload: &[u8],
    ) -> Vec<u8> {
        let tcp = build_tcp_segment(
            GUEST_IP, dst_ip, src_port, dst_port, seq, ack, flags, payload,
        );
        let ipv4 = build_ipv4_packet(GUEST_IP, dst_ip, IPV4_PROTOCOL_TCP, &tcp);
        EthernetFrame::build(GATEWAY_MAC, GUEST_MAC, ETHERTYPE_IPV4, &ipv4)
    }

    fn parse_ipv4_tcp(frame: &[u8]) -> (EthernetFrame<'_>, Ipv4Packet<'_>, TcpSegment<'_>) {
        let eth = EthernetFrame::parse(frame).unwrap();
        let ip = Ipv4Packet::parse(eth.payload).unwrap();
        let tcp = TcpSegment::parse(ip.payload).unwrap();
        (eth, ip, tcp)
    }

    fn loopback_udp_socket() -> Option<UdpSocket> {
        match UdpSocket::bind(SocketAddrV4::new(StdIpv4Addr::LOCALHOST, 0)) {
            Ok(socket) => Some(socket),
            Err(e) if e.kind() == io::ErrorKind::PermissionDenied => None,
            Err(e) => panic!("loopback udp bind failed: {e}"),
        }
    }

    fn loopback_tcp_listener() -> Option<std::net::TcpListener> {
        match std::net::TcpListener::bind(SocketAddrV4::new(StdIpv4Addr::LOCALHOST, 0)) {
            Ok(listener) => Some(listener),
            Err(e) if e.kind() == io::ErrorKind::PermissionDenied => None,
            Err(e) => panic!("loopback tcp bind failed: {e}"),
        }
    }

    #[test]
    fn checksum_helpers_match_known_good_vectors() {
        let mut ipv4_header = Vec::from([
            0x45, 0x00, 0x00, 0x54, 0x00, 0x00, 0x40, 0x00, 0x40, 0x01, 0x00, 0x00, 0xc0, 0xa8,
            0x00, 0x01, 0xc0, 0xa8, 0x00, 0xc7,
        ]);
        assert_eq!(ipv4_header_checksum(&ipv4_header), 0xb890);
        ipv4_header[10..12].copy_from_slice(&0xb890u16.to_be_bytes());
        assert_eq!(ipv4_header_checksum(&ipv4_header), 0);

        let src = [192, 0, 2, 1];
        let dst = [198, 51, 100, 2];
        let mut udp = Vec::from([
            0x30, 0x39, 0x00, 0x35, 0x00, 0x0b, 0x00, 0x00, b'a', b'b', b'c',
        ]);
        assert_eq!(udp_checksum(src, dst, &udp), 0x1ed0);
        udp[6..8].copy_from_slice(&0x1ed0u16.to_be_bytes());
        assert_eq!(udp_checksum(src, dst, &udp), 0);

        let mut icmp = Vec::from([
            8, 0, 0, 0, 0x12, 0x34, 0x00, 0x01, b'h', b'e', b'l', b'l', b'o',
        ]);
        assert_eq!(icmp_checksum(&icmp), 0xa1f8);
        icmp[2..4].copy_from_slice(&0xa1f8u16.to_be_bytes());
        assert_eq!(icmp_checksum(&icmp), 0);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn raw_icmp_nonblocking_uses_variadic_fcntl_setfl_argument() {
        let _setup_path: fn(RawFd) -> io::Result<()> = set_raw_icmp_socket_nonblocking;
        assert_eq!(raw_icmp_nonblocking_flags(0), O_NONBLOCK);
        assert_eq!(raw_icmp_nonblocking_flags(0x20), 0x20 | O_NONBLOCK);
    }

    #[test]
    fn icmp_echo_classification_routes_gateway_and_external_only() {
        let external_frame = icmp_echo_frame([1, 1, 1, 1], 0x2345);
        let external = EthernetFrame::parse(&external_frame).unwrap();
        let external_ip = Ipv4Packet::parse(external.payload).unwrap();
        assert_eq!(
            classify_icmp_echo(external_ip.dst, external_ip.payload),
            IcmpEchoRoute::External
        );

        let gateway_frame = icmp_echo_request();
        let gateway = EthernetFrame::parse(&gateway_frame).unwrap();
        let gateway_ip = Ipv4Packet::parse(gateway.payload).unwrap();
        assert_eq!(
            classify_icmp_echo(gateway_ip.dst, gateway_ip.payload),
            IcmpEchoRoute::Gateway
        );

        let mut non_echo = gateway_ip.payload.to_vec();
        non_echo[0] = 3;
        assert_eq!(
            classify_icmp_echo([1, 1, 1, 1], &non_echo),
            IcmpEchoRoute::Other
        );
        assert_eq!(
            classify_icmp_echo([10, 0, 2, 99], gateway_ip.payload),
            IcmpEchoRoute::Other
        );
    }

    #[test]
    fn icmp_echo_reply_identifier_rewrite_recomputes_checksum() {
        let mut reply = vec![0, 0, 0, 0, 0xab, 0xcd, 0x00, 0x02];
        reply.extend_from_slice(b"payload");
        let checksum = icmp_checksum(&reply);
        reply[2..4].copy_from_slice(&checksum.to_be_bytes());

        let rewritten = rewrite_icmp_echo_reply_identifier(&reply, 0x1234).unwrap();
        assert_eq!(rewritten[0], 0);
        assert_eq!(rewritten[1], 0);
        assert_eq!(read_u16_be(&rewritten, 4), Some(0x1234));
        assert_eq!(
            &rewritten[6..],
            &[0x00, 0x02, b'p', b'a', b'y', b'l', b'o', b'a', b'd']
        );
        assert_eq!(icmp_checksum(&rewritten), 0);
        assert_ne!(read_u16_be(&rewritten, 2), read_u16_be(&reply, 2));
    }

    #[test]
    fn icmp_reply_payload_offset_accepts_ipv4_prefixed_and_raw_replies() {
        let icmp = [0, 0, 0, 0, 0xab, 0xcd, 0x00, 0x02];
        let ipv4 = build_ipv4_packet([1, 1, 1, 1], GUEST_IP, IPV4_PROTOCOL_ICMP, &icmp);
        assert_eq!(icmp_reply_payload_offset(&ipv4), Some(20));

        let mut ipv4_with_options = Vec::from([
            0x46, 0x00, 0x00, 0x20, 0x00, 0x00, 0x40, 0x00, 0x40, 0x01, 0x00, 0x00, 0x01, 0x01,
            0x01, 0x01, 0x0a, 0x00, 0x02, 0x0f, 0x01, 0x02, 0x03, 0x04,
        ]);
        ipv4_with_options.extend_from_slice(&icmp);
        assert_eq!(icmp_reply_payload_offset(&ipv4_with_options), Some(24));

        assert_eq!(icmp_reply_payload_offset(&icmp), Some(0));
        assert_eq!(icmp_reply_payload_offset(&[0, 0, 0]), None);
        assert_eq!(icmp_reply_payload_offset(&ipv4[..27]), None);
    }

    #[test]
    fn icmp_echo_reply_identifier_rewrite_skips_ipv4_header_prefix() {
        let mut reply = vec![0, 0, 0, 0, 0xab, 0xcd, 0x00, 0x02];
        reply.extend_from_slice(b"payload");
        let checksum = icmp_checksum(&reply);
        reply[2..4].copy_from_slice(&checksum.to_be_bytes());
        let ipv4 = build_ipv4_packet([1, 1, 1, 1], GUEST_IP, IPV4_PROTOCOL_ICMP, &reply);

        let rewritten = rewrite_icmp_echo_reply_identifier(&ipv4, 0x1234).unwrap();
        assert_eq!(rewritten[0], 0);
        assert_eq!(rewritten[1], 0);
        assert_eq!(read_u16_be(&rewritten, 4), Some(0x1234));
        assert_eq!(
            &rewritten[6..],
            &[0x00, 0x02, b'p', b'a', b'y', b'l', b'o', b'a', b'd']
        );
        assert_eq!(icmp_checksum(&rewritten), 0);
    }

    #[test]
    fn rewritten_icmp_echo_reply_frame_skips_ipv4_header_prefix() {
        let mut reply = vec![0, 0, 0, 0, 0xab, 0xcd, 0x00, 0x02];
        reply.extend_from_slice(b"payload");
        let checksum = icmp_checksum(&reply);
        reply[2..4].copy_from_slice(&checksum.to_be_bytes());
        let ipv4 = build_ipv4_packet([1, 1, 1, 1], GUEST_IP, IPV4_PROTOCOL_ICMP, &reply);

        let frame = build_rewritten_icmp_echo_reply_frame(
            GUEST_MAC,
            GATEWAY_MAC,
            [1, 1, 1, 1],
            GUEST_IP,
            &ipv4,
            0x1234,
        )
        .unwrap();

        let eth = EthernetFrame::parse(&frame).unwrap();
        assert_eq!(eth.dst, GUEST_MAC);
        assert_eq!(eth.src, GATEWAY_MAC);
        assert_eq!(eth.ethertype, ETHERTYPE_IPV4);
        let ip = Ipv4Packet::parse(eth.payload).unwrap();
        assert_eq!(ip.src, [1, 1, 1, 1]);
        assert_eq!(ip.dst, GUEST_IP);
        assert_eq!(ip.protocol, IPV4_PROTOCOL_ICMP);
        assert_eq!(ipv4_header_checksum(&ip.bytes[..ip.header_len]), 0);
        assert_eq!(read_u16_be(ip.payload, 4), Some(0x1234));
        assert_eq!(
            &ip.payload[6..],
            &[0x00, 0x02, b'p', b'a', b'y', b'l', b'o', b'a', b'd']
        );
        assert_eq!(icmp_checksum(ip.payload), 0);
    }

    #[derive(Debug)]
    struct TestFlow {
        last_activity: u64,
    }

    impl HasLastActivity for TestFlow {
        fn last_activity(&self) -> u64 {
            self.last_activity
        }
    }

    #[test]
    fn icmp_flow_table_get_or_create_expires_and_caps_lru() {
        let mut flows = HashMap::<IcmpFlowKey, TestFlow>::new();
        let key1 = IcmpFlowKey {
            guest_identifier: 1,
            dst_ip: [1, 1, 1, 1],
        };
        let key2 = IcmpFlowKey {
            guest_identifier: 2,
            dst_ip: [8, 8, 8, 8],
        };
        let key3 = IcmpFlowKey {
            guest_identifier: 3,
            dst_ip: [9, 9, 9, 9],
        };

        let first = get_or_insert_lru(&mut flows, key1, 2, || {
            Ok::<_, io::Error>(TestFlow { last_activity: 10 })
        })
        .unwrap()
        .unwrap() as *mut TestFlow;
        let again = get_or_insert_lru(&mut flows, key1, 2, || {
            Ok::<_, io::Error>(TestFlow { last_activity: 99 })
        })
        .unwrap()
        .unwrap() as *mut TestFlow;
        assert_eq!(first, again);
        assert_eq!(flows[&key1].last_activity, 10);

        get_or_insert_lru(&mut flows, key2, 2, || {
            Ok::<_, io::Error>(TestFlow { last_activity: 20 })
        })
        .unwrap();
        get_or_insert_lru(&mut flows, key3, 2, || {
            Ok::<_, io::Error>(TestFlow { last_activity: 30 })
        })
        .unwrap();
        assert!(!flows.contains_key(&key1));
        assert!(flows.contains_key(&key2));
        assert!(flows.contains_key(&key3));

        let now = 53;
        let timeout = 25;
        flows.retain(|_, flow| now - flow.last_activity <= timeout);
        assert!(!flows.contains_key(&key2));
        assert!(flows.contains_key(&key3));
    }

    #[test]
    fn arp_request_produces_gateway_reply() {
        let mut backend = NatBackend::new();

        backend.transmit(&arp_request(GUEST_MAC, GUEST_IP, GATEWAY_IP));
        let reply = backend.poll_receive().unwrap();
        assert!(backend.poll_receive().is_none());

        assert_eq!(reply.len(), 42);
        let eth = EthernetFrame::parse(&reply).unwrap();
        assert_eq!(eth.dst, GUEST_MAC);
        assert_eq!(eth.src, GATEWAY_MAC);
        assert_eq!(eth.ethertype, ETHERTYPE_ARP);
        assert_eq!(read_u16_be(eth.payload, 6), Some(ARP_OPCODE_REPLY));
        assert_eq!(read_array::<6>(eth.payload, 8), Some(GATEWAY_MAC));
        assert_eq!(read_array::<4>(eth.payload, 14), Some(GATEWAY_IP));
        assert_eq!(read_array::<6>(eth.payload, 18), Some(GUEST_MAC));
        assert_eq!(read_array::<4>(eth.payload, 24), Some(GUEST_IP));
    }

    #[test]
    fn dhcp_discover_offer_and_request_ack_echo_xid_chaddr_and_options() {
        let mut backend = NatBackend::new();
        let discover_xid = [0xde, 0xad, 0xbe, 0xef];
        let request_xid = [0xca, 0xfe, 0xba, 0xbe];

        backend.transmit(&dhcp_frame(DHCP_DISCOVER, discover_xid, GUEST_MAC));
        let offer = backend.poll_receive().unwrap();
        assert_dhcp_reply(&offer, DHCP_OFFER, discover_xid, GUEST_MAC);

        backend.transmit(&dhcp_frame(DHCP_REQUEST, request_xid, GUEST_MAC));
        let ack = backend.poll_receive().unwrap();
        assert_dhcp_reply(&ack, DHCP_ACK, request_xid, GUEST_MAC);
        assert!(backend.poll_receive().is_none());
    }

    #[test]
    fn dhcp_reply_preserves_request_ipv4_identification() {
        let mut backend = NatBackend::new();

        backend.transmit(&dhcp_frame_with_ipv4_id(
            DHCP_DISCOVER,
            [0x45, 0x67, 0x89, 0xab],
            GUEST_MAC,
            0x4567,
        ));
        let offer = backend.poll_receive().unwrap();
        assert!(backend.poll_receive().is_none());

        let (_, ip, udp) = parse_ipv4_udp_payload(&offer);
        assert_eq!(ip.identification, 0x4567);
        assert_eq!(ip.src, DHCP_SERVER_IP);
        assert_eq!(ip.dst, GUEST_IP);
        assert_eq!(udp.src_port, DHCP_SERVER_PORT);
        assert_eq!(udp.dst_port, DHCP_CLIENT_PORT);
        assert_eq!(ipv4_header_checksum(&ip.bytes[..ip.header_len]), 0);
        assert_eq!(udp_checksum(ip.src, ip.dst, udp.segment), 0);
    }

    fn assert_dhcp_reply(frame: &[u8], expected_type: u8, xid: [u8; 4], chaddr: MacAddr) {
        let (eth, ip, udp) = parse_ipv4_udp_payload(frame);
        assert_eq!(eth.dst, chaddr);
        assert_eq!(eth.src, GATEWAY_MAC);
        assert_eq!(eth.ethertype, ETHERTYPE_IPV4);
        assert_eq!(ip.src, DHCP_SERVER_IP);
        assert_eq!(ip.dst, GUEST_IP);
        assert_eq!(ip.protocol, IPV4_PROTOCOL_UDP);
        assert_eq!(ipv4_header_checksum(&ip.bytes[..ip.header_len]), 0);
        assert_eq!(udp.src_port, DHCP_SERVER_PORT);
        assert_eq!(udp.dst_port, DHCP_CLIENT_PORT);
        assert_eq!(udp_checksum(ip.src, ip.dst, udp.segment), 0);

        let payload = udp.payload;
        assert_eq!(payload.len(), DHCP_REPLY_PAYLOAD_LEN);
        assert_eq!(payload[0], 2);
        assert_eq!(payload[1], 1);
        assert_eq!(payload[2], 6);
        assert_eq!(payload[4..8], xid);
        assert_eq!(payload[16..20], GUEST_IP);
        assert_eq!(payload[20..24], DHCP_SERVER_IP);
        assert_eq!(payload[28..34], chaddr);
        assert_eq!(payload[236..240], DHCP_MAGIC_COOKIE);

        let options = &payload[240..];
        assert_eq!(
            dhcp_option(options, DHCP_OPT_MESSAGE_TYPE),
            Some(&[expected_type][..])
        );
        assert_eq!(
            dhcp_option(options, DHCP_OPT_SERVER_ID),
            Some(&DHCP_SERVER_IP[..])
        );
        assert_eq!(
            dhcp_option(options, DHCP_OPT_LEASE_TIME),
            Some(&DHCP_LEASE_SECONDS.to_be_bytes()[..])
        );
        assert_eq!(
            dhcp_option(options, DHCP_OPT_SUBNET_MASK),
            Some(&SUBNET_MASK[..])
        );
        assert_eq!(dhcp_option(options, DHCP_OPT_ROUTER), Some(&GATEWAY_IP[..]));
        assert_eq!(dhcp_option(options, DHCP_OPT_DNS), Some(&DNS_IP[..]));
    }

    #[test]
    fn icmp_echo_request_to_gateway_returns_echo_reply() {
        let mut backend = NatBackend::new();

        backend.transmit(&icmp_echo_request());
        let reply = backend.poll_receive().unwrap();
        assert!(backend.poll_receive().is_none());

        let eth = EthernetFrame::parse(&reply).unwrap();
        assert_eq!(eth.dst, GUEST_MAC);
        assert_eq!(eth.src, GATEWAY_MAC);
        assert_eq!(eth.ethertype, ETHERTYPE_IPV4);
        let ip = Ipv4Packet::parse(eth.payload).unwrap();
        assert_eq!(ip.src, GATEWAY_IP);
        assert_eq!(ip.dst, GUEST_IP);
        assert_eq!(ip.protocol, IPV4_PROTOCOL_ICMP);
        assert_eq!(ipv4_header_checksum(&ip.bytes[..ip.header_len]), 0);
        assert_eq!(ip.payload[0], 0);
        assert_eq!(ip.payload[1], 0);
        assert_eq!(
            ip.payload[4..],
            [0x12, 0x34, 0x00, 0x01, b'h', b'e', b'l', b'l', b'o']
        );
        assert_eq!(icmp_checksum(ip.payload), 0);
    }

    #[test]
    fn unrelated_ethertype_and_unknown_arp_target_produce_no_reply() {
        let mut backend = NatBackend::new();
        let unrelated = EthernetFrame::build(BROADCAST_MAC, GUEST_MAC, 0x86dd, &[1, 2, 3, 4]);
        backend.transmit(&unrelated);
        assert!(backend.poll_receive().is_none());

        backend.transmit(&arp_request(GUEST_MAC, GUEST_IP, [10, 0, 2, 99]));
        assert!(backend.poll_receive().is_none());
        assert_eq!(backend.pending_receive_len(), 0);
    }

    #[test]
    fn arp_request_for_advertised_dns_ip_produces_gateway_mac_reply() {
        let mut backend = NatBackend::new();

        backend.transmit(&arp_request(GUEST_MAC, GUEST_IP, DNS_IP));
        let reply = backend.poll_receive().unwrap();
        assert!(backend.poll_receive().is_none());

        assert_eq!(reply.len(), 42);
        let eth = EthernetFrame::parse(&reply).unwrap();
        assert_eq!(eth.dst, GUEST_MAC);
        assert_eq!(eth.src, GATEWAY_MAC);
        assert_eq!(eth.ethertype, ETHERTYPE_ARP);
        assert_eq!(read_u16_be(eth.payload, 6), Some(ARP_OPCODE_REPLY));
        assert_eq!(read_array::<6>(eth.payload, 8), Some(GATEWAY_MAC));
        assert_eq!(read_array::<4>(eth.payload, 14), Some(DNS_IP));
        assert_eq!(read_array::<6>(eth.payload, 18), Some(GUEST_MAC));
        assert_eq!(read_array::<4>(eth.payload, 24), Some(GUEST_IP));
    }

    #[test]
    fn nat_stats_count_guest_frames_replies_and_lease() {
        let mut backend = NatBackend::new();
        backend.transmit(&arp_request(GUEST_MAC, GUEST_IP, GATEWAY_IP));
        backend.transmit(&dhcp_frame(DHCP_DISCOVER, [1, 2, 3, 4], GUEST_MAC));
        backend.transmit(&dhcp_frame(DHCP_REQUEST, [5, 6, 7, 8], GUEST_MAC));
        backend.transmit(&icmp_echo_request());
        backend.transmit(&udp_guest_frame(DNS_IP, 53, 53000, b"dns-query"));
        backend.transmit(&tcp_guest_frame(
            [127, 0, 0, 1],
            80,
            49152,
            0x1000,
            0,
            TCP_FLAG_SYN,
            &[],
        ));

        let stats = backend.stats();
        assert_eq!(stats.guest_frames, 6);
        assert_eq!(stats.arp_requests, 1);
        assert_eq!(stats.dhcp_discover, 1);
        assert_eq!(stats.dhcp_request, 1);
        assert_eq!(stats.icmp_echo, 1);
        assert_eq!(stats.dns_queries, 1);
        assert_eq!(stats.tcp_segments, 1);
        assert_eq!(stats.arp_replies, 1);
        assert_eq!(stats.dhcp_offers, 1);
        assert_eq!(stats.dhcp_acks, 1);
        assert_eq!(stats.icmp_replies, 1);
        assert_eq!(stats.dhcp_lease_ip, GUEST_IP);
        assert_eq!(stats.pending_replies, 4);
        assert_eq!(stats.tcp_flow_count, 0);
        assert_eq!(stats.udp_flow_count, 0);
    }

    #[test]
    fn guest_mac_learning_uses_first_frame_source_for_replies() {
        let mut backend = NatBackend::new();
        let first = EthernetFrame::build(BROADCAST_MAC, GUEST_MAC, 0x88b5, &[0xab]);
        backend.transmit(&first);
        assert_eq!(backend.guest_mac(), Some(GUEST_MAC));

        backend.transmit(&arp_request(OTHER_GUEST_MAC, GUEST_IP, GATEWAY_IP));
        let reply = backend.poll_receive().unwrap();
        let eth = EthernetFrame::parse(&reply).unwrap();
        assert_eq!(eth.dst, GUEST_MAC);
        assert_eq!(eth.src, GATEWAY_MAC);
    }

    #[test]
    fn non_local_tcp_udp_packets_are_queued_for_stage_2b() {
        let mut backend = NatBackend::new();
        let tcp_payload = [0x12, 0x34, 0x00, 0x50, 0, 0, 0, 1];
        let ipv4 = build_ipv4_packet(
            GUEST_IP,
            [93, 184, 216, 34],
            IPV4_PROTOCOL_TCP,
            &tcp_payload,
        );
        let frame = EthernetFrame::build(GATEWAY_MAC, GUEST_MAC, ETHERTYPE_IPV4, &ipv4);

        backend.transmit(&frame);
        assert!(backend.poll_receive().is_none());
        assert_eq!(backend.queued_outbound_ipv4_len(), 1);
        assert_eq!(backend.poll_outbound_ipv4(), Some(ipv4));
    }

    #[test]
    fn synthetic_dns_udp_packets_are_queued_for_outbound_handler() {
        let mut backend = NatBackend::new();
        let frame = udp_guest_frame(DNS_IP, 53, 53000, b"dns-query");

        backend.transmit(&frame);

        assert!(backend.poll_receive().is_none());
        let packet = backend.poll_outbound_ipv4().unwrap();
        let ip = Ipv4Packet::parse(&packet).unwrap();
        let udp = UdpDatagram::parse(ip.payload).unwrap();
        assert_eq!(ip.dst, DNS_IP);
        assert_eq!(udp.dst_port, 53);
        assert_eq!(udp.payload, b"dns-query");
    }

    #[test]
    fn host_socket_udp_flow_echoes_reply_to_guest() {
        let Some(echo) = loopback_udp_socket() else {
            return;
        };
        echo.set_nonblocking(true).unwrap();
        let port = echo.local_addr().unwrap().port();
        let mut backend = NatBackend::with_outbound_handler(
            HostSocketOutboundIpv4Handler::with_dns_resolver(StdIpv4Addr::LOCALHOST),
        );

        backend.transmit(&udp_guest_frame([127, 0, 0, 1], port, 49152, b"hello"));

        let mut buf = [0u8; 64];
        let peer = loop {
            match echo.recv_from(&mut buf) {
                Ok((len, peer)) => {
                    assert_eq!(&buf[..len], b"hello");
                    break peer;
                }
                Err(e) if would_block(&e) => backend.poll_host_sockets(),
                Err(e) => panic!("udp echo recv failed: {e}"),
            }
        };
        echo.send_to(b"world", peer).unwrap();
        for _ in 0..64 {
            backend.poll_host_sockets();
            if backend.pending_receive_len() > 0 {
                break;
            }
        }

        let reply = backend.poll_receive().unwrap();
        let (eth, ip, udp) = parse_ipv4_udp_payload(&reply);
        assert_eq!(eth.dst, GUEST_MAC);
        assert_eq!(eth.src, GATEWAY_MAC);
        assert_eq!(ip.src, [127, 0, 0, 1]);
        assert_eq!(ip.dst, GUEST_IP);
        assert_eq!(udp.src_port, port);
        assert_eq!(udp.dst_port, 49152);
        assert_eq!(udp.payload, b"world");
        assert_eq!(udp_checksum(ip.src, ip.dst, udp.segment), 0);
    }

    #[test]
    fn host_socket_udp_multiple_replies_all_drain_from_receive_queue() {
        let Some(echo) = loopback_udp_socket() else {
            return;
        };
        echo.set_nonblocking(true).unwrap();
        let port = echo.local_addr().unwrap().port();
        let mut backend = NatBackend::with_outbound_handler(
            HostSocketOutboundIpv4Handler::with_dns_resolver(StdIpv4Addr::LOCALHOST),
        );

        backend.transmit(&udp_guest_frame([127, 0, 0, 1], port, 49152, b"hello"));

        let mut buf = [0u8; 64];
        let peer = loop {
            match echo.recv_from(&mut buf) {
                Ok((len, peer)) => {
                    assert_eq!(&buf[..len], b"hello");
                    break peer;
                }
                Err(e) if would_block(&e) => backend.poll_host_sockets(),
                Err(e) => panic!("udp echo recv failed: {e}"),
            }
        };
        echo.send_to(b"one", peer).unwrap();
        echo.send_to(b"two", peer).unwrap();
        for _ in 0..64 {
            backend.poll_host_sockets();
            if backend.pending_receive_len() >= 2 {
                break;
            }
        }

        let first = backend.poll_receive().unwrap();
        let second = backend.poll_receive().unwrap();
        assert!(backend.poll_receive().is_none());
        let (_, _, first_udp) = parse_ipv4_udp_payload(&first);
        let (_, _, second_udp) = parse_ipv4_udp_payload(&second);
        assert_eq!(first_udp.payload, b"one");
        assert_eq!(second_udp.payload, b"two");
        let stats = backend.stats();
        assert_eq!(stats.udp_datagrams, 1);
        assert_eq!(stats.udp_datagrams_out, 2);
        assert_eq!(stats.pending_replies, 0);
        assert_eq!(stats.udp_flow_count, 1);
    }

    #[test]
    fn host_socket_udp_flows_evict_after_idle_timeout() {
        let Some(echo) = loopback_udp_socket() else {
            return;
        };
        let port = echo.local_addr().unwrap().port();
        let handler = HostSocketOutboundIpv4Handler::with_dns_resolver(StdIpv4Addr::LOCALHOST)
            .with_idle_timeout_ticks(2);
        let mut backend = NatBackend::with_outbound_handler(handler);

        backend.transmit(&udp_guest_frame([127, 0, 0, 1], port, 49152, b"hello"));
        assert_eq!(backend.outbound_ipv4_handler().udp_flow_count(), 1);
        backend.poll_host_sockets();
        backend.poll_host_sockets();
        backend.poll_host_sockets();
        assert_eq!(backend.outbound_ipv4_handler().udp_flow_count(), 0);
    }

    #[test]
    fn host_socket_tcp_flow_proxies_data_and_fin() {
        let Some(listener) = loopback_tcp_listener() else {
            return;
        };
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let mut backend = NatBackend::<HostSocketOutboundIpv4Handler>::new_host_socket();
        let guest_isn = 0x1000_0000;
        let guest_port = 49153;

        backend.transmit(&tcp_guest_frame(
            [127, 0, 0, 1],
            port,
            guest_port,
            guest_isn,
            0,
            TCP_FLAG_SYN,
            &[],
        ));

        let mut server = None;
        let syn_ack = loop {
            backend.poll_host_sockets();
            if server.is_none() {
                match listener.accept() {
                    Ok((stream, _)) => {
                        stream.set_nonblocking(true).unwrap();
                        server = Some(stream);
                    }
                    Err(e) if would_block(&e) => {}
                    Err(e) => panic!("tcp accept failed: {e}"),
                }
            }
            if let Some(frame) = backend.poll_receive() {
                break frame;
            }
        };
        let (_, ip, tcp) = parse_ipv4_tcp(&syn_ack);
        assert_eq!(ip.src, [127, 0, 0, 1]);
        assert_eq!(
            tcp.flags & (TCP_FLAG_SYN | TCP_FLAG_ACK),
            TCP_FLAG_SYN | TCP_FLAG_ACK
        );
        assert_eq!(tcp.ack, guest_isn.wrapping_add(1));
        let host_next = tcp.seq.wrapping_add(1);

        backend.transmit(&tcp_guest_frame(
            [127, 0, 0, 1],
            port,
            guest_port,
            guest_isn.wrapping_add(1),
            host_next,
            TCP_FLAG_ACK | TCP_FLAG_PSH,
            b"ping",
        ));
        let mut server = loop {
            if let Some(stream) = server.take() {
                break stream;
            }
            backend.poll_host_sockets();
            match listener.accept() {
                Ok((stream, _)) => {
                    stream.set_nonblocking(true).unwrap();
                    break stream;
                }
                Err(e) if would_block(&e) => {}
                Err(e) => panic!("tcp accept failed: {e}"),
            }
        };
        let mut buf = [0u8; 16];
        loop {
            backend.poll_host_sockets();
            match server.read(&mut buf) {
                Ok(4) => break,
                Ok(_) => {}
                Err(e) if would_block(&e) => {}
                Err(e) => panic!("server read failed: {e}"),
            }
        }
        assert_eq!(&buf[..4], b"ping");
        server.write_all(b"pong").unwrap();
        let data = loop {
            backend.poll_host_sockets();
            if let Some(frame) = backend.poll_receive() {
                let (_, _, tcp) = parse_ipv4_tcp(&frame);
                if !tcp.payload.is_empty() {
                    break frame;
                }
            }
        };
        let (_, _, tcp) = parse_ipv4_tcp(&data);
        assert_eq!(
            tcp.flags & (TCP_FLAG_PSH | TCP_FLAG_ACK),
            TCP_FLAG_PSH | TCP_FLAG_ACK
        );
        assert_eq!(tcp.payload, b"pong");
        assert_eq!(tcp.ack, guest_isn.wrapping_add(5));

        backend.transmit(&tcp_guest_frame(
            [127, 0, 0, 1],
            port,
            guest_port,
            guest_isn.wrapping_add(5),
            tcp.seq.wrapping_add(4),
            TCP_FLAG_FIN | TCP_FLAG_ACK,
            &[],
        ));
        let _ = server.shutdown(Shutdown::Write);
        let fin = loop {
            backend.poll_host_sockets();
            if let Some(frame) = backend.poll_receive() {
                let (_, _, tcp) = parse_ipv4_tcp(&frame);
                if tcp.flags & TCP_FLAG_FIN != 0 {
                    break frame;
                }
            }
        };
        let (_, _, tcp) = parse_ipv4_tcp(&fin);
        assert_eq!(tcp.ack, guest_isn.wrapping_add(6));
    }

    #[test]
    fn host_socket_tcp_connect_to_closed_port_returns_rst() {
        let Some(listener) = loopback_tcp_listener() else {
            return;
        };
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let mut backend = NatBackend::<HostSocketOutboundIpv4Handler>::new_host_socket();

        backend.transmit(&tcp_guest_frame(
            [127, 0, 0, 1],
            port,
            49154,
            0x2000_0000,
            0,
            TCP_FLAG_SYN,
            &[],
        ));
        let rst = loop {
            backend.poll_host_sockets();
            if let Some(frame) = backend.poll_receive() {
                break frame;
            }
        };
        let (_, _, tcp) = parse_ipv4_tcp(&rst);
        assert_ne!(tcp.flags & TCP_FLAG_RST, 0);
    }

    #[test]
    fn host_socket_tcp_poll_reuses_remove_scratch() {
        let Some(listener) = loopback_tcp_listener() else {
            return;
        };
        let stream = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (_server, _) = listener.accept().unwrap();
        stream.set_nonblocking(true).unwrap();

        let mut handler = HostSocketOutboundIpv4Handler::new();
        handler.tcp_remove_scratch.reserve(8);
        let scratch_capacity = handler.tcp_remove_scratch.capacity();
        let scratch_ptr = handler.tcp_remove_scratch.as_ptr();
        let key = TcpFlowKey {
            guest_ip: GUEST_IP,
            guest_port: 49155,
            dst_ip: [127, 0, 0, 1],
            dst_port: listener.local_addr().unwrap().port(),
        };
        let mut flow = TcpFlow::new(stream, 0, 0, 0);
        flow.state = TcpProxyState::Established;
        flow.guest_fin = true;
        flow.host_fin_sent = true;
        flow.host_fin_acked = true;
        handler.tcp_flows.insert(key, flow);

        let mut replies = VecDeque::new();
        let mut stats = NatStats::default();
        handler.poll_tcp(Some(GUEST_MAC), &mut replies, &mut stats);

        assert!(handler.tcp_flows.is_empty());
        assert!(handler.tcp_remove_scratch.is_empty());
        assert_eq!(handler.tcp_remove_scratch.capacity(), scratch_capacity);
        assert_eq!(handler.tcp_remove_scratch.as_ptr(), scratch_ptr);
    }
}
