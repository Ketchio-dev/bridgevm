use super::*;

use std::{
    collections::{hash_map::Entry, HashMap, VecDeque},
    io,
    net::{IpAddr, Ipv4Addr as StdIpv4Addr, Ipv6Addr, SocketAddr, UdpSocket},
    os::fd::AsRawFd,
    path::PathBuf,
    sync::OnceLock,
    time::Instant,
};

use super::reply_queue::{reply_queue_full, MAX_PENDING_REPLY_FRAMES, MAX_TCP_WRITE_BACKLOG};

#[derive(Debug)]
pub struct HostSocketOutboundIpv4Handler {
    pub(crate) udp_flows: HashMap<UdpFlowKey, UdpFlow>,
    pub(crate) tcp_flows: HashMap<TcpFlowKey, TcpFlow>,
    pub(crate) icmp_flows: HashMap<IcmpFlowKey, IcmpFlow>,
    pub(crate) pending_tcp_resets: VecDeque<PendingTcpReset>,
    pub(crate) tcp_remove_scratch: Vec<TcpFlowKey>,
    pub(crate) udp_recv_scratch: [u8; HOST_SOCKET_UDP_RECV_SCRATCH_LEN],
    pub(crate) tcp_read_scratch: [u8; TCP_MSS],
    pub(crate) icmp_recv_scratch: [u8; HOST_SOCKET_ICMP_RECV_SCRATCH_LEN],
    pub(crate) pending_socket_errors: u64,
    pub(crate) pending_tcp_noflow_drops: u64,
    pub(crate) dns: DnsUpstream,
    pub(crate) epoch: Instant,
    pub(crate) last_idle_sweep_ms: u64,
    pub(crate) idle_timeout_ms: u64,
    pub(crate) max_flows: usize,
    pub(crate) max_icmp_flows: usize,
    pub(crate) tcp_isn_counter: u32,
    /// Started with the first flow once the VMM set `NET_RX_HOOK`.
    pub(crate) wake: std::cell::OnceCell<Option<HostWake>>,
    pub(crate) ready_scratch: Vec<i32>,
    /// Host listeners of the inbound port forwards.
    pub(crate) forwards: Vec<ForwardListener>,
    pub(crate) forward_source_cursor: u16,
    /// `DNS_UNREACHABLE_LINE` was printed and no DNS reply has come back since.
    pub(crate) dns_unreachable: bool,
    #[cfg(test)]
    pub(crate) idle_sweep_count: u64,
}

/// Has the wake thread (started on first use) watch a new flow's socket.
/// False when there is no wake thread: the flow is then read on every poll.
pub(crate) fn watch_socket(
    wake: &std::cell::OnceCell<Option<HostWake>>,
    fd: i32,
    writable: bool,
) -> bool {
    wake.get_or_init(HostWake::start)
        .as_ref()
        .is_some_and(|wake| wake.watch(fd, writable))
}

pub(crate) const HOST_SOCKET_UDP_RECV_SCRATCH_LEN: usize = 2048;

/// The lines the app turns into its notice (HvfPortForwards.swift
/// HvfPortForwardNotice): Windows can't reach the Mac's DNS server, and later
/// that it can again.
pub(crate) const DNS_UNREACHABLE_LINE: &str = "BVNET dns-unreachable";
pub(crate) const DNS_REACHABLE_LINE: &str = "BVNET dns-reachable";

/// A server on the Mac's own network, such as the router: reaching it needs
/// macOS Local Network access.
pub(crate) fn is_local_network_address(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_private() || v4.is_link_local(),
        IpAddr::V6(v6) => v6.is_unicast_link_local() || v6.is_unique_local(),
    }
}
pub(crate) const HOST_SOCKET_ICMP_RECV_SCRATCH_LEN: usize = 2048;

impl Default for HostSocketOutboundIpv4Handler {
    fn default() -> Self {
        Self::new()
    }
}

impl HostSocketOutboundIpv4Handler {
    // Wall-clock idle sweep. Flow stamps advance only on real activity, so
    // this must comfortably exceed legitimate quiet periods inside a live
    // connection (TLS setup, server-side git pack computation).
    pub(crate) const DEFAULT_IDLE_TIMEOUT_MS: u64 = 300_000;
    pub(crate) const DEFAULT_MAX_FLOWS: usize = 256;
    pub(crate) const DEFAULT_MAX_ICMP_FLOWS: usize = 32;
    pub(crate) const MAX_ICMP_RECV_PER_POLL: usize = 64;

