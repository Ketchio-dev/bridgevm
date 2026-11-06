use super::super::reply_queue::{
    MAX_PENDING_REPLY_FRAMES, MAX_TCP_WRITE_BACKLOG, REPLY_BACKLOG_SKIPS_SWEEP,
};
use super::super::*;
use super::helpers::*;
use crate::virtio_net::NetBackend;
use std::collections::VecDeque;
use std::net::{Ipv4Addr as StdIpv4Addr, TcpListener, TcpStream};

#[derive(Default)]
struct CountingSweeps(usize);

impl OutboundIpv4Handler for CountingSweeps {
    fn handle_outbound_ipv4(&mut self, _packet: &Ipv4Packet<'_>) {}
    fn poll_host_sockets(
        &mut self,
        _guest_mac: Option<MacAddr>,
        _reply_queue: &mut VecDeque<Vec<u8>>,
        _stats: &mut NatStats,
    ) {
        self.0 += 1;
    }
}

#[test]
fn acknowledged_burst_gives_its_buffer_back() {
    // The unacked budget counts allocations and a drained VecDeque keeps its
    // own, so buffers left on idle flows (even small ones, times 256 flows)
    // would exhaust the budget and stall every download.
    for burst in [40 * 1024, 1 << 20] {
        let mut send = TcpSendState::new(1000, u16::MAX, Some(8));
        send.unacked.extend(std::iter::repeat_n(7u8, burst));
        send.next = send.una.wrapping_add(burst as u32);
        send.on_guest_ack(send.next, 1 << 20, false);
        assert!(send.unacked.is_empty());
        assert_eq!(
            send.unacked.capacity(),
            0,
            "a {burst}-byte burst kept its buffer"
        );
    }
}

#[test]
fn send_buffers_grow_within_the_unacked_budget() {
    // Plain VecDeque doubling allocated 186,880 bytes for this 100,000-byte
    // budget; send_to_guest reads at most spare capacity plus budget.
    let mut unacked = VecDeque::new();
    let mut budget = 100_000;
    loop {
        let len = TCP_MSS.min(unacked.capacity() - unacked.len() + budget);
        if len == 0 {
            break;
        }
        append_within_budget(&mut unacked, &[7; TCP_MSS][..len], &mut budget);
    }
    assert!(
        unacked.capacity() <= 100_000,
        "allocated {}",
        unacked.capacity()
    );
    assert_eq!(unacked.len(), unacked.capacity());
}

#[test]
fn host_sockets_wait_while_a_ring_of_frames_waits_for_the_guest() {
    let mut backend = NatBackend::with_outbound_handler(CountingSweeps::default());
    let arp = arp_request(GUEST_MAC, GUEST_IP, GATEWAY_IP);
    for _ in 0..REPLY_BACKLOG_SKIPS_SWEEP - 1 {
        backend.transmit(&arp);
    }
    backend.poll_host_sockets();
    assert_eq!(backend.outbound_ipv4_handler().0, 1);

    backend.transmit(&arp);
    backend.poll_host_sockets();
    assert_eq!(
        backend.outbound_ipv4_handler().0,
        1,
        "swept with a full ring queued"
    );
    assert_eq!(backend.stats().sweeps_deferred, 1);

    assert!(backend.poll_receive().is_some());
    backend.poll_host_sockets();
    assert_eq!(backend.outbound_ipv4_handler().0, 2);
}

#[test]
fn guest_requests_without_rx_draining_keep_a_bounded_reply_queue() {
    let mut backend = NatBackend::new();
    let arp = arp_request(GUEST_MAC, GUEST_IP, GATEWAY_IP);
    let ping = icmp_echo_frame(GATEWAY_IP, 7);
    for _ in 0..MAX_PENDING_REPLY_FRAMES {
        backend.transmit(&arp);
        backend.transmit(&ping);
    }

    // Every request was handled; replies past the bound were dropped.
    assert_eq!(
        backend.stats().arp_requests,
        MAX_PENDING_REPLY_FRAMES as u64
    );
    assert_eq!(backend.pending_receive_len(), MAX_PENDING_REPLY_FRAMES);

    // Draining one frame makes room for the next reply.
    assert!(backend.poll_receive().is_some());
    backend.transmit(&arp);
    assert_eq!(backend.pending_receive_len(), MAX_PENDING_REPLY_FRAMES);
}

#[test]
fn tcp_payload_past_the_write_backlog_is_left_unacknowledged() {
    let Ok(listener) = TcpListener::bind((StdIpv4Addr::LOCALHOST, 0)) else {
        return;
    };
    let port = listener.local_addr().unwrap().port();
    let stream = TcpStream::connect((StdIpv4Addr::LOCALHOST, port)).unwrap();
    let (_peer, _) = listener.accept().unwrap();
    stream.set_nonblocking(true).unwrap();

    let mut backend = NatBackend::with_outbound_handler(
        HostSocketOutboundIpv4Handler::with_dns_resolver(StdIpv4Addr::LOCALHOST),
    );
    let key = TcpFlowKey {
        guest_ip: GUEST_IP,
        guest_port: 49200,
        dst_ip: [127, 0, 0, 1],
        dst_port: port,
    };
    let guest_next = 0x3000_0000u32;
    let mut flow = TcpFlow::new(stream, guest_next, 0x10, u16::MAX, None, 0);
    flow.state = TcpProxyState::Established;
    flow.write_buf
        .extend(std::iter::repeat_n(0u8, MAX_TCP_WRITE_BACKLOG));
    backend
        .outbound_ipv4_handler_mut()
        .tcp_flows
        .insert(key, flow);

    backend.transmit(&tcp_guest_frame(
        [127, 0, 0, 1],
        port,
        49200,
        guest_next,
        0x11,
        TCP_FLAG_ACK | TCP_FLAG_PSH,
        b"more",
    ));

    // Not taken: the ACK it draws still points at the dropped segment, and
    // its window (the full backlog leaves none) tells the guest to wait.
    let flow = backend
        .outbound_ipv4_handler_mut()
        .tcp_flows
        .get_mut(&key)
        .unwrap();
    assert_eq!(flow.guest_next, guest_next);
    assert!(flow.pending_ack);
    assert_eq!(flow.window_field(), 0);
    assert!(flow.write_buf.len() <= MAX_TCP_WRITE_BACKLOG);
}
