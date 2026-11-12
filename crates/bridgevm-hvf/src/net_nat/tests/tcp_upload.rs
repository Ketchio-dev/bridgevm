//! Guest-to-host uploads: the window the NAT announces is what its write
//! backlog still takes, so a guest that honours it is never dropped.

#![cfg(unix)]

use super::fake_guest::*;
use crate::net_nat::reply_queue::MAX_TCP_WRITE_BACKLOG;
use crate::net_nat::HostSocketOutboundIpv4Handler;
use crate::virtio_net::NetBackend;
use std::io::Read as _;
use std::net::Ipv4Addr as StdIpv4Addr;
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// Sends upload segments while the window has room, then lets the NAT poll
/// and the guest take its answers, until `done` or nothing moved for `quiet`.
fn upload_until(
    nat: &mut HostNat,
    guest: &mut FakeGuest,
    data: &[u8],
    sent: &mut usize,
    quiet: Duration,
    done: impl Fn(&FakeGuest, usize) -> bool,
) {
    let mut last_move = Instant::now();
    while last_move.elapsed() < quiet && !done(guest, *sent) {
        let before = (*sent, guest.acked, guest.nat_window);
        while *sent < data.len() {
            let chunk = &data[*sent..data.len().min(*sent + MSS)];
            let Some(frame) = guest.upload(chunk) else {
                break;
            };
            nat.transmit(&frame);
            *sent += chunk.len();
        }
        tick(nat, guest, 0);
        if (*sent, guest.acked, guest.nat_window) != before {
            last_move = Instant::now();
        }
    }
}

/// The window is shut and everything sent is acknowledged.
fn shut(guest: &FakeGuest, _: usize) -> bool {
    guest.nat_window < MSS && guest.in_flight() == 0
}

#[test]
fn upload_window_is_scaled_and_follows_the_write_backlog() {
    upload_through_a_backlog_that_fills_and_drains(Some(7));
}

#[test]
fn an_unscaled_upload_window_reopens_after_it_closed() {
    // At most 64 KiB fits the field: the update must not wait for 1 MiB.
    upload_through_a_backlog_that_fills_and_drains(None);
}

#[test]
fn a_segment_the_nat_cannot_take_is_answered_with_an_ack() {
    let handler = HostSocketOutboundIpv4Handler::with_dns_resolver(StdIpv4Addr::LOCALHOST);
    let Some((mut nat, mut guest, server)) =
        connect_with(handler, 64 * 1024, Some(7), |mut stream| {
            let _ = stream.read_to_end(&mut Vec::new());
        })
    else {
        return;
    };
    let start = guest.acked;
    let data = payload(2 * MSS);
    let lost = guest.upload(&data[..MSS]).unwrap();
    // The next segment arrives first: the NAT drops it and says where it is.
    nat.transmit(&guest.upload(&data[MSS..]).unwrap());
    nat.poll_host_sockets();
    let ack = nat
        .poll_receive()
        .expect("no ACK for the out-of-order segment");
    guest.receive(&ack);
    assert_eq!(guest.acked, start);
    assert!(guest.nat_window >= MAX_TCP_WRITE_BACKLOG / 2);
    nat.transmit(&lost);
    nat.poll_host_sockets();
    let ack = nat.poll_receive().expect("no ACK for the missing segment");
    guest.receive(&ack);
    assert_eq!(guest.acked, start.wrapping_add(MSS as u32));
    nat.transmit(&guest.upload_fin());
    drop(nat);
    server.join().unwrap();
}

#[test]
fn guest_fin_drains_the_acknowledged_upload_tail_before_host_eof() {
    upload_fin_with_backlog(false);
}

#[test]
fn guest_payload_and_fin_are_accepted_together() {
    upload_fin_with_backlog(true);
}