    pub fn new() -> Self {
        Self::with_resolv_conf("/etc/resolv.conf")
    }

    /// Guest DNS follows the first nameserver in `path` as the file changes.
    pub fn with_resolv_conf(path: impl Into<PathBuf>) -> Self {
        Self::with_dns(DnsUpstream::from_resolv_conf(path.into()))
    }

    /// Guest DNS always goes to `dns_resolver`.
    pub fn with_dns_resolver(dns_resolver: StdIpv4Addr) -> Self {
        Self::with_dns(DnsUpstream::fixed(dns_resolver.into()))
    }

    fn with_dns(dns: DnsUpstream) -> Self {
        Self {
            udp_flows: HashMap::new(),
            tcp_flows: HashMap::new(),
            icmp_flows: HashMap::new(),
            pending_tcp_resets: VecDeque::new(),
            tcp_remove_scratch: Vec::new(),
            udp_recv_scratch: [0; HOST_SOCKET_UDP_RECV_SCRATCH_LEN],
            tcp_read_scratch: [0; TCP_MSS],
            icmp_recv_scratch: [0; HOST_SOCKET_ICMP_RECV_SCRATCH_LEN],
            pending_socket_errors: 0,
            pending_tcp_noflow_drops: 0,
            dns,
            epoch: crate::vm_clock::active_now(),
            last_idle_sweep_ms: 0,
            idle_timeout_ms: Self::DEFAULT_IDLE_TIMEOUT_MS,
            max_flows: Self::DEFAULT_MAX_FLOWS,
            max_icmp_flows: Self::DEFAULT_MAX_ICMP_FLOWS,
            tcp_isn_counter: 0x4256_0000,
            wake: std::cell::OnceCell::new(),
            ready_scratch: Vec::new(),
            forwards: Vec::new(),
            forward_source_cursor: 0,
            dns_unreachable: false,
            #[cfg(test)]
            idle_sweep_count: 0,
        }
    }

    #[cfg(test)]
    pub(crate) fn with_idle_timeout_ms(mut self, ms: u64) -> Self {
        self.idle_timeout_ms = ms;
        self
    }

    #[cfg(test)]
    pub(crate) fn udp_flow_count(&self) -> usize {
        self.udp_flows.len()
    }

    /// Answers the wake thread's kick and marks the flows whose sockets it saw
    /// become readable (or finish connecting).
    pub(crate) fn take_ready_sockets(&mut self) {
        let Some(Some(wake)) = self.wake.get() else {
            return;
        };
        wake.polled();
        wake.take_ready(&mut self.ready_scratch);
        if self.ready_scratch.is_empty() {
            return;
        }
        for flow in self.tcp_flows.values_mut() {
            if self.ready_scratch.contains(&flow.stream.as_raw_fd()) {
                flow.readable = true;
                flow.write_armed = false;
            }
        }
        for flow in self.udp_flows.values_mut() {
            flow.readable |= self.ready_scratch.contains(&flow.socket.as_raw_fd());
        }
        for forward in &mut self.forwards {
            forward.readable |= self.ready_scratch.contains(&forward.listener.as_raw_fd());
        }
    }

    pub(crate) fn now_ms(&self) -> u64 {
        // VM time: a pause neither fires retransmits nor ages flows (vm_clock).
        let elapsed = crate::vm_clock::active_now().saturating_duration_since(self.epoch);
        u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
    }

    pub(crate) fn next_tcp_isn(&mut self) -> u32 {
        self.tcp_isn_counter = self.tcp_isn_counter.wrapping_add(0x1f3d_5b79);
        self.tcp_isn_counter
    }

    pub(crate) fn get_or_create_icmp_flow(
        &mut self,
        key: IcmpFlowKey,
        guest_ip: Ipv4Addr,
        now: u64,
    ) -> io::Result<&mut IcmpFlow> {
        get_or_insert_lru(&mut self.icmp_flows, key, self.max_icmp_flows, || {
            RawIcmpSocket::new_nonblocking().map(|socket| {
                #[cfg(target_os = "macos")]
                let _ = watch_socket(&self.wake, socket.fd, false);
                IcmpFlow {
                    socket,
                    guest_ip,
                    last_activity: now,
                }
            })
        })?
        .ok_or_else(|| io::Error::other("icmp flow missing after insert"))
    }

