//! Deterministic userspace NAT control plane for virtio-net.
//!
//! Stage 2a is deliberately socket-free: guest Ethernet frames enter through
//! `NetBackend::transmit`, local control-plane replies are queued for
//! `poll_receive`, and non-local TCP/UDP IPv4 packets are handed to the
//! `OutboundIpv4Handler` seam below. Stage 2b can replace the default queued
//! handler with a socket-backed handler without changing the virtio-net device
//! model.

use std::{
    collections::{HashMap, VecDeque},
    io::{self, Read, Write},
    net::{IpAddr, Ipv4Addr as StdIpv4Addr, Shutdown, SocketAddrV4, TcpStream, UdpSocket},
    os::fd::{FromRawFd, RawFd},
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
    ) {
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
}

impl<H: OutboundIpv4Handler + Send> NetBackend for NatBackend<H> {
    fn transmit(&mut self, frame: &[u8]) {
        let Some(eth) = EthernetFrame::parse(frame) else {
            return;
        };
        if self.guest_mac.is_none() {
            self.guest_mac = Some(eth.src);
        }

        match eth.ethertype {
            ETHERTYPE_ARP => self.handle_arp(&eth),
            ETHERTYPE_IPV4 => self.handle_ipv4(&eth),
            _ => {}
        }
    }

    fn poll_receive(&mut self) -> Option<Vec<u8>> {
        self.reply_queue.pop_front()
    }

    fn poll_host_sockets(&mut self) {
        self.outbound_ipv4
            .poll_host_sockets(self.guest_mac, &mut self.reply_queue);
    }
}

impl<H: OutboundIpv4Handler> NatBackend<H> {
    fn handle_arp(&mut self, eth: &EthernetFrame<'_>) {
        let Some(request) = ArpPacket::parse(eth.payload) else {
            return;
        };
        if request.opcode != ARP_OPCODE_REQUEST || request.target_ip != GATEWAY_IP {
            return;
        }

        let mut payload = Vec::with_capacity(28);
        payload.extend_from_slice(&ARP_HARDWARE_ETHERNET.to_be_bytes());
        payload.extend_from_slice(&ETHERTYPE_IPV4.to_be_bytes());
        payload.push(6);
        payload.push(4);
        payload.extend_from_slice(&ARP_OPCODE_REPLY.to_be_bytes());
        payload.extend_from_slice(&GATEWAY_MAC);
        payload.extend_from_slice(&GATEWAY_IP);
        payload.extend_from_slice(&request.sender_mac);
        payload.extend_from_slice(&request.sender_ip);
        self.queue_ethernet(ETHERTYPE_ARP, &payload);
    }

    fn handle_ipv4(&mut self, eth: &EthernetFrame<'_>) {
        let Some(packet) = Ipv4Packet::parse(eth.payload) else {
            return;
        };

        if packet.protocol == IPV4_PROTOCOL_UDP {
            let Some(udp) = UdpDatagram::parse(packet.payload) else {
                return;
            };
            if udp.src_port == DHCP_CLIENT_PORT && udp.dst_port == DHCP_SERVER_PORT {
                self.handle_dhcp(&packet, &udp);
                return;
            }
            if (packet.dst == DNS_IP && udp.dst_port == 53)
                || is_non_local_ipv4_destination(packet.dst)
            {
                self.outbound_ipv4.handle_outbound_ipv4(&packet);
            }
            return;
        }

        if packet.protocol == IPV4_PROTOCOL_ICMP {
            self.handle_icmp(&packet);
            return;
        }

        if packet.protocol == IPV4_PROTOCOL_TCP && is_non_local_ipv4_destination(packet.dst) {
            self.outbound_ipv4.handle_outbound_ipv4(&packet);
        }
    }

    fn handle_dhcp(&mut self, packet: &Ipv4Packet<'_>, udp: &UdpDatagram<'_>) {
        let Some(request) = DhcpRequest::parse(udp.payload) else {
            return;
        };
        let reply_type = match request.message_type {
            DHCP_DISCOVER => DHCP_OFFER,
            DHCP_REQUEST => DHCP_ACK,
            _ => return,
        };

        let payload = build_dhcp_reply(&request, reply_type);
        let dst_ip = dhcp_reply_destination(&request);
        let udp = build_udp_datagram(
            DHCP_SERVER_IP,
            dst_ip,
            DHCP_SERVER_PORT,
            DHCP_CLIENT_PORT,
            &payload,
        );
        let ipv4 = build_ipv4_packet_with_id(
            DHCP_SERVER_IP,
            dst_ip,
            IPV4_PROTOCOL_UDP,
            &udp,
            packet.identification,
        );
        self.queue_ethernet(ETHERTYPE_IPV4, &ipv4);
    }

