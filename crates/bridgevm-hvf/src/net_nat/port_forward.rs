//! Inbound port forwards: a connection to a host port becomes a TCP
//! connection from the gateway to a guest port, then proxied like the
//! guest's own connections. Listeners bind 127.0.0.1 only, so nothing on
//! the network reaches the guest through them.

use super::*;

use std::collections::VecDeque;
use std::net::{Ipv4Addr as StdIpv4Addr, TcpListener, TcpStream};
use std::os::fd::AsRawFd;

/// Host 127.0.0.1:`host` to the guest's TCP port `guest`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PortForward {
    pub host: u16,
    pub guest: u16,
}

#[derive(Debug)]
pub(crate) struct ForwardListener {
    pub(crate) listener: TcpListener,
    pub(crate) guest_port: u16,
    pub(crate) readable: bool,
    pub(crate) watched: bool,
}

/// The gateway ports forwarded connections come from, as the guest sees them.
const FORWARD_SOURCE_PORTS: std::ops::RangeInclusive<u16> = 40_000..=49_151;

/// `BRIDGEVM_NET_FORWARD`: comma-separated `HOST:GUEST` TCP port pairs.
pub fn forwards_from_env() -> Vec<PortForward> {
    std::env::var("BRIDGEVM_NET_FORWARD")
        .map(|spec| parse_forwards(&spec))
        .unwrap_or_default()
}

/// An entry that does not parse is left out, said once in the log.
pub(crate) fn parse_forwards(spec: &str) -> Vec<PortForward> {
    let entries = spec
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty());
    entries
        .filter_map(|entry| {
            let forward = entry.split_once(':').and_then(|(host, guest)| {
                Some(PortForward {
                    host: host.trim().parse().ok()?,
                    guest: guest.trim().parse().ok().filter(|&port| port != 0)?,
                })
            });
            if forward.is_none() {
                eprintln!("virtio-net NAT port forward '{entry}' is off: write it as HOST:GUEST");
            }
            forward
        })
        .collect()
}

impl HostSocketOutboundIpv4Handler {
    /// Listens on 127.0.0.1 for each forward. A port another app holds turns
    /// that forward off, said once in the log; the others and the VM go on.
    pub fn with_forwards(mut self, forwards: &[PortForward]) -> Self {
        for forward in forwards {
            let bound = TcpListener::bind((StdIpv4Addr::LOCALHOST, forward.host))
                .and_then(|listener| listener.set_nonblocking(true).map(|()| listener));
            match bound {
                Ok(listener) => {
                    println!(
                        "virtio-net NAT port forward 127.0.0.1:{} -> guest port {}",
                        forward.host, forward.guest
                    );
                    self.forwards.push(ForwardListener {
                        listener,
                        guest_port: forward.guest,
                        readable: true,
                        watched: false,
                    });
                }
                Err(error) => eprintln!(
                    "virtio-net NAT port forward 127.0.0.1:{} -> guest port {} is off: {error}",
                    forward.host, forward.guest
                ),
            }
        }
        self
    }

    #[cfg(test)]
    pub(crate) fn forward_host_ports(&self) -> Vec<u16> {
        let ports = self
            .forwards
            .iter()
            .map(|f| f.listener.local_addr().unwrap().port());
        ports.collect()
    }

    /// Each connection waiting on a forward becomes a flow that opens the
    /// guest port with a SYN from the gateway.
    pub(crate) fn accept_forwarded(
        &mut self,
        guest_mac: MacAddr,
        reply_queue: &mut VecDeque<Vec<u8>>,
        stats: &mut NatStats,
    ) {
        for index in 0..self.forwards.len() {
            let forward = &mut self.forwards[index];
            if !forward.watched {
                // Registered on first use, like flows: the wake thread may
                // start only once the VMM has set its hook.
                forward.watched = watch_socket(&self.wake, forward.listener.as_raw_fd(), false);
            } else if !forward.readable {
                continue;
            }
            let guest_port = forward.guest_port;
            loop {
                match self.forwards[index].listener.accept() {
                    Ok((stream, _)) => {
                        self.open_forwarded(stream, guest_port, guest_mac, reply_queue, stats)
                    }
                    Err(error) => {
                        let forward = &mut self.forwards[index];
                        if would_block(&error) {
                            forward.readable = !forward.watched;
                        }
                        break;
                    }
                }
            }
        }
    }

    fn open_forwarded(
        &mut self,
        stream: TcpStream,
        guest_port: u16,
        guest_mac: MacAddr,
        reply_queue: &mut VecDeque<Vec<u8>>,
        stats: &mut NatStats,
    ) {
        if stream.set_nonblocking(true).is_err() {
            return;
        }
        let Some(source_port) = self.free_forward_source_port(guest_port) else {
            return;
        };
        let key = TcpFlowKey {
            guest_ip: GUEST_IP,
            guest_port,
            dst_ip: GATEWAY_IP,
            dst_port: source_port,
        };
        let now = self.now_ms();
        let our_seq = self.next_tcp_isn();
        let fd = stream.as_raw_fd();
        // The guest's window is unknown until its SYN-ACK: nothing is sent before.
        let mut flow = TcpFlow::new(stream, 0, our_seq, 0, None, now);
        flow.state = TcpProxyState::Opening;
        flow.watched = watch_socket(&self.wake, fd, false);
        flow.syn_sent(now);
        flow.queue_syn(&key, guest_mac, reply_queue);
        stats.tcp_segments_out = stats.tcp_segments_out.saturating_add(1);
        self.tcp_flows.insert(key, flow);
        self.evict_idle_flows();
    }

    fn free_forward_source_port(&mut self, guest_port: u16) -> Option<u16> {
        let (first, last) = (*FORWARD_SOURCE_PORTS.start(), *FORWARD_SOURCE_PORTS.end());
        let span = last - first + 1;
        (0..span).find_map(|_| {
            let port = first + self.forward_source_cursor % span;
            self.forward_source_cursor = self.forward_source_cursor.wrapping_add(1);
            let key = TcpFlowKey {
                guest_ip: GUEST_IP,
                guest_port,
                dst_ip: GATEWAY_IP,
                dst_port: port,
            };
            (!self.tcp_flows.contains_key(&key)).then_some(port)
        })
    }
}