    pub(crate) fn handle_icmp(&mut self, packet: &Ipv4Packet<'_>) -> io::Result<()> {
        let now = self.now_ms();
        let guest_identifier = read_u16_be(packet.payload, 4)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "short icmp echo"))?;
        let key = IcmpFlowKey {
            guest_identifier,
            dst_ip: packet.dst,
        };
        let flow = self.get_or_create_icmp_flow(key, packet.src, now)?;
        flow.guest_ip = packet.src;
        flow.last_activity = now;
        flow.socket
            .send_to(packet.payload, StdIpv4Addr::from(packet.dst))
            .inspect(|bytes_sent| {
                if trace_icmp_enabled() {
                    eprintln!(
                        "bridgevm icmp forward dst={} guest_id=0x{guest_identifier:04x} bytes={bytes_sent}",
                        StdIpv4Addr::from(packet.dst),
                    );
                }
            })?;
        self.evict_idle_flows();
        Ok(())
    }

    pub(crate) fn handle_udp(&mut self, packet: &Ipv4Packet<'_>, udp: &UdpDatagram<'_>) {
        let now = self.now_ms();
        let dns = packet.dst == DNS_IP && udp.dst_port == 53;
        let socket_dst = if dns {
            self.dns.server_at(now)
        } else {
            SocketAddr::from((packet.dst, udp.dst_port))
        };
        let key = UdpFlowKey {
            guest_ip: packet.src,
            guest_port: udp.src_port,
            public_dst: packet.dst,
            public_dst_port: udp.dst_port,
            socket_dst,
        };
        let flow = match self.udp_flows.entry(key) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => {
                let local = match socket_dst {
                    SocketAddr::V4(_) => IpAddr::V4(StdIpv4Addr::UNSPECIFIED),
                    SocketAddr::V6(_) => IpAddr::V6(Ipv6Addr::UNSPECIFIED),
                };
                let socket = match UdpSocket::bind(SocketAddr::new(local, 0)).and_then(|socket| {
                    socket.set_nonblocking(true)?;
                    socket.connect(socket_dst)?;
                    Ok(socket)
                }) {
                    Ok(socket) => socket,
                    Err(error) => {
                        if dns {
                            self.note_dns_send_error(&error, socket_dst);
                        }
                        return;
                    }
                };
                let watched = watch_socket(&self.wake, socket.as_raw_fd(), false);
                entry.insert(UdpFlow {
                    socket,
                    last_activity: now,
                    readable: true,
                    watched,
                })
            }
        };
        flow.last_activity = now;
        if let Err(error) = flow.socket.send(udp.payload) {
            // A connected socket keeps the source address it was connected with. Once that is gone
            // (the Mac switched networks) every send fails, and a guest that keeps sending never lets
            // the flow idle out, so a VPN tunnel or call stayed dead. The next datagram opens a new
            // socket on the current network.
            if !would_block(&error) && error.raw_os_error() != Some(libc::ENOBUFS) {
                self.udp_flows.remove(&key);
            }
            if dns {
                self.note_dns_send_error(&error, socket_dst);
            }
        }
        self.evict_idle_flows();
    }

    /// macOS refuses a send to a local network address with EHOSTUNREACH ("No
    /// route to host") while BridgeVM's Local Network access is off (TN3179).
    /// With the Mac's DNS server on that network, Windows then resolves no
    /// names and nothing said why: say it once, until a reply comes back.
    pub(crate) fn note_dns_send_error(&mut self, error: &io::Error, server: SocketAddr) {
        if self.dns_unreachable
            || error.raw_os_error() != Some(libc::EHOSTUNREACH)
            || !is_local_network_address(server.ip())
        {
            return;
        }
        self.dns_unreachable = true;
        crate::host_log::out(DNS_UNREACHABLE_LINE);
    }

    pub(crate) fn handle_tcp(&mut self, packet: &Ipv4Packet<'_>, tcp: &TcpSegment<'_>) {
        let now = self.now_ms();
        let key = TcpFlowKey {
            guest_ip: packet.src,
            guest_port: tcp.src_port,
            dst_ip: packet.dst,
            dst_port: tcp.dst_port,
        };
        if tcp.flags & TCP_FLAG_RST != 0 {
            if let Some(flow) = self.tcp_flows.remove(&key) {
                if flow.state == TcpProxyState::Opening {
                    self.note_forward_answer(key.guest_port, false);
                }
            }
            return;
        }
        // Windows retries a lookup over TCP when the UDP answer was truncated;
        // the guest's side of the flow stays addressed to DNS_IP.
        let dns = packet.dst == DNS_IP && tcp.dst_port == 53;
        if tcp.flags & TCP_FLAG_SYN != 0
            && !self.tcp_flows.contains_key(&key)
            && (dns || is_non_local_ipv4_destination(packet.dst))
        {
            let our_seq = self.next_tcp_isn();
            let target = if dns {
                self.dns.server_at(now)
            } else {
                SocketAddr::from((packet.dst, tcp.dst_port))
            };
            let connected = match target {
                SocketAddr::V4(v4) => nonblocking_tcp_connect(*v4.ip(), v4.port()),
                // An IPv6-only resolver: reset, so the lookup fails at once.
                SocketAddr::V6(_) => Err(io::Error::from_raw_os_error(libc::EAFNOSUPPORT)),
            };
            let stream = match connected {
                Ok(stream) => stream,
                Err(e) => {
                    if self.pending_tcp_resets.len() < MAX_PENDING_REPLY_FRAMES {
                        self.pending_tcp_resets.push_back(PendingTcpReset {
                            key,
                            seq: our_seq,
                            ack: tcp.seq.wrapping_add(1),
                            reason: TcpResetReason::Connect,
                            error: Some(HostError::from(&e)),
                        });
                    }
                    return;
                }
            };
            let shift = syn_window_shift(tcp.segment);
            let guest_next = tcp.seq.wrapping_add(1);
            let fd = stream.as_raw_fd();
            let mut flow = TcpFlow::new(stream, guest_next, our_seq, tcp.window, shift, now);
            flow.watched =
                watch_socket(&self.wake, fd, true) && watch_socket(&self.wake, fd, false);
            self.tcp_flows.insert(key, flow);
            self.evict_idle_flows();
            return;
        }
        let Some(flow) = self.tcp_flows.get_mut(&key) else {
            self.pending_tcp_noflow_drops = self.pending_tcp_noflow_drops.saturating_add(1);
            return;
        };
        flow.last_activity = now;
        if flow.state == TcpProxyState::Opening {
            // Only the SYN-ACK to the NAT's SYN opens a forwarded connection.
            let syn_ack = TCP_FLAG_SYN | TCP_FLAG_ACK;
            if tcp.flags & syn_ack == syn_ack && tcp.ack == flow.our_seq.wrapping_add(1) {
                flow.open_from_syn_ack(tcp);
                self.note_forward_answer(key.guest_port, true);
            }
            return;
        }
        if tcp.flags & TCP_FLAG_ACK != 0 {
            let bare = tcp.payload.is_empty() && tcp.flags & (TCP_FLAG_SYN | TCP_FLAG_FIN) == 0;
            let window = flow.send.guest_window(tcp.window);
            flow.send.on_guest_ack(tcp.ack, window, bare);
        }
        if !tcp.payload.is_empty() && tcp.seq == flow.guest_next {
            if flow.write_buf.len() >= MAX_TCP_WRITE_BACKLOG {
                // The host peer is not reading: drop the whole segment, FIN
                // included. The ACK tells the guest the window is shut.
                flow.pending_ack = true;
                return;
            }
            flow.guest_next = flow.guest_next.wrapping_add(tcp.payload.len() as u32);
            flow.write_buf.extend(tcp.payload);
            flow.pending_ack = true;
            flow.flush_host_write();
        } else if !tcp.payload.is_empty() || tcp.seq == flow.guest_next.wrapping_sub(1) {
            // Out of order, a retransmission, a window probe or a keepalive:
            // dropped and answered with an ACK and the current window (RFC
            // 9293 3.10.7.4), so the guest resends from there at once.
            flow.pending_ack = true;
        }
        if tcp.flags & TCP_FLAG_FIN != 0
            && tcp.seq.wrapping_add(tcp.payload.len() as u32) == flow.guest_next
        {
            flow.guest_next = flow.guest_next.wrapping_add(1);
            flow.guest_fin = true;
            flow.pending_ack = true;
            flow.flush_host_write();
        }
        self.evict_idle_flows();
    }

    pub(crate) fn poll_udp(
        &mut self,
        guest_mac: Option<MacAddr>,
        reply_queue: &mut VecDeque<Vec<u8>>,
        stats: &mut NatStats,
    ) {
        let Some(guest_mac) = guest_mac else {
            return;
        };
        let now = self.now_ms();
        let recv_scratch = &mut self.udp_recv_scratch;
        let dns_unreachable = &mut self.dns_unreachable;
        for (key, flow) in &mut self.udp_flows {
            while flow.readable && !reply_queue_full(reply_queue) {
                match flow.socket.recv(recv_scratch) {
                    Ok(len) => {
                        flow.last_activity = now;
                        queue_udp_reply(
                            reply_queue,
                            guest_mac,
                            key.public_dst,
                            key.guest_ip,
                            key.public_dst_port,
                            key.guest_port,
                            &recv_scratch[..len],
                        );
                        if key.public_dst == DNS_IP && key.public_dst_port == 53 {
                            stats.dns_replies = stats.dns_replies.saturating_add(1);
                            if std::mem::take(dns_unreachable) {
                                crate::host_log::out(DNS_REACHABLE_LINE);
                            }
                        } else {
                            stats.udp_datagrams_out = stats.udp_datagrams_out.saturating_add(1);
                        }
                    }
                    Err(e) if would_block(&e) => {
                        stats.udp_recv_again = stats.udp_recv_again.saturating_add(1);
                        flow.readable = !flow.watched;
                        break;
                    }
                    Err(_) => {
                        stats.socket_errors = stats.socket_errors.saturating_add(1);
                        break;
                    }
                }
            }
        }
    }

    pub(crate) fn poll_tcp(
        &mut self,
        guest_mac: Option<MacAddr>,
        reply_queue: &mut VecDeque<Vec<u8>>,
        stats: &mut NatStats,
    ) {
        let Some(guest_mac) = guest_mac else {
            return;
        };
        while !reply_queue_full(reply_queue) {
            let Some(reset) = self.pending_tcp_resets.pop_front() else {
                break;
            };
            queue_tcp_reply(
                reply_queue,
                guest_mac,
                &reset.key,
                reset.seq,
                reset.ack,
                TCP_FLAG_RST | TCP_FLAG_ACK,
                &[],
            );
            stats.tcp_segments_out = stats.tcp_segments_out.saturating_add(1);
            note_tcp_reset(stats, reset.reason, &reset.key, None, reset.error);
        }
        let now = self.now_ms();
        // Capacity, not length: a drained deque keeps its allocation.
        let unacked: usize = self
            .tcp_flows
            .values()
            .map(|f| f.send.unacked.capacity())
            .sum();
        let mut unacked_budget = MAX_TCP_UNACKED_BYTES.saturating_sub(unacked);
        {
            let remove_scratch = &mut self.tcp_remove_scratch;
            let read_scratch = &mut self.tcp_read_scratch;
            remove_scratch.clear();
            for (key, flow) in &mut self.tcp_flows {
                // Stamp activity only when the flow actually did something
                // this poll. Refreshing every flow unconditionally disabled
                // idle eviction and flattened the LRU order, so a saturated
                // table evicted arbitrary entries - including active bulk
                // transfers (observed live as mid-download connection
                // resets). Guest-driven activity is stamped in the packet
                // handlers.
                let mut active = false;
                if flow.state == TcpProxyState::Opening {
                    // A forwarded connection's SYN the guest has not answered:
                    // resent, backing off, then given up (closing the host side).
                    match flow.syn_due(now) {
                        Some(true) => {
                            flow.queue_syn(key, guest_mac, reply_queue);
                            stats.tcp_segments_out = stats.tcp_segments_out.saturating_add(1);
                        }
                        Some(false) => remove_scratch.push(*key),
                        None => {}
                    }
                    continue;
                }
                if flow.state == TcpProxyState::Connecting && flow.readable {
                    let failed = match tcp_connect_error(&flow.stream) {
                        Ok(Some(0)) => {
                            active = true;
                            flow.state = TcpProxyState::Established;
                            flow.queue_syn_ack(key, guest_mac, reply_queue);
                            stats.tcp_segments_out = stats.tcp_segments_out.saturating_add(1);
                            None
                        }
                        Ok(Some(errno)) => Some(HostError::os(errno)),
                        Err(e) => Some(HostError::from(&e)),
                        Ok(None) => {
                            stats.tcp_connect_again = stats.tcp_connect_again.saturating_add(1);
                            flow.readable = !flow.watched;
                            None
                        }
                    };
                    if let Some(error) = failed {
                        queue_tcp_reply(
                            reply_queue,
                            guest_mac,
                            key,
                            flow.our_seq,
                            flow.guest_next,
                            TCP_FLAG_RST | TCP_FLAG_ACK,
                            &[],
                        );
                        stats.tcp_segments_out = stats.tcp_segments_out.saturating_add(1);
                        stats.socket_errors = stats.socket_errors.saturating_add(1);
                        note_tcp_reset(
                            stats,
                            TcpResetReason::Connect,
                            key,
                            Some(&flow.stream),
                            Some(error),
                        );
                        remove_scratch.push(*key);
                        continue;
                    }
                }
                if flow.state != TcpProxyState::Connecting {
                    let write_backlog = flow.write_buf.len();
                    flow.flush_host_write();
                    if flow.write_buf.len() != write_backlog {
                        active = true;
                    }
                    if !flow.write_buf.is_empty() && flow.watched && !flow.write_armed {
                        // The host socket is full: wake when it drains.
                        flow.write_armed = watch_socket(&self.wake, flow.stream.as_raw_fd(), true);
                    }
                    if flow.pending_ack || flow.window_grew() {
                        let window = flow.window_field();
                        queue_tcp_segment(
                            reply_queue,
                            guest_mac,
                            key,
                            flow.send.next,
                            flow.guest_next,
                            TCP_FLAG_ACK,
                            window,
                            &[],
                        );
                        stats.tcp_segments_out = stats.tcp_segments_out.saturating_add(1);
                        flow.pending_ack = false;
                        active = true;
                    }
                    let budget = &mut unacked_budget;
                    match flow.send_to_guest(
                        key,
                        guest_mac,
                        reply_queue,
                        stats,
                        read_scratch,
                        budget,
                        now,
                    ) {
                        HostRead::Idle => {}
                        HostRead::Sent => active = true,
                        HostRead::Failed(error) => {
                            queue_tcp_reply(
                                reply_queue,
                                guest_mac,
                                key,
                                flow.send.next,
                                flow.guest_next,
                                TCP_FLAG_RST | TCP_FLAG_ACK,
                                &[],
                            );
                            stats.tcp_segments_out = stats.tcp_segments_out.saturating_add(1);
                            stats.socket_errors = stats.socket_errors.saturating_add(1);
                            note_tcp_reset(
                                stats,
                                TcpResetReason::HostRead,
                                key,
                                Some(&flow.stream),
                                Some(error),
                            );
                            remove_scratch.push(*key);
                        }
                    }
                }
                if active {
                    flow.last_activity = now;
                }
                if flow.closed() {
                    remove_scratch.push(*key);
                }
            }
        }
        let mut removed = std::mem::take(&mut self.tcp_remove_scratch);
        for key in removed.drain(..) {
            // A forwarded connection still Opening here is one whose SYN the guest never answered.
            if let Some(flow) = self.tcp_flows.remove(&key) {
                if flow.state == TcpProxyState::Opening {
                    self.note_forward_answer(key.guest_port, false);
                }
            }
        }
        self.tcp_remove_scratch = removed;
    }

    pub(crate) fn poll_icmp(
        &mut self,
        guest_mac: Option<MacAddr>,
        reply_queue: &mut VecDeque<Vec<u8>>,
        stats: &mut NatStats,
    ) {
        let Some(guest_mac) = guest_mac else {
            return;
        };
        let now = self.now_ms();
        let recv_scratch = &mut self.icmp_recv_scratch;
        for (key, flow) in &mut self.icmp_flows {
            for _ in 0..Self::MAX_ICMP_RECV_PER_POLL {
                match flow.socket.recv_from(recv_scratch) {
                    Ok((len, _)) => {
                        flow.last_activity = now;
                        let recv = &recv_scratch[..len];
                        let first_byte = recv.first().copied().unwrap_or(0);
                        let trace_icmp = trace_icmp_enabled();
                        if let Some(frame) = build_rewritten_icmp_echo_reply_frame(
                            guest_mac,
                            GATEWAY_MAC,
                            key.dst_ip,
                            flow.guest_ip,
                            recv,
                            key.guest_identifier,
                        ) {
                            if trace_icmp {
                                trace_icmp_recv(len, first_byte, "accepted");
                            }
                            reply_queue.push_back(frame);
                            stats.icmp_external_replies =
                                stats.icmp_external_replies.saturating_add(1);
                        } else if trace_icmp {
                            trace_icmp_recv(len, first_byte, icmp_reply_rejection_reason(recv));
                        }
                    }
                    Err(e) if would_block(&e) => break,
                    Err(_) => {
                        stats.socket_errors = stats.socket_errors.saturating_add(1);
                        break;
                    }
                }
            }
        }
    }
}

