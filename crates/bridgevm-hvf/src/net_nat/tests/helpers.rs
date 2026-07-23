//! Split test module.

use super::super::*;
use std::{
    io::{self},
    net::{Ipv4Addr as StdIpv4Addr, SocketAddrV4, UdpSocket},
};

pub(super) const GUEST_MAC: MacAddr = [0x52, 0x54, 0x00, 0x12, 0x34, 0x56];
pub(super) const OTHER_GUEST_MAC: MacAddr = [0x52, 0x54, 0x00, 0xaa, 0xbb, 0xcc];
pub(super) const BROADCAST_MAC: MacAddr = [0xff; 6];

pub(super) fn temp_resolv_path(label: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "bridgevm-resolv-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

pub(super) fn arp_request(src_mac: MacAddr, sender_ip: Ipv4Addr, target_ip: Ipv4Addr) -> Vec<u8> {
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

pub(super) fn icmp_echo_request() -> Vec<u8> {
    icmp_echo_frame(GATEWAY_IP, 0x1234)
}

pub(super) fn icmp_echo_frame(dst_ip: Ipv4Addr, identifier: u16) -> Vec<u8> {
    let mut icmp = vec![8, 0, 0, 0, 0x12, 0x34, 0x00, 0x01];
    icmp[4..6].copy_from_slice(&identifier.to_be_bytes());
    icmp.extend_from_slice(b"hello");
    let checksum = icmp_checksum(&icmp);
    icmp[2..4].copy_from_slice(&checksum.to_be_bytes());
    let ipv4 = build_ipv4_packet(GUEST_IP, dst_ip, IPV4_PROTOCOL_ICMP, &icmp);
    EthernetFrame::build(GATEWAY_MAC, GUEST_MAC, ETHERTYPE_IPV4, &ipv4)
}

pub(super) fn dhcp_payload(message_type: u8, xid: [u8; 4], chaddr: MacAddr) -> Vec<u8> {
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

pub(super) fn dhcp_frame(message_type: u8, xid: [u8; 4], chaddr: MacAddr) -> Vec<u8> {
    dhcp_frame_with_ipv4_id(message_type, xid, chaddr, 0)
}

pub(super) fn dhcp_frame_with_ipv4_id(
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

pub(super) fn parse_ipv4_udp_payload(
    frame: &[u8],
) -> (EthernetFrame<'_>, Ipv4Packet<'_>, UdpDatagram<'_>) {
    let eth = EthernetFrame::parse(frame).unwrap();
    let ip = Ipv4Packet::parse(eth.payload).unwrap();
    let udp = UdpDatagram::parse(ip.payload).unwrap();
    (eth, ip, udp)
}

pub(super) fn udp_guest_frame(
    dst_ip: Ipv4Addr,
    dst_port: u16,
    src_port: u16,
    payload: &[u8],
) -> Vec<u8> {
    let udp = build_udp_datagram(GUEST_IP, dst_ip, src_port, dst_port, payload);
    let ipv4 = build_ipv4_packet(GUEST_IP, dst_ip, IPV4_PROTOCOL_UDP, &udp);
    EthernetFrame::build(GATEWAY_MAC, GUEST_MAC, ETHERTYPE_IPV4, &ipv4)
}

pub(super) fn tcp_guest_frame(
    dst_ip: Ipv4Addr,
    dst_port: u16,
    src_port: u16,
    seq: u32,
    ack: u32,
    flags: u8,
    payload: &[u8],
) -> Vec<u8> {
    let tcp = build_tcp_segment(
        Ipv4Endpoint::new(GUEST_IP, src_port),
        Ipv4Endpoint::new(dst_ip, dst_port),
        seq,
        ack,
        flags,
        payload,
    );
    let ipv4 = build_ipv4_packet(GUEST_IP, dst_ip, IPV4_PROTOCOL_TCP, &tcp);
    EthernetFrame::build(GATEWAY_MAC, GUEST_MAC, ETHERTYPE_IPV4, &ipv4)
}

pub(super) fn parse_ipv4_tcp(frame: &[u8]) -> (EthernetFrame<'_>, Ipv4Packet<'_>, TcpSegment<'_>) {
    let eth = EthernetFrame::parse(frame).unwrap();
    let ip = Ipv4Packet::parse(eth.payload).unwrap();
    let tcp = TcpSegment::parse(ip.payload).unwrap();
    (eth, ip, tcp)
}

pub(super) fn loopback_udp_socket() -> Option<UdpSocket> {
    match UdpSocket::bind(SocketAddrV4::new(StdIpv4Addr::LOCALHOST, 0)) {
        Ok(socket) => Some(socket),
        Err(e) if e.kind() == io::ErrorKind::PermissionDenied => None,
        Err(e) => panic!("loopback udp bind failed: {e}"),
    }
}

pub(super) fn loopback_tcp_listener() -> Option<std::net::TcpListener> {
    match std::net::TcpListener::bind(SocketAddrV4::new(StdIpv4Addr::LOCALHOST, 0)) {
        Ok(listener) => Some(listener),
        Err(e) if e.kind() == io::ErrorKind::PermissionDenied => None,
        Err(e) => panic!("loopback tcp bind failed: {e}"),
    }
}

#[derive(Debug)]
pub(super) struct TestFlow {
    pub(super) last_activity: u64,
}

impl HasLastActivity for TestFlow {
    fn last_activity(&self) -> u64 {
        self.last_activity
    }
}

pub(super) fn assert_dhcp_reply(frame: &[u8], expected_type: u8, xid: [u8; 4], chaddr: MacAddr) {
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
