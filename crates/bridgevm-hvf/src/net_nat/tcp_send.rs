//! Host-to-guest TCP sending for one proxied flow.
//!
//! The guest's stack drops segments beyond its receive window (live: a slow
//! PowerShell download stopped for good at the first such hole), and it may
//! drop others under load. So the NAT reads the host socket only while the
//! guest has room (the host kernel's socket buffer then pushes back on the
//! remote server), keeps every byte until the guest acknowledges it, and
//! resends from the guest's ACK after three duplicate ACKs or a retransmission
//! timeout. A shut window is probed until it reopens.
//!
//! A window field holds at most 64 KiB, which made downloads wait for the
//! guest's ACKs (live: 149 MB/s against 311-373 MB/s unthrottled), so the
//! SYN-ACK accepts the window scaling the guest offers in its SYN.

use super::reply_queue::reply_queue_full;
use super::*;

use std::{collections::VecDeque, io::Read};

/// Payload bytes per segment: a 1500-byte MTU minus the IPv4 and TCP headers.
pub(crate) const TCP_MSS: usize = 1460;
/// Bytes sent and not yet acknowledged, for all flows together. The guest's
/// window decides how many it takes, so this bounds the memory it can pin,
/// like the reply queue's frame limit did before windows were honoured.
pub(crate) const MAX_TCP_UNACKED_BYTES: usize = 16 * 1024 * 1024;
const INITIAL_RTO_MS: u64 = 200;
const MAX_RTO_MS: u64 = 3_200;
const DUP_ACK_THRESHOLD: u8 = 3;

#[derive(Debug)]
pub(crate) struct TcpSendState {
    /// Oldest sequence number the guest has not acknowledged.
    pub(crate) una: u32,
    /// Next new sequence number (past the FIN once that is sent).
    pub(crate) next: u32,
    /// The guest's advertised receive window, in bytes.
    pub(crate) window: u32,
    /// The guest's window scale from its SYN, when it offered one.
    pub(crate) window_shift: Option<u8>,
    /// The payload from `una` on, kept until the guest acknowledges it.
    pub(crate) unacked: VecDeque<u8>,
    pub(crate) fin_sent: bool,
    pub(crate) fin_acked: bool,
    dup_acks: u8,
    /// Set while repairing a loss: the end of the flight it was found in.
    /// An ACK below it shows the next hole, which is resent at once.
    recover: Option<u32>,
    resend: bool,
    rto_ms: u64,
    /// Retransmission timer while data is in flight, persist timer while the
    /// window is shut.
    deadline_ms: Option<u64>,
}

/// What `TcpFlow::send_to_guest` did with the host socket.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HostRead {
    Idle,
    Sent,
    Failed,
}

fn seq_before(a: u32, b: u32) -> bool {
    (a.wrapping_sub(b) as i32) < 0
}

/// The window scale option of a guest SYN (RFC 7323), capped at 14.
pub(crate) fn syn_window_shift(segment: &[u8]) -> Option<u8> {
    let header_len = usize::from(*segment.get(12)? >> 4) * 4;
    let mut options = segment.get(20..header_len)?;
    while let Some((&kind, rest)) = options.split_first() {
        match kind {
            0 => return None,
            1 => options = rest,
            _ => {
                let len = usize::from(*rest.first()?);
                let option = options.get(..len).filter(|_| len >= 2)?;
                if kind == 3 && len == 3 {
                    return Some(option[2].min(14));
                }
                options = &options[len..];
            }
        }
    }
    None
}

impl TcpSendState {
    pub(crate) fn new(next: u32, window: u16, window_shift: Option<u8>) -> Self {
        Self {
            una: next,
            next,
            window: u32::from(window),
            window_shift,
            unacked: VecDeque::new(),
            fin_sent: false,
            fin_acked: false,
            dup_acks: 0,
            recover: None,
            resend: false,
            rto_ms: INITIAL_RTO_MS,
            deadline_ms: None,
        }
    }

    fn in_flight(&self) -> u32 {
        self.next.wrapping_sub(self.una)
    }

