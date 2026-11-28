use super::reply_queue::MAX_TCP_WRITE_BACKLOG;
use super::*;

use std::{
    collections::{hash_map::Entry, HashMap, VecDeque},
    io::Write,
    net::{TcpStream, UdpSocket},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct UdpFlowKey {
    pub(crate) guest_ip: Ipv4Addr,
    pub(crate) guest_port: u16,
    pub(crate) public_dst: Ipv4Addr,
    pub(crate) public_dst_port: u16,
    /// The host socket's peer; for 10.0.2.3:53 the resolver at flow creation.
    pub(crate) socket_dst: std::net::SocketAddr,
}

#[derive(Debug)]
pub(crate) struct UdpFlow {
    pub(crate) socket: UdpSocket,
    pub(crate) last_activity: u64,
    /// The host socket may have data. A socket the wake thread watches
    /// (`watched`) is read only after its event and until a read would block;
    /// any other socket is read on every poll.
    pub(crate) readable: bool,
    pub(crate) watched: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct IcmpFlowKey {
    pub(crate) guest_identifier: u16,
    pub(crate) dst_ip: Ipv4Addr,
}

#[derive(Debug)]
pub(crate) struct IcmpFlow {
    pub(crate) socket: RawIcmpSocket,
    pub(crate) guest_ip: Ipv4Addr,
    pub(crate) last_activity: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct TcpFlowKey {
    pub(crate) guest_ip: Ipv4Addr,
    pub(crate) guest_port: u16,
    pub(crate) dst_ip: Ipv4Addr,
    pub(crate) dst_port: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PendingTcpReset {
    pub(crate) key: TcpFlowKey,
    pub(crate) seq: u32,
    pub(crate) ack: u32,
    pub(crate) reason: TcpResetReason,
    /// None when the NAT dropped the flow itself.
    pub(crate) error: Option<HostError>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TcpProxyState {
    Connecting,
    /// A forwarded host connection: the NAT's SYN waits for the guest's SYN-ACK.
    Opening,
    Established,
}

/// RFC 5382 REQ-5: an established TCP connection's idle timeout is at least 2 h 4 min.
pub(crate) const ESTABLISHED_TCP_IDLE_TIMEOUT_MS: u64 = 124 * 60 * 1000;

#[derive(Debug)]
pub(crate) struct TcpFlow {
    pub(crate) stream: TcpStream,
    pub(crate) state: TcpProxyState,
    pub(crate) guest_next: u32,
    pub(crate) our_seq: u32,
    /// The host-to-guest direction (`tcp_send.rs`).
    pub(crate) send: TcpSendState,
    pub(crate) write_buf: VecDeque<u8>,
    pub(crate) pending_ack: bool,
    pub(crate) guest_fin: bool,
    /// The guest FIN has reached the host, after every acknowledged byte.
    pub(crate) host_write_closed: bool,
    pub(crate) last_activity: u64,
    /// The host socket may have data. A socket the wake thread watches
    /// (`watched`) is read only after its event and until a read would block;
    /// any other socket is read on every poll.
    pub(crate) readable: bool,
    pub(crate) watched: bool,
    /// The receive window (bytes) our last segment to the guest announced.
    pub(crate) advertised: u32,
    /// A one-shot writable event is armed for a write backlog.
    pub(crate) write_armed: bool,
}

/// Our window scale when the guest's SYN offered one (RFC 7323): the window
/// field then counts 64-byte units and can announce the whole write backlog.
/// A fixed 64 KiB window held guest uploads to about 60 MB/s
/// (net-profile-20261106, QEMU/HVF about 135).
pub(crate) const OUR_WINDOW_SHIFT: u8 = 6;

impl TcpFlow {
    /// The window field our next segment carries: what the write backlog
    /// still takes, so a guest that honours it is never dropped. Records the
    /// announced window for the window update rule.
    pub(crate) fn window_field(&mut self) -> u16 {
        let shift = self.our_shift();
        let field = u16::try_from(self.free_window() >> shift).unwrap_or(u16::MAX);
        self.advertised = u32::from(field) << shift;
        field
    }

    /// The window grew by a quarter of the most this flow can announce since
    /// the last announcement: tell the guest, which may be waiting on a
    /// closed window. Unscaled flows therefore update in 16 KiB steps.
    pub(crate) fn window_grew(&self) -> bool {
        let limit = MAX_TCP_WRITE_BACKLOG.min(usize::from(u16::MAX) << self.our_shift());
        self.free_window().min(limit) >= self.advertised as usize + limit / 4
    }

    fn our_shift(&self) -> u8 {
        if self.send.window_shift.is_some() {
            OUR_WINDOW_SHIFT
        } else {
            0
        }
    }

    fn free_window(&self) -> usize {
        MAX_TCP_WRITE_BACKLOG.saturating_sub(self.write_buf.len())
    }

    /// How long this flow may sit idle. An established connection with no FIN either way keeps
    /// at least 2 h 4 min (RFC 5382; Windows' keepalive is 2 h): an SSH or database session left
    /// alone was cut after 5 min and Windows was never told. Connecting and closing flows keep
    /// the short `transitory` timeout.
    pub(crate) fn idle_limit_ms(&self, transitory: u64) -> u64 {
        if self.state == TcpProxyState::Established && !self.guest_fin && !self.send.fin_sent {
            transitory.max(ESTABLISHED_TCP_IDLE_TIMEOUT_MS)
        } else {
            transitory
        }
    }

    pub(crate) fn new(
        stream: TcpStream,
        guest_next: u32,
        our_seq: u32,
        guest_window: u16,
        guest_window_shift: Option<u8>,
        last_activity: u64,
    ) -> Self {
        Self {
            stream,
            state: TcpProxyState::Connecting,
            guest_next,
            our_seq,
            send: TcpSendState::new(our_seq.wrapping_add(1), guest_window, guest_window_shift),
            write_buf: VecDeque::new(),
            pending_ack: false,
            guest_fin: false,
            host_write_closed: false,
            last_activity,
            readable: true,
            watched: false,
            advertised: MAX_TCP_WRITE_BACKLOG as u32,
            write_armed: false,
        }
    }

    pub(crate) fn flush_host_write(&mut self) {
        while !self.write_buf.is_empty() {
            let contiguous = self.write_buf.make_contiguous();
            match self.stream.write(contiguous) {
                Ok(0) => break,
                Ok(len) => {
                    self.write_buf.drain(..len);
                }
                Err(e) if would_block(&e) => break,
                Err(_) => break,
            }
        }
        // A drained VecDeque keeps its allocation, which an upload grows to about 6 MB per
        // connection; give it back, as the guest-bound direction does (tcp_send.rs).
        if self.write_buf.is_empty() && self.write_buf.capacity() != 0 {
            self.write_buf = VecDeque::new();
        }
        if self.guest_fin && self.write_buf.is_empty() && !self.host_write_closed {
            let _ = self.stream.shutdown(std::net::Shutdown::Write);
            self.host_write_closed = true;
        }
    }

    pub(crate) fn closed(&self) -> bool {
        self.host_write_closed && self.send.fin_acked
    }
}

pub(crate) fn evict_lru<K: Copy + Eq + std::hash::Hash, V: HasLastActivity>(
    map: &mut HashMap<K, V>,
    cap: usize,
) {
    while pop_lru(map, cap).is_some() {}
}

/// Removes and returns the least recently active flow while `map` holds more than `cap`.
pub(crate) fn pop_lru<K: Copy + Eq + std::hash::Hash, V: HasLastActivity>(
    map: &mut HashMap<K, V>,
    cap: usize,
) -> Option<(K, V)> {
    if map.len() <= cap {
        return None;
    }
    let (&key, _) = map.iter().min_by_key(|(_, flow)| flow.last_activity())?;
    map.remove_entry(&key)
}

pub(crate) fn get_or_insert_lru<K, V, E, F>(
    map: &mut HashMap<K, V>,
    key: K,
    cap: usize,
    create: F,
) -> Result<Option<&mut V>, E>
where
    K: Copy + Eq + std::hash::Hash,
    V: HasLastActivity,
    F: FnOnce() -> Result<V, E>,
{
    if let Entry::Vacant(entry) = map.entry(key) {
        entry.insert(create()?);
        evict_lru(map, cap);
    }
    Ok(map.get_mut(&key))
}

pub(crate) trait HasLastActivity {
    fn last_activity(&self) -> u64;
}

impl HasLastActivity for UdpFlow {
    fn last_activity(&self) -> u64 {
        self.last_activity
    }
}

impl HasLastActivity for TcpFlow {
    fn last_activity(&self) -> u64 {
        self.last_activity
    }
}

impl HasLastActivity for IcmpFlow {
    fn last_activity(&self) -> u64 {
        self.last_activity
    }
}
