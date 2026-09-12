#![cfg(unix)]

#[path = "tcp_fixture.rs"]
mod fixture;
use fixture::reserved_nonlistening_socket;

use super::super::*;
use super::helpers::*;
use crate::virtio_net::NetBackend;
use std::net::{Ipv4Addr, TcpStream};
use std::time::{Duration, Instant};

#[test]
fn unconnected_socket_is_not_established() {
    let stream = reserved_nonlistening_socket();
    assert_eq!(tcp_connect_error(&stream).unwrap(), None);
}

#[test]
fn connected_idle_socket_is_established_without_application_data() {
    let listener = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let stream = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (_peer, _) = listener.accept().unwrap();
    stream.set_nonblocking(true).unwrap();
    assert_eq!(tcp_connect_error(&stream).unwrap(), Some(0));
}

#[test]
fn host_socket_tcp_connect_to_closed_port_returns_rst() {
    let reserved = reserved_nonlistening_socket();
    let port = reserved.local_addr().unwrap().port();
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
    let deadline = Instant::now() + Duration::from_secs(2);
    let frame = loop {
        backend.poll_host_sockets();
        if let Some(frame) = backend.poll_receive() {
            break frame;
        }
        assert!(
            Instant::now() < deadline,
            "refused connection did not return RST"
        );
        std::thread::sleep(Duration::from_millis(1));
    };
    let (_, _, tcp) = parse_ipv4_tcp(&frame);
    assert_ne!(tcp.flags & TCP_FLAG_RST, 0);
    assert_eq!(tcp.flags & TCP_FLAG_SYN, 0);
    assert_eq!(tcp.ack, 0x2000_0001);
    drop(reserved);
}
