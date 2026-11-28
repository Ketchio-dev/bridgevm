//! Inbound port forwards: a connection to a host port reaches a guest port.

#![cfg(unix)]

use super::super::port_forward::{
    forward_answered_line, forward_off_line, forward_unanswered_line, parse_forwards,
};
use super::super::*;
use super::helpers::*;
use crate::virtio_net::NetBackend;
use std::io::{ErrorKind, Read, Write};
use std::net::{Ipv4Addr as StdIpv4Addr, TcpListener, TcpStream};
use std::os::fd::AsRawFd;
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
    let mut first = client(port);
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
    match first.read(&mut buf) {
        Ok(0) => {}
        Err(error) => assert_eq!(error.kind(), ErrorKind::ConnectionReset),
        Ok(n) => panic!("read {n} bytes from a refused forward"),
    }
    assert!(nat.outbound_ipv4_handler().tcp_flows.is_empty());
    // Said for the app's notice (Windows isn't accepting connections there), until one opens.
    assert!(nat.outbound_ipv4_handler().forwards[0].unanswered);
    let _second = client(port);
    let (nat_port, nat_isn) = syn(&mut nat);
    nat.transmit(&tcp_guest_frame(
        GATEWAY_IP,
        nat_port,
        GUEST_PORT,
        GUEST_ISN,
        nat_isn.wrapping_add(1),
        TCP_FLAG_SYN | TCP_FLAG_ACK,
        &[],
    ));
    assert!(!nat.outbound_ipv4_handler().forwards[0].unanswered);
    assert_eq!(
        forward_unanswered_line(GUEST_PORT),
        "BVNET forward-unanswered guest=8080"
    );
    assert_eq!(
        forward_answered_line(GUEST_PORT),
        "BVNET forward-answered guest=8080"
    );
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
    assert!(
        nat.outbound_ipv4_handler().forwards[0].unanswered,
        "said for the app's notice"
    );
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
    assert_eq!(
        forward_off_line(&PortForward {
            host: 8080,
            guest: 80
        }),
        "BVNET forward-off host=8080 guest=80",
        "the line HvfPortForwardNoticeTests reads"
    );
}

/// A port no listener holds now.
fn free_port() -> u16 {
    let probe = TcpListener::bind((StdIpv4Addr::LOCALHOST, 0)).unwrap();
    probe.local_addr().unwrap().port()
}

fn listening(port: u16) -> bool {
    TcpStream::connect((StdIpv4Addr::LOCALHOST, port)).is_ok()
}

/// Forwards could only change with Windows shut down; the app now sets them while it runs.
#[test]
fn forwards_set_while_windows_runs_open_new_rules_close_removed_ones_and_keep_the_rest() {
    let (first, second) = (free_port(), free_port());
    let rule = |host, guest| PortForward { host, guest };
    let mut handler = HostSocketOutboundIpv4Handler::with_dns_resolver(StdIpv4Addr::LOCALHOST)
        .with_forwards(&[rule(first, 80)]);
    let kept = handler.forwards[0].listener.as_raw_fd();
    handler.set_forwards(&[rule(first, 80), rule(second, 81)]);
    assert_eq!(handler.forward_host_ports(), [first, second]);
    assert_eq!(
        handler.forwards[0].listener.as_raw_fd(),
        kept,
        "a rule kept keeps its listener"
    );
    assert!(listening(second));

    // A Mac port moved to another Windows port is closed, then opened again for the new one.
    handler.set_forwards(&[rule(second, 82)]);
    assert_eq!(handler.forward_host_ports(), [second]);
    assert_eq!(handler.forwards[0].guest_port, 82);
    assert!(!listening(first), "a rule gone closes its Mac port");

    // A rule that was off, its Mac port held by another app, is tried again.
    let held = TcpListener::bind((StdIpv4Addr::LOCALHOST, 0)).unwrap();
    let busy = held.local_addr().unwrap().port();
    handler.set_forwards(&[rule(second, 82), rule(busy, 83)]);
    assert_eq!(handler.forward_host_ports(), [second]);
    drop(held);
    handler.set_forwards(&[rule(second, 82), rule(busy, 83)]);
    assert_eq!(handler.forward_host_ports(), [second, busy]);
    handler.set_forwards(&[]);
    assert!(handler.forwards.is_empty());
}

#[test]
fn a_requested_change_is_taken_once_by_the_live_nat() {
    let port = free_port();
    let mut nat = NatBackend::with_outbound_handler(
        HostSocketOutboundIpv4Handler::with_dns_resolver(StdIpv4Addr::LOCALHOST),
    );
    request_forwards(vec![PortForward {
        host: port,
        guest: 80,
    }]);
    nat.poll_host_sockets();
    assert_eq!(nat.outbound_ipv4_handler().forward_host_ports(), [port]);
    nat.outbound_ipv4_handler_mut().set_forwards(&[]);
    nat.poll_host_sockets();
    assert!(
        nat.outbound_ipv4_handler().forwards.is_empty(),
        "taken once"
    );
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