impl OutboundIpv4Handler for HostSocketOutboundIpv4Handler {
    fn handle_outbound_ipv4(&mut self, packet: &Ipv4Packet<'_>) {
        match packet.protocol {
            IPV4_PROTOCOL_UDP => {
                if let Some(udp) = UdpDatagram::parse(packet.payload) {
                    self.handle_udp(packet, &udp);
                }
            }
            IPV4_PROTOCOL_TCP => {
                if let Some(tcp) = TcpSegment::parse(packet.payload) {
                    self.handle_tcp(packet, &tcp);
                }
            }
            IPV4_PROTOCOL_ICMP if self.handle_icmp(packet).is_err() => {
                self.pending_socket_errors = self.pending_socket_errors.saturating_add(1);
            }
            _ => {}
        }
    }

    fn poll_host_sockets(
        &mut self,
        guest_mac: Option<MacAddr>,
        reply_queue: &mut VecDeque<Vec<u8>>,
        stats: &mut NatStats,
    ) {
        self.take_ready_sockets();
        if self.pending_socket_errors != 0 {
            stats.socket_errors = stats
                .socket_errors
                .saturating_add(self.pending_socket_errors);
            self.pending_socket_errors = 0;
        }
        stats.tcp_noflow_drops = stats
            .tcp_noflow_drops
            .saturating_add(std::mem::take(&mut self.pending_tcp_noflow_drops));
        // Notice a resolver change without waiting for a guest query: a name
        // Windows cached as failed sends none until its cache is flushed.
        let now = self.now_ms();
        self.dns.server_at(now);
        self.poll_udp(guest_mac, reply_queue, stats);
        if let Some(guest_mac) = guest_mac {
            self.accept_forwarded(guest_mac, reply_queue, stats);
        }
        self.poll_tcp(guest_mac, reply_queue, stats);
        self.poll_icmp(guest_mac, reply_queue, stats);
        self.evict_idle_flows();
        if let Some(Some(wake)) = self.wake.get() {
            let deadline = self
                .tcp_flows
                .values()
                .filter_map(|flow| flow.send.deadline_ms())
                .min();
            wake.set_deadline(deadline.map(|ms| self.epoch + std::time::Duration::from_millis(ms)));
        }
    }

    fn active_flow_counts(&self) -> (usize, usize) {
        (self.tcp_flows.len(), self.udp_flows.len())
    }

    fn dns_server_changes(&self) -> u64 {
        self.dns.changes
    }
}

pub(crate) fn icmp_reply_rejection_reason(reply: &[u8]) -> &'static str {
    let Some(offset) = icmp_reply_payload_offset(reply) else {
        return "malformed";
    };
    let icmp = &reply[offset..];
    if icmp[0] != 0 {
        return "not_echo_reply";
    }
    if icmp[1] != 0 {
        return "nonzero_code";
    }
    "rejected"
}

pub(crate) fn trace_icmp_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("BRIDGEVM_TRACE_ICMP").is_some())
}

pub(crate) fn trace_icmp_recv(len: usize, first_byte: u8, outcome: &str) {
    eprintln!("bridgevm icmp recv bytes={len} first=0x{first_byte:02x} {outcome}");
}
