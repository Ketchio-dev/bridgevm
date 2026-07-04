//! Deterministic userspace NAT control plane for virtio-net.
//!
//! Stage 2a is deliberately socket-free: guest Ethernet frames enter through
//! `NetBackend::transmit`, local control-plane replies are queued for
//! `poll_receive`, and non-local TCP/UDP IPv4 packets are handed to the
//! `OutboundIpv4Handler` seam below. Stage 2b can replace the default queued
//! handler with a socket-backed handler without changing the virtio-net device
//! model.

use std::collections::VecDeque;

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

impl<H: OutboundIpv4Handler> NetBackend for NatBackend<H> {
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
            if is_non_local_ipv4_destination(packet.dst) {
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
}
