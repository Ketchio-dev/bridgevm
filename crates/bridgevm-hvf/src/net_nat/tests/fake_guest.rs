//! A fake guest TCP receiver for host-to-guest downloads through the NAT.
//!
//! It behaves like a real stack whose application reads slowly: it advertises
//! what is left of a small receive buffer, accepts only the next in-order
//! segment that fits, drops everything else, and answers every segment it
//! drops with a cumulative ACK. It keeps no out-of-order queue, the strictest
//! receiver a sender has to cope with.

#![cfg(unix)]

use super::super::*;
use super::helpers::*;
use crate::virtio_net::NetBackend;
use std::io::Write as _;
use std::net::{Ipv4Addr as StdIpv4Addr, Shutdown};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub(super) const SERVER_IP: Ipv4Addr = [127, 0, 0, 1];
pub(super) const MSS: usize = 1460;
const GUEST_PORT: u16 = 49_300;
const GUEST_ISN: u32 = 0x5100_0000;

pub(super) struct FakeGuest {
    server_port: u16,
    seq: u32,
    rcv_nxt: u32,
    capacity: usize,
    unread: usize,
    advertised: usize,
    pub(super) data: Vec<u8>,
    pub(super) fin: bool,
    /// In-order data segments dropped because they did not fit the window.
    pub(super) overruns: usize,
    /// 1-based numbers of data segments lost before the guest stack sees them
    /// (a full guest RX ring).
    pub(super) lose: Vec<usize>,
    data_segments: usize,
    /// Whether reading from the buffer announces the reopened window.
    pub(super) window_updates: bool,
    /// The window scale offered in the SYN; dropped when the SYN-ACK has none.
    shift: Option<u8>,
    pub(super) syn_ack_options: Vec<u8>,
}

impl FakeGuest {
    pub(super) fn new(server_port: u16, capacity: usize, shift: Option<u8>) -> Self {
        Self {
            server_port,
            seq: GUEST_ISN,
            rcv_nxt: 0,
            capacity,
            unread: 0,
            advertised: 0,
            data: Vec::new(),
            fin: false,
            overruns: 0,
            lose: Vec::new(),
            data_segments: 0,
            window_updates: true,
            shift,
            syn_ack_options: Vec::new(),
        }
    }

    pub(super) fn window(&self) -> usize {
        self.capacity - self.unread
    }

    fn segment(&mut self, flags: u8) -> Vec<u8> {
        // A SYN's window is never scaled; it carries the offer instead.
        let syn = flags & TCP_FLAG_SYN != 0;
        let shift = if syn { 0 } else { self.shift.unwrap_or(0) };
        let window = u16::try_from(self.window() >> shift).unwrap_or(u16::MAX);
        self.advertised = usize::from(window) << shift;
        let mut tcp = build_tcp_segment(
            Ipv4Endpoint::new(GUEST_IP, GUEST_PORT),
            Ipv4Endpoint::new(SERVER_IP, self.server_port),
            self.seq,
            self.rcv_nxt,
            flags,
            &[],
        );
        if let Some(offer) = self.shift.filter(|_| syn) {
            tcp.splice(20..20, [1, 3, 3, offer]);
            tcp[12] = 6 << 4;
        }
        tcp[14..16].copy_from_slice(&window.to_be_bytes());
        tcp[16..18].fill(0);
        let checksum = tcp_checksum(GUEST_IP, SERVER_IP, &tcp);
        tcp[16..18].copy_from_slice(&checksum.to_be_bytes());
        let ipv4 = build_ipv4_packet(GUEST_IP, SERVER_IP, IPV4_PROTOCOL_TCP, &tcp);
        EthernetFrame::build(GATEWAY_MAC, GUEST_MAC, ETHERTYPE_IPV4, &ipv4)
    }

    pub(super) fn syn(&mut self) -> Vec<u8> {
        let frame = self.segment(TCP_FLAG_SYN);
        self.seq = self.seq.wrapping_add(1);
        frame
    }

