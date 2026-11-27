//! DNS over TCP to the guest's resolver address: Windows retries a lookup
//! over TCP when the UDP answer was truncated. The SYN was counted as
//! `other` and dropped, so the lookup hung until Windows gave up.

#![cfg(unix)]

use super::super::*;
use super::helpers::*;
use crate::virtio_net::NetBackend;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

fn dns_syn(guest_port: u16, isn: u32) -> Vec<u8> {
    tcp_guest_frame(DNS_IP, 53, guest_port, isn, 0, TCP_FLAG_SYN, &[])
}

#[test]
fn a_tcp_lookup_to_the_resolver_address_reaches_the_upstream_resolver() {
    let Some(upstream) = loopback_tcp_listener() else {
        return;
    };
    upstream.set_nonblocking(true).unwrap();
    let mut backend = NatBackend::<HostSocketOutboundIpv4Handler>::new_host_socket();
    backend.outbound_ipv4.dns.server = upstream.local_addr().unwrap();
    let isn = 0x4000_0000;
    backend.transmit(&dns_syn(49200, isn));
    assert_eq!(backend.outbound_ipv4.tcp_flows.len(), 1);

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut accepted = false;
    let syn_ack = loop {
        assert!(
            Instant::now() < deadline,
            "no SYN-ACK from the resolver address"
        );
        backend.poll_host_sockets();
        accepted |= upstream.accept().is_ok();
        if let Some(frame) = backend.poll_receive() {
            break frame;
        }
        std::thread::sleep(Duration::from_millis(1));
    };
    while !accepted && Instant::now() < deadline {
        accepted = upstream.accept().is_ok();
    }
    assert!(accepted, "the upstream resolver saw no connection");
    let (_, ip, tcp) = parse_ipv4_tcp(&syn_ack);
    assert_eq!((ip.src, ip.dst), (DNS_IP, GUEST_IP));
    assert_eq!((tcp.src_port, tcp.dst_port), (53, 49200));
    assert_eq!(tcp.flags, TCP_FLAG_SYN | TCP_FLAG_ACK);
    assert_eq!(tcp.ack, isn.wrapping_add(1));
}

#[test]
fn a_tcp_lookup_with_only_an_ipv6_resolver_is_reset_at_once() {
    let mut backend = NatBackend::<HostSocketOutboundIpv4Handler>::new_host_socket();
    backend.outbound_ipv4.dns.server = SocketAddr::from((std::net::Ipv6Addr::LOCALHOST, 53));
    backend.transmit(&dns_syn(49201, 0x5000_0000));
    assert!(backend.outbound_ipv4.tcp_flows.is_empty());
    backend.poll_host_sockets();
    let frame = backend.poll_receive().expect("no reset for the lookup");
    let (_, ip, tcp) = parse_ipv4_tcp(&frame);
    assert_eq!((ip.src, ip.dst), (DNS_IP, GUEST_IP));
    assert_eq!(tcp.flags, TCP_FLAG_RST | TCP_FLAG_ACK);
    assert_eq!(tcp.ack, 0x5000_0001);
}
