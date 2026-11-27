use super::super::reply_queue::MAX_PENDING_REPLY_FRAMES;
use super::super::*;
use super::helpers::{parse_ipv4_tcp, GUEST_MAC};
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

fn tcp_flow(guest_port: u16) -> (TcpFlowKey, TcpFlow, std::net::TcpListener) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let stream = std::net::TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let mut flow = TcpFlow::new(stream, 1, 1, u16::MAX, None, 0);
    flow.state = TcpProxyState::Established;
    let key = TcpFlowKey {
        guest_ip: GUEST_IP,
        guest_port,
        dst_ip: [127, 0, 0, 1],
        dst_port: listener.local_addr().unwrap().port(),
    };
    (key, flow, listener)
}

/// An SSH or database session Windows leaves alone was cut at the short idle timeout and Windows
/// was never told. An established connection with no FIN now outlives it; a closing one does not.
#[test]
fn an_idle_established_tcp_connection_outlives_the_short_idle_timeout() {
    let mut handler = HostSocketOutboundIpv4Handler::new().with_idle_timeout_ms(0);
    let (open_key, open, _open_peer) = tcp_flow(40_010);
    let (closing_key, mut closing, _closing_peer) = tcp_flow(40_011);
    closing.guest_fin = true;
    handler.tcp_flows.insert(open_key, open);
    handler.tcp_flows.insert(closing_key, closing);
    handler.epoch = std::time::Instant::now() - std::time::Duration::from_millis(1);

    handler.evict_idle_flows();

    assert!(handler.tcp_flows.contains_key(&open_key));
    assert!(!handler.tcp_flows.contains_key(&closing_key));
    let open = &handler.tcp_flows[&open_key];
    assert_eq!(open.idle_limit_ms(300_000), 124 * 60 * 1000);
}

/// A TCP flow the NAT dropped itself, idle or over the flow cap, was dropped silently: the guest's
/// next segment matched no flow and the Windows app hung until its own timeout. The guest now gets
/// a RST it accepts: our next sequence number, acknowledging everything it sent.
#[test]
fn a_tcp_flow_dropped_by_the_idle_sweep_or_the_flow_cap_is_reset_toward_the_guest() {
    let mut handler = HostSocketOutboundIpv4Handler::new().with_idle_timeout_ms(0);
    handler.max_flows = 1;
    let (closing_key, mut closing, _closing_peer) = tcp_flow(40_020);
    closing.guest_fin = true;
    closing.guest_next = 0x1111_0000;
    closing.send.next = 0x2222_0000;
    let (oldest_key, mut oldest, _oldest_peer) = tcp_flow(40_021);
    oldest.guest_next = 0x3333_0000;
    oldest.send.next = 0x4444_0000;
    let (newest_key, mut newest, _newest_peer) = tcp_flow(40_022);
    newest.last_activity = 1;
    for (key, flow) in [
        (closing_key, closing),
        (oldest_key, oldest),
        (newest_key, newest),
    ] {
        flow.stream.set_nonblocking(true).unwrap();
        handler.tcp_flows.insert(key, flow);
    }
    handler.epoch = std::time::Instant::now() - std::time::Duration::from_millis(2);

    handler.evict_idle_flows();
    let mut queue = VecDeque::new();
    let mut stats = NatStats::default();
    handler.poll_tcp(Some(GUEST_MAC), &mut queue, &mut stats);

    assert_eq!(handler.tcp_flows.keys().collect::<Vec<_>>(), [&newest_key]);
    let resets: Vec<_> = queue
        .iter()
        .map(|frame| {
            let (_, ip, tcp) = parse_ipv4_tcp(frame);
            assert_eq!(tcp.flags, TCP_FLAG_RST | TCP_FLAG_ACK);
            assert_eq!((ip.src, ip.dst), ([127, 0, 0, 1], GUEST_IP));
            (tcp.src_port, tcp.dst_port, tcp.seq, tcp.ack)
        })
        .collect();
    assert_eq!(
        resets,
        [
            (closing_key.dst_port, 40_020, 0x2222_0000, 0x1111_0000),
            (oldest_key.dst_port, 40_021, 0x4444_0000, 0x3333_0000),
        ]
    );
    assert_eq!(stats.rst_evicted, 2);
}