    /// Feeds one NAT frame to the guest stack; returns its answer, if any.
    pub(super) fn receive(&mut self, frame: &[u8]) -> Option<Vec<u8>> {
        let (_, ip, tcp) = parse_ipv4_tcp(frame);
        assert_eq!((ip.dst, tcp.dst_port), (GUEST_IP, GUEST_PORT));
        assert_eq!(tcp.flags & TCP_FLAG_RST, 0, "the NAT reset the download");
        if tcp.flags & TCP_FLAG_SYN != 0 {
            self.rcv_nxt = tcp.seq.wrapping_add(1);
            let header_len = usize::from(tcp.segment[12] >> 4) * 4;
            self.syn_ack_options = tcp.segment[20..header_len].to_vec();
            if syn_window_shift(tcp.segment).is_none() {
                self.shift = None;
            }
            return Some(self.segment(TCP_FLAG_ACK));
        }
        if !tcp.payload.is_empty() {
            self.data_segments += 1;
            if self.lose.contains(&self.data_segments) {
                return None;
            }
        }
        let in_order = tcp.seq == self.rcv_nxt;
        if in_order && !tcp.payload.is_empty() {
            if tcp.payload.len() > self.window() {
                self.overruns += 1;
            } else {
                self.data.extend_from_slice(tcp.payload);
                self.unread += tcp.payload.len();
                self.rcv_nxt = self.rcv_nxt.wrapping_add(tcp.payload.len() as u32);
            }
        }
        let fin = tcp.flags & TCP_FLAG_FIN != 0;
        if fin && !self.fin && tcp.seq.wrapping_add(tcp.payload.len() as u32) == self.rcv_nxt {
            self.fin = true;
            self.rcv_nxt = self.rcv_nxt.wrapping_add(1);
        }
        // An in-order bare ACK needs no answer; anything else gets the
        // cumulative ACK (a duplicate one when the segment was dropped).
        if in_order && tcp.payload.is_empty() && !fin {
            return None;
        }
        Some(self.segment(TCP_FLAG_ACK))
    }

    /// The application reads up to `bytes`; returns the window update a real
    /// stack sends once the window has grown by a full segment.
    pub(super) fn app_read(&mut self, bytes: usize) -> Option<Vec<u8>> {
        self.unread -= bytes.min(self.unread);
        (self.window_updates && self.window() >= self.advertised + MSS)
            .then(|| self.segment(TCP_FLAG_ACK))
    }
}

pub(super) type HostNat = NatBackend<HostSocketOutboundIpv4Handler>;

/// Deterministic bytes that expose loss, duplication and reordering.
pub(super) fn payload(len: usize) -> Vec<u8> {
    (0..len as u32)
        .map(|i| (i.wrapping_mul(2_654_435_761) >> 24) as u8)
        .collect()
}

/// Starts a loopback server that sends `payload` and closes, then completes
/// the guest's handshake through a fresh NAT. `None` when loopback TCP is not
/// available (sandboxed runners).
pub(super) fn connect(
    payload: &[u8],
    capacity: usize,
    shift: Option<u8>,
) -> Option<(HostNat, FakeGuest, JoinHandle<()>)> {
    let listener = loopback_tcp_listener()?;
    let port = listener.local_addr().unwrap().port();
    let bytes = payload.to_vec();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream.write_all(&bytes).unwrap();
        stream.shutdown(Shutdown::Write).unwrap();
    });
    let mut backend = NatBackend::with_outbound_handler(
        HostSocketOutboundIpv4Handler::with_dns_resolver(StdIpv4Addr::LOCALHOST),
    );
    let mut guest = FakeGuest::new(port, capacity, shift);
    backend.transmit(&guest.syn());
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        backend.poll_host_sockets();
        if let Some(frame) = backend.poll_receive() {
            let ack = guest.receive(&frame).expect("handshake ACK");
            backend.transmit(&ack);
            return Some((backend, guest, server));
        }
        assert!(Instant::now() < deadline, "no SYN-ACK from the NAT");
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// One VMM poll: the NAT reads its sockets, the guest takes every queued
/// frame and answers, then its application reads up to `app_read` bytes.
pub(super) fn tick(backend: &mut HostNat, guest: &mut FakeGuest, app_read: usize) {
    backend.poll_host_sockets();
    while let Some(frame) = backend.poll_receive() {
        if let Some(reply) = guest.receive(&frame) {
            backend.transmit(&reply);
        }
    }
    if let Some(update) = guest.app_read(app_read) {
        backend.transmit(&update);
    }
    std::thread::sleep(Duration::from_micros(100));
}

/// Ticks until the guest application has read the FIN, or until nothing new
/// arrived for `stall` (the stream is stuck).
pub(super) fn download(
    backend: &mut HostNat,
    guest: &mut FakeGuest,
    app_read: usize,
    stall: Duration,
) {
    let mut progress = (guest.data.len(), Instant::now());
    while !(guest.fin && guest.unread == 0) {
        tick(backend, guest, app_read);
        if guest.data.len() != progress.0 || guest.fin {
            progress = (guest.data.len(), Instant::now());
        } else if progress.1.elapsed() > stall {
            return;
        }
    }
}
