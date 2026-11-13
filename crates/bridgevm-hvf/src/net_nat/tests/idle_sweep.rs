use super::super::reply_queue::MAX_PENDING_REPLY_FRAMES;
use super::super::*;
use std::collections::VecDeque;
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
            socket_dst: SocketAddrV4::new(StdIpv4Addr::LOCALHOST, port).into(),
        },
        UdpFlow {
            socket,
            last_activity: 0,
            readable: true,
            watched: false,
        },
    )
}

#[test]
fn bounds_regression_udp_queue_resumes_after_drain() {
    let mut handler = HostSocketOutboundIpv4Handler::new();
    let (key, mut flow) = udp_flow(40_004);
    flow.watched = true;
    let peer = UdpSocket::bind("127.0.0.1:0").unwrap();
    peer.send_to(b"queued", flow.socket.local_addr().unwrap())
        .unwrap();
    // Loopback delivery is not synchronous with send_to. A watched flow that reads
    // too early is not read again until its watcher fires, and this test has none.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while flow.socket.peek(&mut [0; 1]).is_err() {
        assert!(
            std::time::Instant::now() < deadline,
            "datagram never arrived"
        );
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    handler.udp_flows.insert(key, flow);
    let mut queue = VecDeque::from(vec![Vec::new(); MAX_PENDING_REPLY_FRAMES]);
    let mut stats = NatStats::default();
    handler.poll_udp(Some([2, 0, 0, 0, 0, 1]), &mut queue, &mut stats);
    assert_eq!(queue.len(), MAX_PENDING_REPLY_FRAMES);
    assert!(handler.udp_flows[&key].readable);
    queue.clear();
    handler.poll_udp(Some([2, 0, 0, 0, 0, 1]), &mut queue, &mut stats);
    assert_eq!(queue.len(), 1);
    assert!(queue[0].ends_with(b"queued"));
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
