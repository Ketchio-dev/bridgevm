//! With macOS Local Network access off, sends to the Mac's DNS server on its
//! own network fail with EHOSTUNREACH and Windows resolves nothing. The VMM
//! says so once, and again when a reply comes back.

#![cfg(unix)]

use super::super::*;
use super::helpers::*;
use crate::virtio_net::NetBackend;
use std::io;
use std::net::{Ipv4Addr as StdIpv4Addr, SocketAddr};
use std::time::{Duration, Instant};

fn refused() -> io::Error {
    io::Error::from_raw_os_error(libc::EHOSTUNREACH)
}

#[test]
fn a_refused_send_to_a_dns_server_on_the_mac_network_is_reported_once() {
    let mut handler = HostSocketOutboundIpv4Handler::default();
    let router = SocketAddr::from(([192, 168, 1, 1], 53));
    handler.note_dns_send_error(&io::Error::from_raw_os_error(libc::ECONNREFUSED), router);
    assert!(
        !handler.dns_unreachable,
        "only EHOSTUNREACH is the Local Network refusal"
    );
    handler.note_dns_send_error(&refused(), SocketAddr::from(([1, 1, 1, 1], 53)));
    assert!(
        !handler.dns_unreachable,
        "a public server needs no Local Network access"
    );
    handler.note_dns_send_error(&refused(), router);
    assert!(handler.dns_unreachable);
    assert_eq!(DNS_UNREACHABLE_LINE, "BVNET dns-unreachable");
    assert!(is_local_network_address("fe80::1".parse().unwrap()));
    assert!(is_local_network_address("fd00::53".parse().unwrap()));
    assert!(!is_local_network_address(
        "2606:4700::1111".parse().unwrap()
    ));
}

#[test]
fn a_dns_reply_clears_the_unreachable_state() {
    let Some(upstream) = loopback_udp_socket() else {
        return;
    };
    upstream.set_nonblocking(true).unwrap();
    let mut backend = NatBackend::with_outbound_handler(
        HostSocketOutboundIpv4Handler::with_dns_resolver(StdIpv4Addr::LOCALHOST),
    );
    backend.outbound_ipv4.dns.server = upstream.local_addr().unwrap();
    backend.outbound_ipv4.dns_unreachable = true;
    backend.transmit(&udp_guest_frame(DNS_IP, 53, 53001, b"query"));

    let deadline = Instant::now() + Duration::from_secs(2);
    let mut buf = [0u8; 64];
    let peer = loop {
        assert!(
            Instant::now() < deadline,
            "the query never reached the upstream"
        );
        match upstream.recv_from(&mut buf) {
            Ok((_, peer)) => break peer,
            Err(e) if would_block(&e) => backend.poll_host_sockets(),
            Err(e) => panic!("upstream recv failed: {e}"),
        }
    };
    upstream.send_to(b"answer", peer).unwrap();
    while backend.pending_receive_len() == 0 && Instant::now() < deadline {
        backend.poll_host_sockets();
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(
        backend.poll_receive().is_some(),
        "the answer never reached Windows"
    );
    assert!(!backend.outbound_ipv4.dns_unreachable);
}
