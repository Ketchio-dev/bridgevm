//! A flow whose socket the wake thread watches is read only after its events.

#![cfg(target_os = "macos")]

use super::super::*;
use super::fake_guest::*;
use std::io::Write as _;
use std::net::{Ipv4Addr as StdIpv4Addr, Shutdown};
use std::sync::mpsc;
use std::time::Duration;

#[test]
fn a_watched_flow_reads_its_socket_only_after_an_event() {
    let (first, rest) = (payload(64 * 1024), payload(256 * 1024));
    let (go_tx, go_rx) = mpsc::channel::<()>();
    let handler = HostSocketOutboundIpv4Handler::with_dns_resolver(StdIpv4Addr::LOCALHOST);
    let _ = handler.wake.set(HostWake::start_unhooked());
    let (head, tail) = (first.clone(), rest.clone());
    let Some((mut nat, mut guest, server)) =
        connect_with(handler, 512 * 1024, Some(4), move |mut stream| {
            stream.write_all(&head).unwrap();
            go_rx.recv().unwrap();
            stream.write_all(&tail).unwrap();
            stream.shutdown(Shutdown::Write).unwrap();
        })
    else {
        return;
    };
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while guest.data.len() < first.len() {
        tick(&mut nat, &mut guest, usize::MAX);
        assert!(
            std::time::Instant::now() < deadline,
            "the first part never arrived"
        );
    }
    // Nothing more to read: the socket is not read again (an unwatched flow
    // would be read, and would block, on every one of these polls).
    let before = nat.stats().tcp_read_again;
    for _ in 0..50 {
        tick(&mut nat, &mut guest, usize::MAX);
    }
    let idle_reads = nat.stats().tcp_read_again - before;
    assert!(
        idle_reads <= 1,
        "{idle_reads} reads in 50 polls with nothing to read"
    );
    // New data is an event again, and the rest arrives.
    go_tx.send(()).unwrap();
    download(&mut nat, &mut guest, usize::MAX, Duration::from_secs(2));
    assert_eq!(
        guest.data.len(),
        first.len() + rest.len(),
        "the download stalled"
    );
    assert!(
        guest.data[first.len()..] == rest[..],
        "the guest received different bytes"
    );
    server.join().unwrap();
}