    fn handle_icmp(&mut self, packet: &Ipv4Packet<'_>) {
        if packet.dst != GATEWAY_IP || packet.payload.len() < 8 {
            return;
        }
        if packet.payload[0] != 8 || packet.payload[1] != 0 {
            return;
        }

        let mut reply = packet.payload.to_vec();
        reply[0] = 0;
        reply[2] = 0;
        reply[3] = 0;
        let checksum = icmp_checksum(&reply);
        reply[2..4].copy_from_slice(&checksum.to_be_bytes());

        let ipv4 = build_ipv4_packet_with_id(
            GATEWAY_IP,
            packet.src,
            IPV4_PROTOCOL_ICMP,
            &reply,
            packet.identification,
        );
        self.queue_ethernet(ETHERTYPE_IPV4, &ipv4);
    }

    fn queue_ethernet(&mut self, ethertype: u16, payload: &[u8]) {
        let Some(dst_mac) = self.guest_mac else {
            return;
        };
        self.reply_queue.push_back(EthernetFrame::build(
            dst_mac,
            GATEWAY_MAC,
            ethertype,
            payload,
        ));
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
    pending_tcp_resets: VecDeque<PendingTcpReset>,
    dns_resolver: StdIpv4Addr,
    tick: u64,
    idle_timeout_ticks: u64,
    max_flows: usize,
    tcp_isn_counter: u32,
}

impl Default for HostSocketOutboundIpv4Handler {
    fn default() -> Self {
        Self::new()
    }
}

impl HostSocketOutboundIpv4Handler {
    const DEFAULT_IDLE_TIMEOUT_TICKS: u64 = 30_000;
    const DEFAULT_MAX_FLOWS: usize = 256;

    pub fn new() -> Self {
        Self::with_dns_resolver(
            first_resolv_conf_nameserver().unwrap_or(StdIpv4Addr::new(1, 1, 1, 1)),
        )
    }

