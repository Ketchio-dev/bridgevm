//! One readiness syscall replaces per-exit empty reads across every NAT flow.

use super::*;

use std::collections::{HashMap, VecDeque};
use std::net::Ipv4Addr as StdIpv4Addr;
use std::os::fd::AsRawFd;
use std::time::Instant;

const READ: i16 = libc::POLLIN;
const WRITE: i16 = libc::POLLOUT;
const READINESS_ENV: &str = "BRIDGEVM_NAT_READINESS";

#[derive(Debug)]
pub(crate) struct HostSocketReadiness {
    pub(crate) enabled: bool,
    poll_fds: Vec<libc::pollfd>,
}

impl HostSocketReadiness {
    fn from_env() -> Self {
        Self::from_value(std::env::var(READINESS_ENV).ok().as_deref())
    }

    fn from_value(value: Option<&str>) -> Self {
        Self {
            enabled: value.is_none_or(|value| value.trim() != "0"),
            poll_fds: Vec::new(),
        }
    }

    fn push(&mut self, fd: libc::c_int, events: i16) {
        self.poll_fds.push(libc::pollfd {
            fd,
            events,
            revents: 0,
        });
    }

    fn any_ready(&mut self) -> bool {
        if self.poll_fds.is_empty() {
            return false;
        }
        let count = libc::nfds_t::try_from(self.poll_fds.len()).unwrap_or(libc::nfds_t::MAX);
        loop {
            // SAFETY: the vector owns `count` initialized pollfd records for
            // this zero-timeout call; poll never retains the pointer.
            let result = unsafe { libc::poll(self.poll_fds.as_mut_ptr(), count, 0) };
            if result >= 0 {
                return result != 0;
            }
            if std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted {
                return true;
            }
        }
    }
}

impl HostSocketOutboundIpv4Handler {
    pub fn new() -> Self {
        Self::with_dns_resolver(
            first_resolv_conf_nameserver().unwrap_or(StdIpv4Addr::new(1, 1, 1, 1)),
        )
    }

    pub fn with_dns_resolver(dns_resolver: StdIpv4Addr) -> Self {
        Self {
            udp_flows: HashMap::new(),
            tcp_flows: HashMap::new(),
            icmp_flows: HashMap::new(),
            pending_tcp_resets: VecDeque::new(),
            tcp_remove_scratch: Vec::new(),
            socket_readiness: HostSocketReadiness::from_env(),
            udp_recv_scratch: [0; HOST_SOCKET_UDP_RECV_SCRATCH_LEN],
            tcp_read_scratch: [0; HOST_SOCKET_TCP_READ_SCRATCH_LEN],
            icmp_recv_scratch: [0; HOST_SOCKET_ICMP_RECV_SCRATCH_LEN],
            pending_socket_errors: 0,
            dns_resolver,
            epoch: Instant::now(),
            last_idle_sweep_ms: 0,
            idle_timeout_ms: Self::DEFAULT_IDLE_TIMEOUT_MS,
            max_flows: Self::DEFAULT_MAX_FLOWS,
            max_icmp_flows: Self::DEFAULT_MAX_ICMP_FLOWS,
            tcp_isn_counter: 0x4256_0000,
            #[cfg(test)]
            idle_sweep_count: 0,
        }
    }

    fn ready_or_immediate_work(&mut self) -> bool {
        if !self.socket_readiness.enabled
            || !self.pending_tcp_resets.is_empty()
            || self
                .tcp_flows
                .values()
                .any(|flow| flow.pending_ack || flow.closed())
        {
            return true;
        }
        self.socket_readiness.poll_fds.clear();
        for flow in self.udp_flows.values() {
            self.socket_readiness.push(flow.socket.as_raw_fd(), READ);
        }
        for flow in self.tcp_flows.values() {
            let write = flow.state == TcpProxyState::Connecting || !flow.write_buf.is_empty();
            let read_events = if flow.host_fin_sent { 0 } else { READ };
            let write_events = if write { WRITE } else { 0 };
            let events = read_events | write_events;
            if events != 0 {
                self.socket_readiness.push(flow.stream.as_raw_fd(), events);
            }
        }
        #[cfg(target_os = "macos")]
        for flow in self.icmp_flows.values() {
            self.socket_readiness.push(flow.socket.fd, READ);
        }
        self.socket_readiness.any_ready()
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
        if self.pending_socket_errors != 0 {
            stats.socket_errors = stats
                .socket_errors
                .saturating_add(self.pending_socket_errors);
            self.pending_socket_errors = 0;
        }
        if self.ready_or_immediate_work() {
            self.poll_udp(guest_mac, reply_queue, stats);
            self.poll_tcp(guest_mac, reply_queue, stats);
            self.poll_icmp(guest_mac, reply_queue, stats);
        }
        self.evict_idle_flows();
    }

    fn active_flow_counts(&self) -> (usize, usize) {
        (self.tcp_flows.len(), self.udp_flows.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::UdpSocket;

    #[test]
    fn readiness_defaults_on_and_zero_is_the_legacy_kill_switch() {
        assert!(HostSocketReadiness::from_value(None).enabled);
        assert!(!HostSocketReadiness::from_value(Some("0")).enabled);
        assert!(HostSocketReadiness::from_value(Some("invalid")).enabled);
    }

    #[test]
    fn empty_and_invalid_descriptor_results_fail_safely() {
        let mut readiness = HostSocketReadiness::from_value(None);
        assert!(!readiness.any_ready());
        readiness.push(libc::c_int::MAX, READ);
        assert!(readiness.any_ready());
    }

    #[test]
    fn local_udp_payload_is_reported_ready_without_blocking() {
        let receiver = UdpSocket::bind((StdIpv4Addr::LOCALHOST, 0)).unwrap();
        receiver.set_nonblocking(true).unwrap();
        let sender = UdpSocket::bind((StdIpv4Addr::LOCALHOST, 0)).unwrap();
        let mut readiness = HostSocketReadiness::from_value(None);
        readiness.push(receiver.as_raw_fd(), READ);
        assert!(!readiness.any_ready());
        sender
            .send_to(b"ready", receiver.local_addr().unwrap())
            .unwrap();
        let deadline = Instant::now() + std::time::Duration::from_millis(100);
        while !readiness.any_ready() && Instant::now() < deadline {
            std::thread::yield_now();
        }
        assert!(readiness.any_ready());
    }
}
