#![cfg(target_os = "macos")]

use super::super::*;
use super::helpers::*;
use crate::virtio_net::NetBackend;
use std::net::Ipv4Addr as StdIpv4Addr;

#[test]
fn host_socket_tcp_connect_error_queues_rst_without_flow() {
    // Darwin's connect() rejects destination port 0 before sending a SYN, so
    // the error is synchronous even on a non-blocking socket.
    let err = nonblocking_tcp_connect(StdIpv4Addr::LOCALHOST, 0).unwrap_err();
    assert_eq!(err.raw_os_error(), Some(libc::EADDRNOTAVAIL));
    let mut backend = NatBackend::<HostSocketOutboundIpv4Handler>::new_host_socket();
    backend.transmit(&tcp_guest_frame(
        [127, 0, 0, 1],
        0,
        49156,
        0x3000_0000,
        0,
        TCP_FLAG_SYN,
        &[],
    ));
    assert!(backend.outbound_ipv4.tcp_flows.is_empty());
    assert_eq!(backend.outbound_ipv4.pending_tcp_resets.len(), 1);
    backend.poll_host_sockets();
    let frame = backend
        .poll_receive()
        .expect("connect error returned no RST");
    let (eth, ip, tcp) = parse_ipv4_tcp(&frame);
    assert_eq!(eth.dst, GUEST_MAC);
    assert_eq!((ip.src, ip.dst), ([127, 0, 0, 1], GUEST_IP));
    assert_eq!((tcp.src_port, tcp.dst_port), (0, 49156));
    assert_eq!(tcp.flags, TCP_FLAG_RST | TCP_FLAG_ACK);
    assert_eq!(tcp.ack, 0x3000_0001);
    assert!(backend.outbound_ipv4.pending_tcp_resets.is_empty());
    assert!(backend.poll_receive().is_none());
}
