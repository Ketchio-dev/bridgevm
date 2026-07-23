//! Split out of net_nat.rs to keep files under 850 lines.

use super::*;

use std::{
    collections::VecDeque,
    io::Write,
    net::{TcpStream, UdpSocket},
};

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
            IPV4_PROTOCOL_ICMP if self.handle_icmp(packet).is_err() => {
                self.pending_socket_errors = self.pending_socket_errors.saturating_add(1);
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
pub(crate) struct UdpFlowKey {
    pub(crate) guest_ip: Ipv4Addr,
    pub(crate) guest_port: u16,
    pub(crate) public_dst: Ipv4Addr,
    pub(crate) public_dst_port: u16,
    pub(crate) socket_dst: Ipv4Addr,
    pub(crate) socket_dst_port: u16,
}

#[derive(Debug)]
pub(crate) struct UdpFlow {
    pub(crate) socket: UdpSocket,
    pub(crate) last_activity: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct IcmpFlowKey {
    pub(crate) guest_identifier: u16,
    pub(crate) dst_ip: Ipv4Addr,
}

#[derive(Debug)]
pub(crate) struct IcmpFlow {
    pub(crate) socket: RawIcmpSocket,
    pub(crate) guest_ip: Ipv4Addr,
    pub(crate) last_activity: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct TcpFlowKey {
    pub(crate) guest_ip: Ipv4Addr,
    pub(crate) guest_port: u16,
    pub(crate) dst_ip: Ipv4Addr,
    pub(crate) dst_port: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PendingTcpReset {
    pub(crate) key: TcpFlowKey,
    pub(crate) seq: u32,
    pub(crate) ack: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TcpProxyState {
    Connecting,
    Established,
}

#[derive(Debug)]
pub(crate) struct TcpFlow {
    pub(crate) stream: TcpStream,
    pub(crate) state: TcpProxyState,
    pub(crate) guest_next: u32,
    pub(crate) our_seq: u32,
    pub(crate) our_next: u32,
    pub(crate) write_buf: VecDeque<u8>,
    pub(crate) pending_ack: bool,
    pub(crate) guest_fin: bool,
    pub(crate) host_fin_sent: bool,
    pub(crate) host_fin_acked: bool,
    pub(crate) last_activity: u64,
}

impl TcpFlow {
    pub(crate) fn new(
        stream: TcpStream,
        guest_next: u32,
        our_seq: u32,
        last_activity: u64,
    ) -> Self {
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

    pub(crate) fn observe_guest_ack(&mut self, ack: u32) {
        if self.host_fin_sent && ack == self.our_next {
            self.host_fin_acked = true;
        }
    }

    pub(crate) fn flush_host_write(&mut self) {
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

    pub(crate) fn closed(&self) -> bool {
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

pub(crate) const TCP_FLAG_FIN: u8 = 0x01;
pub(crate) const TCP_FLAG_SYN: u8 = 0x02;
pub(crate) const TCP_FLAG_RST: u8 = 0x04;
pub(crate) const TCP_FLAG_PSH: u8 = 0x08;
pub(crate) const TCP_FLAG_ACK: u8 = 0x10;

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

pub(crate) fn build_arp_reply_frame(
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
pub(crate) struct ArpPacket {
    pub(crate) opcode: u16,
    pub(crate) sender_mac: MacAddr,
    pub(crate) sender_ip: Ipv4Addr,
    pub(crate) target_ip: Ipv4Addr,
}

impl ArpPacket {
    pub(crate) fn parse(payload: &[u8]) -> Option<Self> {
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
pub(crate) struct DhcpRequest {
    pub(crate) htype: u8,
    pub(crate) hlen: u8,
    pub(crate) xid: [u8; 4],
    pub(crate) flags: u16,
    pub(crate) ciaddr: Ipv4Addr,
    pub(crate) chaddr: [u8; 16],
    pub(crate) message_type: u8,
}

impl DhcpRequest {
    pub(crate) fn parse(bytes: &[u8]) -> Option<Self> {
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

pub(crate) fn build_ipv4_packet_with_id(
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

pub(crate) fn build_icmp_echo_reply_frame(
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

pub(crate) fn build_rewritten_icmp_echo_reply_frame(
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

pub(crate) fn build_udp_reply_frame(
    dst_mac: MacAddr,
    src_mac: MacAddr,
    src_ip: Ipv4Addr,
    dst_ip: Ipv4Addr,
    src_port: u16,
    dst_port: u16,
    payload: &[u8],
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
    frame.extend_from_slice(&0u16.to_be_bytes());
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

pub(crate) fn build_dhcp_reply_frame(
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

pub(crate) fn build_tcp_reply_frame(
    source: EthernetIpv4Endpoint,
    destination: EthernetIpv4Endpoint,
    seq: u32,
    ack: u32,
    flags: u8,
    payload: &[u8],
) -> Option<Vec<u8>> {
    let tcp_len = 20usize.checked_add(payload.len())?;
    let ipv4_len = 20usize.checked_add(tcp_len)?;
    let total_len = u16::try_from(ipv4_len).ok()?;
    let mut frame = Vec::with_capacity(14 + ipv4_len);
    frame.extend_from_slice(&destination.mac);
    frame.extend_from_slice(&source.mac);
    frame.extend_from_slice(&ETHERTYPE_IPV4.to_be_bytes());
    frame.push(0x45);
    frame.push(0);
    frame.extend_from_slice(&total_len.to_be_bytes());
    frame.extend_from_slice(&0u16.to_be_bytes());
    frame.extend_from_slice(&0u16.to_be_bytes());
    frame.push(64);
    frame.push(IPV4_PROTOCOL_TCP);
    frame.extend_from_slice(&0u16.to_be_bytes());
    frame.extend_from_slice(&source.network.address);
    frame.extend_from_slice(&destination.network.address);
    let ipv4_header_start = 14;
    let ipv4_payload_start = ipv4_header_start + 20;
    let checksum = ipv4_header_checksum(&frame[ipv4_header_start..ipv4_payload_start]);
    frame[ipv4_header_start + 10..ipv4_header_start + 12].copy_from_slice(&checksum.to_be_bytes());

    frame.extend_from_slice(&source.network.port.to_be_bytes());
    frame.extend_from_slice(&destination.network.port.to_be_bytes());
    frame.extend_from_slice(&seq.to_be_bytes());
    frame.extend_from_slice(&ack.to_be_bytes());
    frame.push(5 << 4);
    frame.push(flags);
    frame.extend_from_slice(&65535u16.to_be_bytes());
    frame.extend_from_slice(&0u16.to_be_bytes());
    frame.extend_from_slice(&0u16.to_be_bytes());
    frame.extend_from_slice(payload);
    let checksum = tcp_checksum(
        source.network.address,
        destination.network.address,
        &frame[ipv4_payload_start..],
    );
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
    source: Ipv4Endpoint,
    destination: Ipv4Endpoint,
    seq: u32,
    ack: u32,
    flags: u8,
    payload: &[u8],
) -> Vec<u8> {
    let mut segment = Vec::with_capacity(20 + payload.len());
    segment.extend_from_slice(&source.port.to_be_bytes());
    segment.extend_from_slice(&destination.port.to_be_bytes());
    segment.extend_from_slice(&seq.to_be_bytes());
    segment.extend_from_slice(&ack.to_be_bytes());
    segment.push(5 << 4);
    segment.push(flags);
    segment.extend_from_slice(&65535u16.to_be_bytes());
    segment.extend_from_slice(&0u16.to_be_bytes());
    segment.extend_from_slice(&0u16.to_be_bytes());
    segment.extend_from_slice(payload);
    let checksum = tcp_checksum(source.address, destination.address, &segment);
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

pub(crate) fn checksum_add_bytes(mut sum: u32, bytes: &[u8]) -> u32 {
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

pub(crate) fn checksum_finalize(mut sum: u32) -> u16 {
    while sum >> 16 != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !(sum as u16)
}

pub(crate) fn push_dhcp_option(msg: &mut Vec<u8>, code: u8, value: &[u8]) {
    let len = u8::try_from(value.len()).expect("DHCP option too long");
    msg.push(code);
    msg.push(len);
    msg.extend_from_slice(value);
}

pub(crate) fn dhcp_reply_destination(request: &DhcpRequest) -> Ipv4Addr {
    if request.ciaddr != [0, 0, 0, 0] {
        request.ciaddr
    } else if request.flags & DHCP_FLAG_BROADCAST != 0 {
        IPV4_BROADCAST
    } else {
        GUEST_IP
    }
}

pub(crate) fn dhcp_option(options: &[u8], code: u8) -> Option<&[u8]> {
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

pub(crate) fn is_non_local_ipv4_destination(dst: Ipv4Addr) -> bool {
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
pub(crate) enum IcmpEchoRoute {
    Gateway,
    External,
    Other,
}

pub(crate) fn classify_icmp_echo(dst: Ipv4Addr, payload: &[u8]) -> IcmpEchoRoute {
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

pub(crate) fn icmp_reply_payload_offset(buf: &[u8]) -> Option<usize> {
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
pub(crate) fn rewrite_icmp_echo_reply_identifier(
    reply: &[u8],
    guest_identifier: u16,
) -> Option<Vec<u8>> {
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