    pub fn with_dns_resolver(dns_resolver: StdIpv4Addr) -> Self {
        Self {
            udp_flows: HashMap::new(),
            tcp_flows: HashMap::new(),
            pending_tcp_resets: VecDeque::new(),
            dns_resolver,
            tick: 0,
            idle_timeout_ticks: Self::DEFAULT_IDLE_TIMEOUT_TICKS,
            max_flows: Self::DEFAULT_MAX_FLOWS,
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
        evict_lru(&mut self.udp_flows, self.max_flows);
        evict_lru(&mut self.tcp_flows, self.max_flows);
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
        if !self.udp_flows.contains_key(&key) {
            match UdpSocket::bind(SocketAddrV4::new(StdIpv4Addr::UNSPECIFIED, 0)).and_then(
                |socket| {
                    socket.set_nonblocking(true)?;
                    socket.connect(SocketAddrV4::new(StdIpv4Addr::from(socket_dst), dst_port))?;
                    Ok(socket)
                },
            ) {
                Ok(socket) => {
                    self.udp_flows.insert(
                        key,
                        UdpFlow {
                            socket,
                            last_activity: now,
                        },
                    );
                }
                Err(_) => return,
            }
        }
        if let Some(flow) = self.udp_flows.get_mut(&key) {
            flow.last_activity = now;
            let _ = flow.socket.send(udp.payload);
        }
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
        if !tcp.payload.is_empty() {
            if tcp.seq == flow.guest_next {
                flow.guest_next = flow.guest_next.wrapping_add(tcp.payload.len() as u32);
                flow.write_buf.extend(tcp.payload);
                flow.pending_ack = true;
                flow.flush_host_write();
            }
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

    fn poll_udp(&mut self, guest_mac: Option<MacAddr>, reply_queue: &mut VecDeque<Vec<u8>>) {
        let Some(guest_mac) = guest_mac else {
            return;
        };
        let now = self.tick;
        let mut buf = [0u8; 2048];
        for (key, flow) in &mut self.udp_flows {
            loop {
                match flow.socket.recv(&mut buf) {
                    Ok(len) => {
                        flow.last_activity = now;
                        queue_udp_reply(
                            reply_queue,
                            guest_mac,
                            key.public_dst,
                            key.guest_ip,
                            key.public_dst_port,
                            key.guest_port,
                            &buf[..len],
                        );
                    }
                    Err(e) if would_block(&e) => break,
                    Err(_) => break,
                }
            }
        }
    }

    fn poll_tcp(&mut self, guest_mac: Option<MacAddr>, reply_queue: &mut VecDeque<Vec<u8>>) {
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
        }
        let now = self.tick;
        let mut remove = Vec::new();
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
                        remove.push(*key);
                        continue;
                    }
                    Ok(None) => {}
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
                    flow.pending_ack = false;
                }
                let mut buf = [0u8; 1460];
                loop {
                    match flow.stream.read(&mut buf) {
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
                                &buf[..len],
                            );
                            flow.our_next = flow.our_next.wrapping_add(len as u32);
                        }
                        Err(e) if would_block(&e) => break,
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
                            remove.push(*key);
                            break;
                        }
                    }
                }
            }
            if flow.closed() {
                remove.push(*key);
            }
        }
        for key in remove {
            self.tcp_flows.remove(&key);
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
            _ => {}
        }
    }

    fn poll_host_sockets(
        &mut self,
        guest_mac: Option<MacAddr>,
        reply_queue: &mut VecDeque<Vec<u8>>,
    ) {
        self.bump_tick();
        self.poll_udp(guest_mac, reply_queue);
        self.poll_tcp(guest_mac, reply_queue);
        self.evict_idle_flows();
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

fn build_dhcp_reply(request: &DhcpRequest, message_type: u8) -> Vec<u8> {
    let mut msg = vec![0u8; 240];
    msg[0] = 2;
    msg[1] = request.htype;
    msg[2] = request.hlen;
    msg[4..8].copy_from_slice(&request.xid);
    msg[10..12].copy_from_slice(&request.flags.to_be_bytes());
    msg[16..20].copy_from_slice(&GUEST_IP);
    msg[20..24].copy_from_slice(&DHCP_SERVER_IP);
    msg[28..44].copy_from_slice(&request.chaddr);
    msg[236..240].copy_from_slice(&DHCP_MAGIC_COOKIE);

    push_dhcp_option(&mut msg, DHCP_OPT_MESSAGE_TYPE, &[message_type]);
    push_dhcp_option(&mut msg, DHCP_OPT_SERVER_ID, &DHCP_SERVER_IP);
    push_dhcp_option(
        &mut msg,
        DHCP_OPT_LEASE_TIME,
        &DHCP_LEASE_SECONDS.to_be_bytes(),
    );
    push_dhcp_option(&mut msg, DHCP_OPT_SUBNET_MASK, &SUBNET_MASK);
    push_dhcp_option(&mut msg, DHCP_OPT_ROUTER, &GATEWAY_IP);
    push_dhcp_option(&mut msg, DHCP_OPT_DNS, &DNS_IP);
    msg.push(DHCP_OPT_END);
    msg
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

fn first_resolv_conf_nameserver() -> Option<StdIpv4Addr> {
    let contents = std::fs::read_to_string("/etc/resolv.conf").ok()?;
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
    let udp = build_udp_datagram(src_ip, dst_ip, src_port, dst_port, payload);
    let ipv4 = build_ipv4_packet(src_ip, dst_ip, IPV4_PROTOCOL_UDP, &udp);
    reply_queue.push_back(EthernetFrame::build(
        guest_mac,
        GATEWAY_MAC,
        ETHERTYPE_IPV4,
        &ipv4,
    ));
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
    let tcp = build_tcp_segment(
        key.dst_ip,
        key.guest_ip,
        key.dst_port,
        key.guest_port,
        seq,
        ack,
        flags,
        payload,
    );
    let ipv4 = build_ipv4_packet(key.dst_ip, key.guest_ip, IPV4_PROTOCOL_TCP, &tcp);
    reply_queue.push_back(EthernetFrame::build(
        guest_mac,
        GATEWAY_MAC,
        ETHERTYPE_IPV4,
        &ipv4,
    ));
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
    fn close(fd: RawFd) -> i32;
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
        let mut icmp = vec![8, 0, 0, 0, 0x12, 0x34, 0x00, 0x01];
        icmp.extend_from_slice(b"hello");
        let checksum = icmp_checksum(&icmp);
        icmp[2..4].copy_from_slice(&checksum.to_be_bytes());
        let ipv4 = build_ipv4_packet(GUEST_IP, GATEWAY_IP, IPV4_PROTOCOL_ICMP, &icmp);
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
        let payload = dhcp_payload(message_type, xid, chaddr);
        let udp = build_udp_datagram(
            [0, 0, 0, 0],
            IPV4_BROADCAST,
            DHCP_CLIENT_PORT,
            DHCP_SERVER_PORT,
            &payload,
        );
        let ipv4 = build_ipv4_packet([0, 0, 0, 0], IPV4_BROADCAST, IPV4_PROTOCOL_UDP, &udp);
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

    #[test]
    fn arp_request_produces_gateway_reply() {
        let mut backend = NatBackend::new();

        backend.transmit(&arp_request(GUEST_MAC, GUEST_IP, GATEWAY_IP));
        let reply = backend.poll_receive().unwrap();
        assert!(backend.poll_receive().is_none());

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

        backend.transmit(&arp_request(GUEST_MAC, GUEST_IP, DNS_IP));
        assert!(backend.poll_receive().is_none());
        assert_eq!(backend.pending_receive_len(), 0);
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
        let mut server = server.unwrap();
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
}
