//! Split out of net_nat.rs to keep files under 850 lines.

use super::*;

use std::{
    collections::{hash_map::Entry, HashMap, VecDeque},
    io::{self, Read},
    net::{Ipv4Addr as StdIpv4Addr, Shutdown, SocketAddrV4, UdpSocket},
    time::Instant,
};

use crate::virtio_net::NetBackend;

pub type MacAddr = [u8; 6];
pub type Ipv4Addr = [u8; 4];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ipv4Endpoint {
    pub address: Ipv4Addr,
    pub port: u16,
}

impl Ipv4Endpoint {
    pub const fn new(address: Ipv4Addr, port: u16) -> Self {
        Self { address, port }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EthernetIpv4Endpoint {
    pub(crate) mac: MacAddr,
    pub(crate) network: Ipv4Endpoint,
}

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

pub(crate) const ARP_HARDWARE_ETHERNET: u16 = 1;
pub(crate) const ARP_OPCODE_REQUEST: u16 = 1;
pub(crate) const ARP_OPCODE_REPLY: u16 = 2;

pub(crate) const DHCP_CLIENT_PORT: u16 = 68;
pub(crate) const DHCP_SERVER_PORT: u16 = 67;
pub(crate) const DHCP_MAGIC_COOKIE: [u8; 4] = [99, 130, 83, 99];
pub(crate) const DHCP_FLAG_BROADCAST: u16 = 0x8000;
pub(crate) const DHCP_OPT_SUBNET_MASK: u8 = 1;
pub(crate) const DHCP_OPT_ROUTER: u8 = 3;
pub(crate) const DHCP_OPT_DNS: u8 = 6;
pub(crate) const DHCP_OPT_LEASE_TIME: u8 = 51;
pub(crate) const DHCP_OPT_MESSAGE_TYPE: u8 = 53;
pub(crate) const DHCP_OPT_SERVER_ID: u8 = 54;
pub(crate) const DHCP_OPT_END: u8 = 255;
pub(crate) const DHCP_DISCOVER: u8 = 1;
pub(crate) const DHCP_OFFER: u8 = 2;
pub(crate) const DHCP_REQUEST: u8 = 3;
pub(crate) const DHCP_ACK: u8 = 5;
pub(crate) const DHCP_LEASE_SECONDS: u32 = 86_400;
pub(crate) const DHCP_REPLY_FIXED_LEN: usize = 240;
pub(crate) const DHCP_REPLY_OPTIONS_LEN: usize = 3 + (5 * 6) + 1;
pub(crate) const DHCP_REPLY_PAYLOAD_LEN: usize = DHCP_REPLY_FIXED_LEN + DHCP_REPLY_OPTIONS_LEN;
pub(crate) const MAX_RESOLV_CONF_BYTES: u64 = 64 * 1024;
#[cfg(test)]
pub(crate) const DHCP_OPT_REQUESTED_IP: u8 = 50;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NatBackend<H = QueuedOutboundIpv4Handler> {
    pub(crate) guest_mac: Option<MacAddr>,
    pub(crate) reply_queue: VecDeque<Vec<u8>>,
    pub(crate) outbound_ipv4: H,
    pub(crate) stats: NatStats,
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
    pub(crate) fn handle_arp(&mut self, eth: &EthernetFrame<'_>) {
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

    pub(crate) fn handle_ipv4(&mut self, eth: &EthernetFrame<'_>) {
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

    pub(crate) fn handle_dhcp(&mut self, packet: &Ipv4Packet<'_>, udp: &UdpDatagram<'_>) {
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

    pub(crate) fn handle_icmp(&mut self, packet: &Ipv4Packet<'_>) {
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

    pub(crate) fn queue_gateway_icmp_echo_reply(&mut self, packet: &Ipv4Packet<'_>) -> bool {
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
    pub(crate) udp_flows: HashMap<UdpFlowKey, UdpFlow>,
    pub(crate) tcp_flows: HashMap<TcpFlowKey, TcpFlow>,
    pub(crate) icmp_flows: HashMap<IcmpFlowKey, IcmpFlow>,
    pub(crate) pending_tcp_resets: VecDeque<PendingTcpReset>,
    pub(crate) tcp_remove_scratch: Vec<TcpFlowKey>,
    pub(crate) udp_recv_scratch: [u8; HOST_SOCKET_UDP_RECV_SCRATCH_LEN],
    pub(crate) tcp_read_scratch: [u8; HOST_SOCKET_TCP_READ_SCRATCH_LEN],
    pub(crate) icmp_recv_scratch: [u8; HOST_SOCKET_ICMP_RECV_SCRATCH_LEN],
    pub(crate) pending_socket_errors: u64,
    pub(crate) dns_resolver: StdIpv4Addr,
    pub(crate) epoch: Instant,
    pub(crate) idle_timeout_ms: u64,
    pub(crate) max_flows: usize,
    pub(crate) max_icmp_flows: usize,
    pub(crate) tcp_isn_counter: u32,
}

pub(crate) const HOST_SOCKET_UDP_RECV_SCRATCH_LEN: usize = 2048;
pub(crate) const HOST_SOCKET_TCP_READ_SCRATCH_LEN: usize = 1460;
pub(crate) const HOST_SOCKET_ICMP_RECV_SCRATCH_LEN: usize = 2048;

impl Default for HostSocketOutboundIpv4Handler {
    fn default() -> Self {
        Self::new()
    }
}

impl HostSocketOutboundIpv4Handler {
    // Wall-clock idle sweep. Flow stamps advance only on real activity, so
    // this must comfortably exceed legitimate quiet periods inside a live
    // connection (TLS setup, server-side git pack computation).
    pub(crate) const DEFAULT_IDLE_TIMEOUT_MS: u64 = 300_000;
    pub(crate) const DEFAULT_MAX_FLOWS: usize = 256;
    pub(crate) const DEFAULT_MAX_ICMP_FLOWS: usize = 32;
    pub(crate) const MAX_ICMP_RECV_PER_POLL: usize = 64;

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
            epoch: Instant::now(),
            idle_timeout_ms: Self::DEFAULT_IDLE_TIMEOUT_MS,
            max_flows: Self::DEFAULT_MAX_FLOWS,
            max_icmp_flows: Self::DEFAULT_MAX_ICMP_FLOWS,
            tcp_isn_counter: 0x4256_0000,
        }
    }

    #[cfg(test)]
    pub(crate) fn with_idle_timeout_ms(mut self, ms: u64) -> Self {
        self.idle_timeout_ms = ms;
        self
    }

    #[cfg(test)]
    pub(crate) fn udp_flow_count(&self) -> usize {
        self.udp_flows.len()
    }

    pub(crate) fn now_ms(&self) -> u64 {
        u64::try_from(self.epoch.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    pub(crate) fn next_tcp_isn(&mut self) -> u32 {
        self.tcp_isn_counter = self.tcp_isn_counter.wrapping_add(0x1f3d_5b79);
        self.tcp_isn_counter
    }

    pub(crate) fn evict_idle_flows(&mut self) {
        let now = self.now_ms();
        let timeout = self.idle_timeout_ms;
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

    pub(crate) fn get_or_create_icmp_flow(
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

    pub(crate) fn handle_icmp(&mut self, packet: &Ipv4Packet<'_>) -> io::Result<()> {
        let now = self.now_ms();
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

    pub(crate) fn handle_udp(&mut self, packet: &Ipv4Packet<'_>, udp: &UdpDatagram<'_>) {
        let now = self.now_ms();
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

    pub(crate) fn handle_tcp(&mut self, packet: &Ipv4Packet<'_>, tcp: &TcpSegment<'_>) {
        let now = self.now_ms();
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

    pub(crate) fn poll_udp(
        &mut self,
        guest_mac: Option<MacAddr>,
        reply_queue: &mut VecDeque<Vec<u8>>,
        stats: &mut NatStats,
    ) {
        let Some(guest_mac) = guest_mac else {
            return;
        };
        let now = self.now_ms();
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

    pub(crate) fn poll_tcp(
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
        let now = self.now_ms();
        {
            let remove_scratch = &mut self.tcp_remove_scratch;
            let read_scratch = &mut self.tcp_read_scratch;
            remove_scratch.clear();
            for (key, flow) in &mut self.tcp_flows {
                // Stamp activity only when the flow actually did something
                // this poll. Refreshing every flow unconditionally disabled
                // idle eviction and flattened the LRU order, so a saturated
                // table evicted arbitrary entries - including active bulk
                // transfers (observed live as mid-download connection
                // resets). Guest-driven activity is stamped in the packet
                // handlers.
                let mut active = false;
                if flow.state == TcpProxyState::Connecting {
                    match tcp_connect_error(&flow.stream) {
                        Ok(Some(0)) => {
                            active = true;
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
                    let write_backlog = flow.write_buf.len();
                    flow.flush_host_write();
                    if flow.write_buf.len() != write_backlog {
                        active = true;
                    }
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
                        active = true;
                    }
                    loop {
                        match flow.stream.read(read_scratch) {
                            Ok(0) => {
                                if !flow.host_fin_sent {
                                    active = true;
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
                                active = true;
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
                if active {
                    flow.last_activity = now;
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

    pub(crate) fn poll_icmp(
        &mut self,
        guest_mac: Option<MacAddr>,
        reply_queue: &mut VecDeque<Vec<u8>>,
        stats: &mut NatStats,
    ) {
        let Some(guest_mac) = guest_mac else {
            return;
        };
        let now = self.now_ms();
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
