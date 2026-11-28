//! A UDP reply larger than the guest's MTU (a big DNS, IKE or SIP answer, which macOS hands over
//! reassembled) went to the guest as one oversized frame, cut to 2048 bytes. The guest's
//! receive buffers hold one MTU-sized frame, so it waited forever, and every frame behind it.

use super::super::*;
use super::helpers::GUEST_MAC;
use std::collections::VecDeque;
use std::net::{Ipv4Addr as StdIpv4Addr, SocketAddrV4, UdpSocket};
use std::time::{Duration, Instant};

#[test]
fn a_udp_reply_over_the_mtu_reaches_the_guest_whole_as_ipv4_fragments() {
    let socket = UdpSocket::bind(SocketAddrV4::new(StdIpv4Addr::LOCALHOST, 0)).unwrap();
    socket.set_nonblocking(true).unwrap();
    let peer = UdpSocket::bind("127.0.0.1:0").unwrap();
    let peer_port = peer.local_addr().unwrap().port();
    let key = UdpFlowKey {
        guest_ip: GUEST_IP,
        guest_port: 50_000,
        public_dst: [127, 0, 0, 1],
        public_dst_port: peer_port,
        socket_dst: peer.local_addr().unwrap(),
    };
    let data: Vec<u8> = (0..3_000u32).map(|i| (i * 7) as u8).collect();
    peer.send_to(&data, socket.local_addr().unwrap()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while socket.peek(&mut [0; 1]).is_err() {
        assert!(Instant::now() < deadline, "datagram never arrived");
        std::thread::sleep(Duration::from_millis(1));
    }
    let mut handler = HostSocketOutboundIpv4Handler::new();
    let flow = UdpFlow {
        socket,
        last_activity: 0,
        readable: true,
        watched: false,
    };
    handler.udp_flows.insert(key, flow);
    let mut queue = VecDeque::new();
    handler.poll_udp(Some(GUEST_MAC), &mut queue, &mut NatStats::default());

    assert_eq!(queue.len(), 3, "3008 UDP bytes in 1480-byte fragments");
    let mut datagram = Vec::new();
    let mut id = None;
    for (index, frame) in queue.iter().enumerate() {
        assert!(
            frame.len() <= 1514,
            "fragment {index} is {} bytes",
            frame.len()
        );
        let ip = &frame[14..34];
        assert_eq!(
            ipv4_header_checksum(ip),
            0,
            "header checksum of fragment {index}"
        );
        assert_eq!(
            usize::from(u16::from_be_bytes([ip[2], ip[3]])),
            frame.len() - 14
        );
        let fragment_id = u16::from_be_bytes([ip[4], ip[5]]);
        assert_ne!(fragment_id, 0);
        assert_eq!(
            *id.get_or_insert(fragment_id),
            fragment_id,
            "one datagram, one ID"
        );
        let flags = u16::from_be_bytes([ip[6], ip[7]]);
        assert_eq!(
            flags & 0x2000 != 0,
            index < 2,
            "more fragments on all but the last"
        );
        assert_eq!(
            usize::from(flags & 0x1fff) * 8,
            datagram.len(),
            "fragment {index} offset"
        );
        datagram.extend_from_slice(&frame[34..]);
    }
    let ip = &queue[0][14..34];
    assert_eq!(
        udp_checksum(
            [ip[12], ip[13], ip[14], ip[15]],
            [ip[16], ip[17], ip[18], ip[19]],
            &datagram
        ),
        0
    );
    assert_eq!(u16::from_be_bytes([datagram[2], datagram[3]]), 50_000);
    assert_eq!(
        usize::from(u16::from_be_bytes([datagram[4], datagram[5]])),
        8 + data.len()
    );
    assert_eq!(
        &datagram[8..],
        &data[..],
        "the whole datagram, not its first 2040 bytes"
    );
}
