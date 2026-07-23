//! Split test module.

use super::super::*;
use crate::virtio_net::NetBackend;
use std::{
    collections::{HashMap, VecDeque},
    io::{self, Read, Write},
    net::{Ipv4Addr as StdIpv4Addr, Shutdown, TcpStream},
    os::fd::RawFd,
};

use super::helpers::*;

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

#[test]
fn checksum_helpers_match_known_good_vectors() {
    let mut ipv4_header = Vec::from([
        0x45, 0x00, 0x00, 0x54, 0x00, 0x00, 0x40, 0x00, 0x40, 0x01, 0x00, 0x00, 0xc0, 0xa8, 0x00,
        0x01, 0xc0, 0xa8, 0x00, 0xc7,
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
        0x46, 0x00, 0x00, 0x20, 0x00, 0x00, 0x40, 0x00, 0x40, 0x01, 0x00, 0x00, 0x01, 0x01, 0x01,
        0x01, 0x0a, 0x00, 0x02, 0x0f, 0x01, 0x02, 0x03, 0x04,
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
        .with_idle_timeout_ms(2);
    let mut backend = NatBackend::with_outbound_handler(handler);

    backend.transmit(&udp_guest_frame([127, 0, 0, 1], port, 49152, b"hello"));
    assert_eq!(backend.outbound_ipv4_handler().udp_flow_count(), 1);
    std::thread::sleep(std::time::Duration::from_millis(10));
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
fn host_socket_idle_tcp_flow_is_not_refreshed_by_polling() {
    let Some(listener) = loopback_tcp_listener() else {
        return;
    };
    listener.set_nonblocking(true).unwrap();
    let port = listener.local_addr().unwrap().port();
    let mut backend = NatBackend::<HostSocketOutboundIpv4Handler>::new_host_socket();
    let guest_isn = 0x2000_0000;
    let guest_port = 49177;

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
    loop {
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
        if backend.poll_receive().is_some() {
            break;
        }
    }
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

    let idle_stamp = {
        let flow = backend
            .outbound_ipv4
            .tcp_flows
            .values()
            .next()
            .expect("tcp flow present");
        flow.last_activity
    };

    // An established flow with no traffic must not be refreshed by bare
    // polling: the old unconditional refresh disabled idle eviction and
    // flattened LRU ordering, so full tables evicted active transfers.
    for _ in 0..32 {
        backend.poll_host_sockets();
        while backend.poll_receive().is_some() {}
    }
    let polled_stamp = {
        let flow = backend
            .outbound_ipv4
            .tcp_flows
            .values()
            .next()
            .expect("tcp flow present");
        flow.last_activity
    };
    assert_eq!(polled_stamp, idle_stamp);

    // Host-to-guest data is real activity and must advance the stamp.
    use std::io::Write as _;
    std::thread::sleep(std::time::Duration::from_millis(3));
    server.write_all(b"pong").unwrap();
    server.flush().unwrap();
    let mut saw_payload = false;
    for _ in 0..64 {
        backend.poll_host_sockets();
        while let Some(frame) = backend.poll_receive() {
            let (_, _, tcp) = parse_ipv4_tcp(&frame);
            if !tcp.payload.is_empty() {
                saw_payload = true;
            }
        }
        if saw_payload {
            break;
        }
    }
    assert!(saw_payload, "expected proxied host payload");
    let active_stamp = {
        let flow = backend
            .outbound_ipv4
            .tcp_flows
            .values()
            .next()
            .expect("tcp flow present");
        flow.last_activity
    };
    assert!(active_stamp > polled_stamp);
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