    /// A window field of a non-SYN guest segment, in bytes.
    pub(crate) fn guest_window(&self, field: u16) -> u32 {
        u32::from(field) << self.window_shift.unwrap_or(0)
    }

    /// New payload bytes the guest's window takes now. None while a loss is
    /// repaired: a guest that discards out-of-order segments would drop them
    /// too. With data in flight this waits for room for a full segment
    /// instead of sending slivers.
    fn room(&self) -> usize {
        if self.recover.is_some() {
            return 0;
        }
        let in_flight = self.in_flight();
        let room = self.window.saturating_sub(in_flight) as usize;
        if in_flight != 0 && room < TCP_MSS {
            0
        } else {
            room
        }
    }

    /// Takes one guest ACK. `bare` means the segment carried no payload, SYN
    /// or FIN, so a repeat of the last ACK counts as a duplicate.
    pub(crate) fn on_guest_ack(&mut self, ack: u32, window: u32, bare: bool) {
        let acked = ack.wrapping_sub(self.una);
        if acked != 0 && acked <= self.in_flight() {
            let data = (acked as usize).min(self.unacked.len());
            self.unacked.drain(..data);
            self.una = ack;
            self.window = window;
            self.dup_acks = 0;
            self.rto_ms = INITIAL_RTO_MS;
            self.deadline_ms = None;
            if self.fin_sent && ack == self.next {
                self.fin_acked = true;
            }
            match self.recover {
                Some(recover) if seq_before(ack, recover) => self.resend = true,
                Some(_) => self.recover = None,
                None => {}
            }
        } else if acked == 0 {
            let duplicate = bare && window == self.window && self.in_flight() != 0;
            self.window = window;
            if duplicate {
                self.dup_acks = self.dup_acks.saturating_add(1);
                if self.dup_acks == DUP_ACK_THRESHOLD && self.recover.is_none() {
                    self.recover = Some(self.next);
                    self.resend = true;
                }
            }
        }
        // Older ACKs, and ACKs of data never sent, change nothing.
    }
}

impl TcpFlow {
    /// The SYN-ACK: MSS 1460, and when the guest's SYN offered window scaling,
    /// shift 0 for ours (our window field stays in bytes).
    pub(crate) fn queue_syn_ack(
        &self,
        key: &TcpFlowKey,
        guest_mac: MacAddr,
        reply_queue: &mut VecDeque<Vec<u8>>,
    ) {
        const OPTIONS: [u8; 8] = [2, 4, (TCP_MSS >> 8) as u8, TCP_MSS as u8, 1, 3, 3, 0];
        let options = match self.send.window_shift {
            Some(_) => &OPTIONS[..],
            None => &OPTIONS[..4],
        };
        let (source, destination) = tcp_reply_endpoints(guest_mac, key);
        let flags = TCP_FLAG_SYN | TCP_FLAG_ACK;
        let seq = self.our_seq;
        if let Some(frame) = build_tcp_reply_frame(
            source,
            destination,
            seq,
            self.guest_next,
            flags,
            options,
            &[],
        ) {
            reply_queue.push_back(frame);
        }
    }

