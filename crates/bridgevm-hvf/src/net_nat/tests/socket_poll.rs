use super::super::*;
use super::helpers::*;
use crate::virtio_net::NetBackend;
use std::net::{Ipv4Addr as StdIpv4Addr, TcpStream};

#[test]
fn readiness_filter_skips_empty_udp_receive_syscalls() {
    let Some(echo) = loopback_udp_socket() else {
        return;
    };
    let port = echo.local_addr().unwrap().port();
    let handler = HostSocketOutboundIpv4Handler::with_dns_resolver(StdIpv4Addr::LOCALHOST);
    let mut backend = NatBackend::with_outbound_handler(handler);

    backend.transmit(&udp_guest_frame([127, 0, 0, 1], port, 49152, b"hello"));
    for _ in 0..64 {
        backend.poll_host_sockets();
    }

    assert_eq!(backend.stats().udp_recv_again, 0);
    assert_eq!(backend.stats().udp_flow_count, 1);
}

#[test]
fn legacy_kill_switch_restores_empty_receive_attempts() {
    let Some(echo) = loopback_udp_socket() else {
        return;
    };
    let port = echo.local_addr().unwrap().port();
    let mut handler = HostSocketOutboundIpv4Handler::with_dns_resolver(StdIpv4Addr::LOCALHOST);
    handler.socket_readiness.enabled = false;
    let mut backend = NatBackend::with_outbound_handler(handler);

    backend.transmit(&udp_guest_frame([127, 0, 0, 1], port, 49152, b"hello"));
    backend.poll_host_sockets();

    assert_eq!(backend.stats().udp_recv_again, 1);
}

#[test]
fn pending_reset_runs_without_a_ready_socket() {
    let handler = HostSocketOutboundIpv4Handler::with_dns_resolver(StdIpv4Addr::LOCALHOST);
    let mut backend = NatBackend::with_outbound_handler(handler);
    backend.guest_mac = Some(GUEST_MAC);
    backend
        .outbound_ipv4
        .pending_tcp_resets
        .push_back(PendingTcpReset {
            key: TcpFlowKey {
                guest_ip: GUEST_IP,
                guest_port: 49153,
                dst_ip: [127, 0, 0, 1],
                dst_port: 9,
            },
            seq: 7,
            ack: 11,
        });

    backend.poll_host_sockets();

    let frame = backend.poll_receive().unwrap();
    let (_, _, tcp) = parse_ipv4_tcp(&frame);
    assert_eq!(tcp.flags, TCP_FLAG_RST | TCP_FLAG_ACK);
    assert_eq!((tcp.seq, tcp.ack), (7, 11));
}

#[test]
fn pending_ack_and_closed_flow_progress_without_read_events() {
    let Some(listener) = loopback_tcp_listener() else {
        return;
    };
    let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (_server, _) = listener.accept().unwrap();
    client.set_nonblocking(true).unwrap();
    let key = TcpFlowKey {
        guest_ip: GUEST_IP,
        guest_port: 49154,
        dst_ip: [127, 0, 0, 1],
        dst_port: listener.local_addr().unwrap().port(),
    };
    let handler = HostSocketOutboundIpv4Handler::with_dns_resolver(StdIpv4Addr::LOCALHOST);
    let mut backend = NatBackend::with_outbound_handler(handler);
    backend.guest_mac = Some(GUEST_MAC);
    let mut flow = TcpFlow::new(client, 101, 201, 0);
    flow.state = TcpProxyState::Established;
    flow.pending_ack = true;
    backend.outbound_ipv4.tcp_flows.insert(key, flow);

    backend.poll_host_sockets();
    let frame = backend.poll_receive().unwrap();
    assert_eq!(parse_ipv4_tcp(&frame).2.flags, TCP_FLAG_ACK);

    let flow = backend.outbound_ipv4.tcp_flows.get_mut(&key).unwrap();
    flow.guest_fin = true;
    flow.host_fin_sent = true;
    flow.host_fin_acked = true;
    backend.poll_host_sockets();
    assert!(!backend.outbound_ipv4.tcp_flows.contains_key(&key));
}
