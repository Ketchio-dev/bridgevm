//! Inbound port forwards: a connection to a host port reaches a guest port.

#![cfg(unix)]

use super::super::port_forward::parse_forwards;
use super::super::*;
use super::helpers::*;
use crate::virtio_net::NetBackend;
use std::io::{ErrorKind, Read, Write};
use std::net::{Ipv4Addr as StdIpv4Addr, TcpListener, TcpStream};
use std::time::{Duration, Instant};

type Nat = NatBackend<HostSocketOutboundIpv4Handler>;

const GUEST_PORT: u16 = 8080;
const GUEST_ISN: u32 = 0x7000_0000;

/// A NAT forwarding a free host port to the guest's GUEST_PORT.
fn forwarding_nat() -> (Nat, u16) {
    let handler = HostSocketOutboundIpv4Handler::with_dns_resolver(StdIpv4Addr::LOCALHOST)
        .with_forwards(&[PortForward {
            host: 0,
            guest: GUEST_PORT,
        }]);
    let port = handler.forward_host_ports()[0];
    let mut nat = NatBackend::with_outbound_handler(handler);
    nat.guest_mac = Some(GUEST_MAC);
    (nat, port)
}

fn client(port: u16) -> TcpStream {
    let client = TcpStream::connect((StdIpv4Addr::LOCALHOST, port)).unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    client
}

/// Polls until the NAT has a frame for the guest.
fn next_frame(nat: &mut Nat) -> Vec<u8> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        nat.poll_host_sockets();
        if let Some(frame) = nat.poll_receive() {
            return frame;
        }
        assert!(Instant::now() < deadline, "no frame for the guest");
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// The NAT's SYN for a new host connection: (its source port, its ISN).
fn syn(nat: &mut Nat) -> (u16, u32) {
    let frame = next_frame(nat);
    let (_, ip, tcp) = parse_ipv4_tcp(&frame);
    assert_eq!((ip.src, ip.dst), (GATEWAY_IP, GUEST_IP));
    assert_eq!((tcp.dst_port, tcp.flags), (GUEST_PORT, TCP_FLAG_SYN));
    assert_eq!(syn_window_shift(tcp.segment), Some(OUR_WINDOW_SHIFT));
    (tcp.src_port, tcp.seq)
}

#[test]
fn a_host_connection_to_a_forwarded_port_reaches_the_guest_port() {
    let (mut nat, port) = forwarding_nat();
    let mut client = client(port);
    // Sent before the guest answers: it waits in the host socket.
    client.write_all(b"GET /").unwrap();
    let (nat_port, nat_isn) = syn(&mut nat);
    let guest = |seq, ack, flags, payload: &[u8]| {
        tcp_guest_frame(GATEWAY_IP, nat_port, GUEST_PORT, seq, ack, flags, payload)
    };
    nat.transmit(&guest(
        GUEST_ISN,
        nat_isn.wrapping_add(1),
        TCP_FLAG_SYN | TCP_FLAG_ACK,
        &[],
    ));
    // The NAT acknowledges the SYN-ACK, then forwards what the host sent.
    let mut request = Vec::new();
    while request.len() < 5 {
        let frame = next_frame(&mut nat);
        let (_, _, tcp) = parse_ipv4_tcp(&frame);
        assert_eq!(tcp.flags & (TCP_FLAG_SYN | TCP_FLAG_RST), 0);
        assert_eq!(tcp.ack, GUEST_ISN.wrapping_add(1));
        request.extend_from_slice(tcp.payload);
    }
    assert_eq!(request, b"GET /");
    // The guest answers and closes; the host reads the answer, then the end.
    let (ours, theirs) = (GUEST_ISN.wrapping_add(1), nat_isn.wrapping_add(6));
    nat.transmit(&guest(ours, theirs, TCP_FLAG_ACK | TCP_FLAG_PSH, b"200 OK"));
    nat.transmit(&guest(
        ours.wrapping_add(6),
        theirs,
        TCP_FLAG_ACK | TCP_FLAG_FIN,
        &[],
    ));
    nat.poll_host_sockets();
    let mut response = Vec::new();
    client.read_to_end(&mut response).unwrap();
    assert_eq!(response, b"200 OK");
}

#[test]
fn a_guest_port_that_refuses_closes_the_host_connection() {
    let (mut nat, port) = forwarding_nat();
    let mut client = client(port);
    let (nat_port, nat_isn) = syn(&mut nat);
    nat.transmit(&tcp_guest_frame(
        GATEWAY_IP,
        nat_port,
        GUEST_PORT,
        0,
        nat_isn.wrapping_add(1),
        TCP_FLAG_RST | TCP_FLAG_ACK,
        &[],
    ));
    nat.poll_host_sockets();
    let mut buf = [0u8; 16];
    match client.read(&mut buf) {
        Ok(0) => {}
        Err(error) => assert_eq!(error.kind(), ErrorKind::ConnectionReset),
        Ok(n) => panic!("read {n} bytes from a refused forward"),
    }
    assert!(nat.outbound_ipv4_handler().tcp_flows.is_empty());
}

#[test]
fn an_unanswered_syn_is_resent_then_the_host_connection_closes() {
    let (mut nat, port) = forwarding_nat();
    let mut client = client(port);
    syn(&mut nat);
    let started = Instant::now();
    let mut resent = 0;
    while !nat.outbound_ipv4_handler().tcp_flows.is_empty() {
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "never given up"
        );
        nat.poll_host_sockets();
        while let Some(frame) = nat.poll_receive() {
            let (_, _, tcp) = parse_ipv4_tcp(&frame);
            assert_eq!(tcp.flags, TCP_FLAG_SYN);
            resent += 1;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(resent >= 3, "{resent} resends");
    let mut buf = [0u8; 16];
    assert!(matches!(client.read(&mut buf), Ok(0) | Err(_)));
}

#[test]
fn a_host_port_another_app_holds_turns_only_that_forward_off() {
    let held = TcpListener::bind((StdIpv4Addr::LOCALHOST, 0)).unwrap();
    let busy = held.local_addr().unwrap().port();
    let handler = HostSocketOutboundIpv4Handler::with_dns_resolver(StdIpv4Addr::LOCALHOST)
        .with_forwards(&[
            PortForward {
                host: busy,
                guest: 80,
            },
            PortForward { host: 0, guest: 22 },
        ]);
    assert_eq!(handler.forwards.len(), 1);
    assert_eq!(handler.forwards[0].guest_port, 22);
}

#[test]
fn forwards_parse_from_host_guest_pairs() {
    assert_eq!(
        parse_forwards("8080:80, 3390:3389,bad,1:0,:5,"),
        [
            PortForward {
                host: 8080,
                guest: 80
            },
            PortForward {
                host: 3390,
                guest: 3389
            }
        ]
    );
}

#[test]
fn a_guest_syn_to_the_gateway_opens_no_host_connection() {
    // Forwarded connections come from the gateway, so its TCP reaches the
    // flow table; a new connection there must still go nowhere on the host.
    let (mut nat, _) = forwarding_nat();
    nat.transmit(&tcp_guest_frame(
        GATEWAY_IP,
        445,
        49_500,
        1,
        0,
        TCP_FLAG_SYN,
        &[],
    ));
    nat.poll_host_sockets();
    assert!(nat.outbound_ipv4_handler().tcp_flows.is_empty());
    assert_eq!(nat.stats().tcp_noflow_drops, 1);
}