    /// Repairs losses, then forwards host data while the guest's window and
    /// `unacked_budget` have room. Retransmissions and probes are not
    /// activity: a guest that stops answering still idles out.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn send_to_guest(
        &mut self,
        key: &TcpFlowKey,
        guest_mac: MacAddr,
        reply_queue: &mut VecDeque<Vec<u8>>,
        stats: &mut NatStats,
        scratch: &mut [u8; TCP_MSS],
        unacked_budget: &mut usize,
        now: u64,
    ) -> HostRead {
        if std::mem::take(&mut self.send.resend) && self.send.in_flight() != 0 {
            self.resend_oldest(key, guest_mac, reply_queue, scratch);
            stats.tcp_segments_out = stats.tcp_segments_out.saturating_add(1);
            stats.tcp_fast_retransmits = stats.tcp_fast_retransmits.saturating_add(1);
        }
        if self
            .send
            .deadline_ms
            .is_some_and(|deadline| now >= deadline)
        {
            self.send.deadline_ms = None;
            if self.send.in_flight() != 0 {
                self.resend_oldest(key, guest_mac, reply_queue, scratch);
                self.send.recover = Some(self.send.next);
                self.send.rto_ms = (self.send.rto_ms * 2).min(MAX_RTO_MS);
                stats.tcp_segments_out = stats.tcp_segments_out.saturating_add(1);
                stats.tcp_timeout_retransmits = stats.tcp_timeout_retransmits.saturating_add(1);
            } else if self.send.window == 0 && !self.send.fin_sent {
                // One below the window, so the guest answers with its current one.
                let seq = self.send.una.wrapping_sub(1);
                queue_tcp_reply(
                    reply_queue,
                    guest_mac,
                    key,
                    seq,
                    self.guest_next,
                    TCP_FLAG_ACK,
                    &[],
                );
                self.send.rto_ms = (self.send.rto_ms * 2).min(MAX_RTO_MS);
                stats.tcp_segments_out = stats.tcp_segments_out.saturating_add(1);
                stats.tcp_zero_window_probes = stats.tcp_zero_window_probes.saturating_add(1);
            }
        }
        let mut result = HostRead::Idle;
        while !self.send.fin_sent && !reply_queue_full(reply_queue) {
            let room = self.send.room().min(*unacked_budget);
            if room == 0 {
                break;
            }
            let read = self.stream.read(&mut scratch[..room.min(TCP_MSS)]);
            let (flags, len) = match read {
                Ok(0) => (TCP_FLAG_FIN | TCP_FLAG_ACK, 0),
                Ok(len) => (TCP_FLAG_PSH | TCP_FLAG_ACK, len),
                Err(e) if would_block(&e) => {
                    stats.tcp_read_again = stats.tcp_read_again.saturating_add(1);
                    break;
                }
                Err(_) => return HostRead::Failed,
            };
            if self.send.in_flight() == 0 {
                self.send.rto_ms = INITIAL_RTO_MS;
                self.send.deadline_ms = Some(now.saturating_add(INITIAL_RTO_MS));
            }
            let payload = &scratch[..len];
            queue_tcp_reply(
                reply_queue,
                guest_mac,
                key,
                self.send.next,
                self.guest_next,
                flags,
                payload,
            );
            stats.tcp_segments_out = stats.tcp_segments_out.saturating_add(1);
            self.send.unacked.extend(payload);
            *unacked_budget -= len;
            if len == 0 {
                // The FIN takes one sequence number.
                self.send.next = self.send.next.wrapping_add(1);
                self.send.fin_sent = true;
            } else {
                self.send.next = self.send.next.wrapping_add(len as u32);
            }
            result = HostRead::Sent;
        }
        let waiting = self.send.in_flight() != 0 || (self.send.window == 0 && !self.send.fin_sent);
        if !waiting {
            self.send.deadline_ms = None;
        } else if self.send.deadline_ms.is_none() {
            self.send.deadline_ms = Some(now.saturating_add(self.send.rto_ms));
        }
        result
    }

    /// Resends the oldest unacknowledged segment (the FIN when only it is left).
    fn resend_oldest(
        &mut self,
        key: &TcpFlowKey,
        guest_mac: MacAddr,
        reply_queue: &mut VecDeque<Vec<u8>>,
        scratch: &mut [u8; TCP_MSS],
    ) {
        let unacked = &self.send.unacked;
        let len = unacked.len().min(TCP_MSS);
        let (front, back) = unacked.as_slices();
        let head = len.min(front.len());
        scratch[..head].copy_from_slice(&front[..head]);
        scratch[head..len].copy_from_slice(&back[..len - head]);
        let mut flags = TCP_FLAG_ACK;
        if len != 0 {
            flags |= TCP_FLAG_PSH;
        }
        if self.send.fin_sent && len == unacked.len() {
            flags |= TCP_FLAG_FIN;
        }
        let seq = self.send.una;
        queue_tcp_reply(
            reply_queue,
            guest_mac,
            key,
            seq,
            self.guest_next,
            flags,
            &scratch[..len],
        );
    }
}
