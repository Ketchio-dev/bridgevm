use super::super::*;
use std::net::{Ipv4Addr as StdIpv4Addr, SocketAddrV4, UdpSocket};

fn udp_flow(port: u16) -> (UdpFlowKey, UdpFlow) {
    let socket = UdpSocket::bind(SocketAddrV4::new(StdIpv4Addr::LOCALHOST, 0)).unwrap();
    socket.set_nonblocking(true).unwrap();
    (
        UdpFlowKey {
            guest_ip: GUEST_IP,
            guest_port: port,
            public_dst: [127, 0, 0, 1],
            public_dst_port: port,
            socket_dst: [127, 0, 0, 1],
            socket_dst_port: port,
        },
        UdpFlow {
            socket,
            last_activity: 0,
        },
    )
}

#[test]
fn repeated_poll_calls_do_not_rescan_idle_flow_tables() {
    let mut handler = HostSocketOutboundIpv4Handler::new();
    let (key, flow) = udp_flow(40_000);
    handler.udp_flows.insert(key, flow);

    for _ in 0..10_000 {
        handler.evict_idle_flows();
    }

    assert_eq!(handler.idle_sweep_count, 0);
    assert_eq!(handler.udp_flows.len(), 1);
}

#[test]
fn capacity_limit_is_enforced_between_timed_sweeps() {
    let mut handler = HostSocketOutboundIpv4Handler::new();
    handler.max_flows = 1;
    for port in [40_001, 40_002] {
        let (key, flow) = udp_flow(port);
        handler.udp_flows.insert(key, flow);
    }

    handler.evict_idle_flows();

    assert_eq!(handler.idle_sweep_count, 0);
    assert_eq!(handler.udp_flows.len(), 1);
}

#[test]
fn zero_idle_timeout_still_sweeps_immediately() {
    let mut handler = HostSocketOutboundIpv4Handler::new().with_idle_timeout_ms(0);
    let (key, flow) = udp_flow(40_003);
    handler.udp_flows.insert(key, flow);
    handler.epoch = std::time::Instant::now() - std::time::Duration::from_millis(1);

    handler.evict_idle_flows();

    assert_eq!(handler.idle_sweep_count, 1);
    assert!(handler.udp_flows.is_empty());
}
