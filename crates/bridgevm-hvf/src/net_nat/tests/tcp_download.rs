//! Host-to-guest TCP downloads through the NAT against the fake guest stack.

#![cfg(unix)]

use super::fake_guest::*;
use crate::virtio_net::NetBackend;
use std::time::{Duration, Instant};

const STALL: Duration = Duration::from_secs(2);

fn assert_complete(guest: &FakeGuest, sent: &[u8]) {
    assert_eq!(guest.data.len(), sent.len(), "the download stalled");
    assert!(guest.data == sent, "the guest received different bytes");
    assert_eq!(guest.overruns, 0, "the NAT sent past the guest's window");
}

#[test]
fn slow_reader_with_a_small_window_receives_the_whole_download() {
    // Live: PowerShell 5.1's Invoke-WebRequest reads slowly, the guest's
    // receive window fills, and the download stopped for good.
    let sent = payload(1 << 20);
    let Some((mut nat, mut guest, server)) = connect(&sent, 16 * 1024, None) else {
        return;
    };
    // MSS 1460, and no window scale for a guest that offered none.
    assert_eq!(guest.syn_ack_options, [2, 4, 0x05, 0xb4]);
    download(&mut nat, &mut guest, 2048, STALL);
    assert_complete(&guest, &sent);
    server.join().unwrap();
}

#[test]
fn window_scale_offered_in_the_guest_syn_is_applied() {
    // A window field holds at most 64 KiB; live, that halved A20 throughput.
    let sent = payload(2 << 20);
    let capacity = 512 * 1024;
    let Some((mut nat, mut guest, server)) = connect(&sent, capacity, Some(4)) else {
        return;
    };
    // MSS 1460, NOP, and the NAT's own window scale.
    assert_eq!(
        guest.syn_ack_options,
        [2, 4, 0x05, 0xb4, 1, 3, 3, crate::net_nat::OUR_WINDOW_SHIFT]
    );
    // The guest takes data but holds its ACKs back, so everything it gets
    // was sent on the strength of the handshake's window field alone: 32 KiB
    // unscaled, the whole buffer scaled.
    let deadline = Instant::now() + STALL;
    while guest.window() >= MSS {
        nat.poll_host_sockets();
        while let Some(frame) = nat.poll_receive() {
            let _held_back = guest.receive(&frame);
        }
        assert!(
            Instant::now() < deadline,
            "the NAT did not scale the window"
        );
        std::thread::sleep(Duration::from_micros(100));
    }
    assert_eq!(guest.overruns, 0, "the NAT sent past the scaled window");
    if let Some(update) = guest.app_read(usize::MAX) {
        nat.transmit(&update);
    }
    download(&mut nat, &mut guest, usize::MAX, STALL);
    assert_complete(&guest, &sent);
    server.join().unwrap();
}

#[test]
fn segment_lost_mid_flight_is_resent_after_three_duplicate_acks() {
    let sent = payload(256 * 1024);
    let Some((mut nat, mut guest, server)) = connect(&sent, 32 * 1024, None) else {
        return;
    };
    guest.lose = vec![3];
    download(&mut nat, &mut guest, usize::MAX, STALL);
    assert_complete(&guest, &sent);
    let stats = nat.stats();
    assert!(stats.tcp_fast_retransmits >= 1);
    assert_eq!(
        stats.tcp_timeout_retransmits, 0,
        "repaired only by a timeout"
    );
    server.join().unwrap();
}

#[test]
fn segment_lost_without_duplicate_acks_is_resent_after_a_timeout() {
    // Two segments fit the window, so a loss draws one duplicate ACK at most.
    let sent = payload(64 * 1024);
    let Some((mut nat, mut guest, server)) = connect(&sent, 2 * MSS, None) else {
        return;
    };
    guest.lose = vec![4];
    download(&mut nat, &mut guest, usize::MAX, STALL);
    assert_complete(&guest, &sent);
    assert!(nat.stats().tcp_timeout_retransmits >= 1);
    server.join().unwrap();
}

#[test]
fn shut_window_holds_data_back_and_is_probed_until_it_reopens() {
    let sent = payload(256 * 1024);
    let capacity = 8 * MSS;
    let Some((mut nat, mut guest, server)) = connect(&sent, capacity, None) else {
        return;
    };
    // The application stops reading: the window fills and shuts.
    let deadline = Instant::now() + STALL;
    while guest.window() != 0 {
        tick(&mut nat, &mut guest, 0);
        assert!(Instant::now() < deadline, "the window never filled");
    }
    for _ in 0..50 {
        tick(&mut nat, &mut guest, 0);
    }
    assert_eq!(guest.data.len(), capacity);
    assert_eq!(guest.overruns, 0, "the NAT sent into a shut window");

    // The application reads everything but the guest announces nothing:
    // only the NAT's probe can learn that the window reopened.
    guest.window_updates = false;
    guest.app_read(usize::MAX);
    let deadline = Instant::now() + STALL;
    while guest.data.len() == capacity {
        tick(&mut nat, &mut guest, 0);
        assert!(
            Instant::now() < deadline,
            "the shut window was never probed"
        );
    }
    assert!(nat.stats().tcp_zero_window_probes >= 1);

    guest.window_updates = true;
    download(&mut nat, &mut guest, 4096, STALL);
    assert_complete(&guest, &sent);
    server.join().unwrap();
}
