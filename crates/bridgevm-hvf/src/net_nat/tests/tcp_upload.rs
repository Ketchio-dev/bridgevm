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
/// and the guest take its answers, until nothing moved for 200 ms.
fn upload_until_stalled(nat: &mut HostNat, guest: &mut FakeGuest, data: &[u8], sent: &mut usize) {
    let mut last_move = Instant::now();
    while last_move.elapsed() < Duration::from_millis(200) && *sent < data.len() {
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

#[test]
fn upload_window_is_scaled_and_follows_the_write_backlog() {
    let data = payload(24 << 20);
    let (go, wait) = mpsc::channel::<()>();
    let (done, received) = mpsc::channel();
    let handler = HostSocketOutboundIpv4Handler::with_dns_resolver(StdIpv4Addr::LOCALHOST);
    // The host application reads nothing until `go`, then everything.
    let Some((mut nat, mut guest, server)) =
        connect_with(handler, 64 * 1024, Some(7), move |mut stream| {
            wait.recv().unwrap();
            let mut sink = Vec::new();
            stream.read_to_end(&mut sink).unwrap();
            done.send(sink).unwrap();
        })
    else {
        return;
    };
    let mut sent = 0;
    upload_until_stalled(&mut nat, &mut guest, &data, &mut sent);
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
    let deadline = Instant::now() + Duration::from_secs(2);
    while guest.nat_window < MAX_TCP_WRITE_BACKLOG / 4 {
        assert!(
            Instant::now() < deadline,
            "no window update after the host drained"
        );
        tick(&mut nat, &mut guest, 0);
    }
    while sent < data.len() {
        upload_until_stalled(&mut nat, &mut guest, &data, &mut sent);
    }
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
