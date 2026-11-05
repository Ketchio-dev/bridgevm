//! Why the NAT reset a guest TCP connection: the counters and the log line.

#![cfg(target_os = "macos")]

use super::tcp_readiness::fixture::reserved_nonlistening_socket;

use super::super::*;
use super::helpers::*;
use crate::virtio_net::NetBackend;

#[test]
fn host_read_error_resets_the_flow_and_logs_why() {
    take_tcp_reset_log();
    // Reading an unconnected socket fails with ENOTCONN, a host read error.
    let stream = reserved_nonlistening_socket();
    let host_port = stream.local_addr().unwrap().port();
    let mut backend = NatBackend::<HostSocketOutboundIpv4Handler>::new_host_socket();
    backend.guest_mac = Some(GUEST_MAC);
    let key = TcpFlowKey {
        guest_ip: GUEST_IP,
        guest_port: 49160,
        dst_ip: [127, 0, 0, 1],
        dst_port: 8080,
    };
    let now = backend.outbound_ipv4.now_ms();
    let mut flow = TcpFlow::new(stream, 0x4000_0001, 0x5000_0000, 65535, None, now);
    flow.state = TcpProxyState::Established;
    backend.outbound_ipv4.tcp_flows.insert(key, flow);
    backend.poll_host_sockets();
    let frame = backend.poll_receive().expect("no RST");
    let (_, _, tcp) = parse_ipv4_tcp(&frame);
    assert_eq!(tcp.flags, TCP_FLAG_RST | TCP_FLAG_ACK);
    assert_eq!((tcp.src_port, tcp.dst_port), (8080, 49160));
    assert!(backend.outbound_ipv4.tcp_flows.is_empty());
    let stats = backend.stats();
    assert_eq!((stats.rst_hostread, stats.rst_connect), (1, 0));
    let expected = format!(
        "virtio-net NAT reset 1/8: reason=hostread kind=NotConnected errno={} guest_port=49160 \
         dst_port=8080 host_port={host_port}",
        libc::ENOTCONN
    );
    assert_eq!(take_tcp_reset_log(), [expected]);
}

#[test]
fn segment_for_an_unknown_flow_is_counted_and_gets_no_reset() {
    take_tcp_reset_log();
    let mut backend = NatBackend::<HostSocketOutboundIpv4Handler>::new_host_socket();
    backend.transmit(&tcp_guest_frame(
        [127, 0, 0, 1],
        8080,
        49161,
        0x6000_0000,
        0x7000_0000,
        TCP_FLAG_PSH | TCP_FLAG_ACK,
        b"late",
    ));
    backend.poll_host_sockets();
    assert!(backend.poll_receive().is_none());
    let stats = backend.stats();
    assert_eq!(stats.tcp_noflow_drops, 1);
    assert_eq!((stats.rst_hostread, stats.rst_connect), (0, 0));
    assert!(take_tcp_reset_log().is_empty());
}

#[test]
fn ninth_reset_is_counted_but_not_logged() {
    take_tcp_reset_log();
    let mut backend = NatBackend::<HostSocketOutboundIpv4Handler>::new_host_socket();
    // Darwin rejects a connect to port 0 at once, so each SYN gets a RST.
    for guest_port in 49170..49179 {
        backend.transmit(&tcp_guest_frame(
            [127, 0, 0, 1],
            0,
            guest_port,
            0x3000_0000,
            0,
            TCP_FLAG_SYN,
            &[],
        ));
    }
    backend.poll_host_sockets();
    assert_eq!(std::iter::from_fn(|| backend.poll_receive()).count(), 9);
    assert_eq!(backend.stats().rst_connect, 9);
    let log = take_tcp_reset_log();
    assert_eq!(log.len(), 8);
    let first = format!(
        "virtio-net NAT reset 1/8: reason=connect kind=AddrNotAvailable errno={} guest_port=49170 \
         dst_port=0 host_port=-",
        libc::EADDRNOTAVAIL
    );
    assert_eq!(log[0], first);
    assert!(log[7].starts_with("virtio-net NAT reset 8/8: "));
}