fn upload_fin_with_backlog(with_payload: bool) {
    use super::super::*;
    use super::helpers::*;
    use std::net::{TcpListener, TcpStream};

    let listener = TcpListener::bind((StdIpv4Addr::LOCALHOST, 0)).unwrap();
    let stream = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (mut peer, _) = listener.accept().unwrap();
    stream.set_nonblocking(true).unwrap();
    peer.set_nonblocking(true).unwrap();
    let key = TcpFlowKey {
        guest_ip: GUEST_IP,
        guest_port: 49162,
        dst_ip: [127, 0, 0, 1],
        dst_port: listener.local_addr().unwrap().port(),
    };
    let mut nat = NatBackend::<HostSocketOutboundIpv4Handler>::new_host_socket();
    nat.guest_mac = Some(GUEST_MAC);
    let seq = 0x6000_0000;
    let mut flow = TcpFlow::new(stream, seq, 100, 65535, None, 0);
    flow.state = TcpProxyState::Established;
    // Deterministically model bytes already ACKed while the host was blocked.
    // Keep them queued until the FIN arrives, rather than depending on kernel
    // buffer sizes or scheduling to hit WouldBlock at just the right moment.
    let data = payload(64 * 1024);
    let tail = if with_payload {
        &b"last segment"[..]
    } else {
        &[]
    };
    flow.write_buf.extend(&data);
    nat.outbound_ipv4.tcp_flows.insert(key, flow);
    nat.transmit(&tcp_guest_frame(
        key.dst_ip,
        key.dst_port,
        key.guest_port,
        seq,
        101,
        TCP_FLAG_ACK | TCP_FLAG_FIN,
        tail,
    ));
    let flow = &nat.outbound_ipv4.tcp_flows[&key];
    assert!(flow.guest_fin, "payload plus FIN was not accepted");
    assert_eq!(flow.guest_next, seq + tail.len() as u32 + 1);
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut received = Vec::new();
    let mut scratch = [0; 8192];
    loop {
        nat.poll_host_sockets();
        match peer.read(&mut scratch) {
            Ok(0) => break,
            Ok(n) => received.extend_from_slice(&scratch[..n]),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::yield_now();
            }
            Err(e) => panic!("host read: {e}"),
        }
        assert!(Instant::now() < deadline, "host never saw EOF");
    }
    assert_eq!(received.len(), data.len() + tail.len(), "upload tail lost");
    assert_eq!(&received[..data.len()], data);
    assert_eq!(&received[data.len()..], tail);
}

fn upload_through_a_backlog_that_fills_and_drains(shift: Option<u8>) {
    let data = payload(24 << 20);
    let (go, wait) = mpsc::channel::<()>();
    let (done, received) = mpsc::channel();
    let handler = HostSocketOutboundIpv4Handler::with_dns_resolver(StdIpv4Addr::LOCALHOST);
    // The host application reads nothing until `go`, then everything.
    let Some((mut nat, mut guest, server)) =
        connect_with(handler, 64 * 1024, shift, move |mut stream| {
            wait.recv().unwrap();
            let mut sink = Vec::new();
            stream.read_to_end(&mut sink).unwrap();
            done.send(sink).unwrap();
        })
    else {
        return;
    };
    let mut sent = 0;
    // Until the window shuts; 10 s without progress ends it on main, where
    // it never does (a loaded runner may pause the test for a while).
    let quiet = Duration::from_secs(10);
    upload_until(&mut nat, &mut guest, &data, &mut sent, quiet, shut);
    // A 64 KiB window held live uploads to about 60 MB/s. The backlog alone
    // takes 4 MiB, and the guest got all of it acknowledged: nothing it sent
    // within the window was dropped, and the window then closed.
    assert!(
        sent > MAX_TCP_WRITE_BACKLOG,
        "only {sent} bytes before the window closed"
    );
    assert_eq!(
        guest.in_flight(),
        0,
        "segments inside the window were dropped"
    );
    assert!(
        guest.nat_window < MSS,
        "window {} still open",
        guest.nat_window
    );
    assert!(sent < data.len());

    // The host drains: the NAT reopens the window on its own, without a
    // probe from the guest, and the upload completes intact.
    go.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while guest.nat_window < 16 * 1024 {
        assert!(
            Instant::now() < deadline,
            "no window update after the host drained"
        );
        tick(&mut nat, &mut guest, 0);
    }
    upload_until(&mut nat, &mut guest, &data, &mut sent, quiet, |_, sent| {
        sent == data.len()
    });
    assert_eq!(
        sent,
        data.len(),
        "the upload stalled after the window reopened"
    );
    nat.transmit(&guest.upload_fin());
    let deadline = Instant::now() + Duration::from_secs(5);
    let sink = loop {
        tick(&mut nat, &mut guest, 0);
        if let Ok(sink) = received.try_recv() {
            break sink;
        }
        assert!(Instant::now() < deadline, "the host never saw the FIN");
    };
    assert!(
        sink == data,
        "the host received {} of {} bytes, or other bytes",
        sink.len(),
        data.len()
    );
    server.join().unwrap();
}
