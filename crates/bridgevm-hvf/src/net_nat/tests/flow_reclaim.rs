//! Flows that hold the Mac's resources after they are done with them.

use super::super::*;
use super::helpers::GUEST_MAC;
use std::collections::VecDeque;
use std::io::Read;
use std::net::{Ipv4Addr as StdIpv4Addr, SocketAddrV4, TcpListener, TcpStream, UdpSocket};
use std::time::{Duration, Instant};

fn flow_socket_with_a_datagram(reply: &[u8]) -> UdpSocket {
    let socket = UdpSocket::bind(SocketAddrV4::new(StdIpv4Addr::LOCALHOST, 0)).unwrap();
    socket.set_nonblocking(true).unwrap();
    let peer = UdpSocket::bind("127.0.0.1:0").unwrap();
    peer.send_to(reply, socket.local_addr().unwrap()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while socket.peek(&mut [0; 1]).is_err() {
        assert!(Instant::now() < deadline, "datagram never arrived");
        std::thread::sleep(Duration::from_millis(1));
    }
    socket
}

/// Windows asks each DNS query from a new port. Each answered query kept its flow (and socket) for
/// the 300 s idle time, so a browser's lookups filled the 256-flow table and the least recently
/// used flow was dropped: often a VPN, call or game flow that was still in use.
#[test]
fn an_answered_dns_query_gives_its_flow_back_and_other_udp_flows_stay() {
    let mut handler = HostSocketOutboundIpv4Handler::new();
    let mut flow = |guest_port, public_dst, public_dst_port| {
        let key = UdpFlowKey {
            guest_ip: GUEST_IP,
            guest_port,
            public_dst,
            public_dst_port,
            socket_dst: SocketAddrV4::new(StdIpv4Addr::LOCALHOST, public_dst_port).into(),
        };
        let socket = flow_socket_with_a_datagram(b"answer");
        handler.udp_flows.insert(
            key,
            UdpFlow {
                socket,
                last_activity: 0,
                readable: true,
                watched: false,
            },
        );
        key
    };
    let dns = flow(53_000, DNS_IP, 53);
    let call = flow(50_000, [127, 0, 0, 1], 3478);
    let mut queue = VecDeque::new();
    let mut stats = NatStats::default();
    handler.poll_udp(Some(GUEST_MAC), &mut queue, &mut stats);

    assert_eq!(queue.len(), 2, "both answers reach the guest");
    assert_eq!(stats.dns_replies, 1);
    assert!(
        !handler.udp_flows.contains_key(&dns),
        "the answered DNS flow is closed"
    );
    assert!(handler.udp_flows.contains_key(&call), "a call's flow stays");
}

/// A drained upload buffer kept its allocation, which a 4 MiB backlog grows to about 6 MB, for
/// as long as the connection stayed open.
#[test]
fn a_flushed_upload_buffer_gives_its_memory_back() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let stream = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (mut peer, _) = listener.accept().unwrap();
    stream.set_nonblocking(true).unwrap();
    let mut flow = TcpFlow::new(stream, 1, 1, u16::MAX, None, 0);
    flow.write_buf.extend(std::iter::repeat_n(7u8, 256 * 1024));
    assert!(flow.write_buf.capacity() >= 256 * 1024);
    let reader = std::thread::spawn(move || {
        let mut sink = vec![0; 256 * 1024];
        peer.read_exact(&mut sink).unwrap();
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    while !flow.write_buf.is_empty() {
        assert!(Instant::now() < deadline, "the upload never drained");
        flow.flush_host_write();
        std::thread::sleep(Duration::from_millis(1));
    }
    reader.join().unwrap();
    assert_eq!(
        flow.write_buf.capacity(),
        0,
        "the drained buffer keeps its allocation"
    );
}
