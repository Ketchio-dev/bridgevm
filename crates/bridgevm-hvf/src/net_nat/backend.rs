use super::*;

use std::collections::VecDeque;

use super::reply_queue::REPLY_BACKLOG_SKIPS_SWEEP;
use crate::virtio_net::NetBackend;

pub type MacAddr = [u8; 6];
pub type Ipv4Addr = [u8; 4];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ipv4Endpoint {
    pub address: Ipv4Addr,
    pub port: u16,
}

impl Ipv4Endpoint {
    pub const fn new(address: Ipv4Addr, port: u16) -> Self {
        Self { address, port }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EthernetIpv4Endpoint {
    pub(crate) mac: MacAddr,
    pub(crate) network: Ipv4Endpoint,
}

pub const ETHERTYPE_IPV4: u16 = 0x0800;
pub const ETHERTYPE_ARP: u16 = 0x0806;
pub const ETHERTYPE_IPV6: u16 = 0x86dd;
pub const ETHERNET_PAYLOAD_MTU: usize = 1500;
const IPV4_FRAGMENT_MASK: u16 = 0x3fff;

pub const IPV4_PROTOCOL_ICMP: u8 = 1;
pub const IPV4_PROTOCOL_TCP: u8 = 6;
pub const IPV4_PROTOCOL_UDP: u8 = 17;

pub const GUEST_IP: Ipv4Addr = [10, 0, 2, 15];
pub const GATEWAY_IP: Ipv4Addr = [10, 0, 2, 2];
pub const DNS_IP: Ipv4Addr = [10, 0, 2, 3];
pub const DHCP_SERVER_IP: Ipv4Addr = GATEWAY_IP;
pub const SUBNET_MASK: Ipv4Addr = [255, 255, 255, 0];
pub const IPV4_BROADCAST: Ipv4Addr = [255, 255, 255, 255];
pub const GUEST_SUBNET_BROADCAST: Ipv4Addr = [10, 0, 2, 255];
pub const GATEWAY_MAC: MacAddr = [0x52, 0x55, 0x0a, 0x00, 0x02, 0x02];

pub(crate) const ARP_HARDWARE_ETHERNET: u16 = 1;
pub(crate) const ARP_OPCODE_REQUEST: u16 = 1;
pub(crate) const ARP_OPCODE_REPLY: u16 = 2;

pub(crate) const DHCP_CLIENT_PORT: u16 = 68;
pub(crate) const DHCP_SERVER_PORT: u16 = 67;
pub(crate) const DHCP_MAGIC_COOKIE: [u8; 4] = [99, 130, 83, 99];
pub(crate) const DHCP_FLAG_BROADCAST: u16 = 0x8000;
pub(crate) const DHCP_OPT_SUBNET_MASK: u8 = 1;
pub(crate) const DHCP_OPT_ROUTER: u8 = 3;
pub(crate) const DHCP_OPT_DNS: u8 = 6;
pub(crate) const DHCP_OPT_LEASE_TIME: u8 = 51;
pub(crate) const DHCP_OPT_MESSAGE_TYPE: u8 = 53;
pub(crate) const DHCP_OPT_SERVER_ID: u8 = 54;
pub(crate) const DHCP_OPT_END: u8 = 255;
pub(crate) const DHCP_DISCOVER: u8 = 1;
pub(crate) const DHCP_OFFER: u8 = 2;
pub(crate) const DHCP_REQUEST: u8 = 3;
pub(crate) const DHCP_ACK: u8 = 5;
pub(crate) const DHCP_LEASE_SECONDS: u32 = 86_400;
pub(crate) const DHCP_REPLY_FIXED_LEN: usize = 240;
pub(crate) const DHCP_REPLY_OPTIONS_LEN: usize = 3 + (5 * 6) + 1;
pub(crate) const DHCP_REPLY_PAYLOAD_LEN: usize = DHCP_REPLY_FIXED_LEN + DHCP_REPLY_OPTIONS_LEN;
#[cfg(test)]
pub(crate) const DHCP_OPT_REQUESTED_IP: u8 = 50;

/// Stage 2b extension seam for internet-bound IPv4 traffic.
///
/// `packet.bytes` is the guest's IPv4 datagram with the Ethernet header
/// already stripped. The stage 2a implementation uses
/// `QueuedOutboundIpv4Handler`, which stores those datagrams for deterministic
/// tests. Stage 2b should implement this trait by translating outbound TCP/UDP
/// flows to host sockets and feeding socket completions back into the NAT
/// receive queue as Ethernet-framed IPv4 packets.
pub trait OutboundIpv4Handler {
    fn handle_outbound_ipv4(&mut self, packet: &Ipv4Packet<'_>);
    fn poll_host_sockets(
        &mut self,
        _guest_mac: Option<MacAddr>,
        _reply_queue: &mut VecDeque<Vec<u8>>,
        _stats: &mut NatStats,
    ) {
    }
    fn active_flow_counts(&self) -> (usize, usize) {
        (0, 0)
    }
    fn dns_server_changes(&self) -> u64 {
        0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NatBackend<H = QueuedOutboundIpv4Handler> {
    pub(crate) guest_mac: Option<MacAddr>,
    pub(crate) reply_queue: VecDeque<Vec<u8>>,
    pub(crate) outbound_ipv4: H,
    pub(crate) stats: NatStats,
}

impl Default for NatBackend<QueuedOutboundIpv4Handler> {
    fn default() -> Self {
        Self::new()
    }
}

impl NatBackend<QueuedOutboundIpv4Handler> {
    pub fn new() -> Self {
        Self::with_outbound_handler(QueuedOutboundIpv4Handler::default())
    }

    pub fn poll_outbound_ipv4(&mut self) -> Option<Vec<u8>> {
        self.outbound_ipv4.pop_front()
    }

    pub fn queued_outbound_ipv4_len(&self) -> usize {
        self.outbound_ipv4.len()
    }
}

impl<H> NatBackend<H> {
    pub fn with_outbound_handler(outbound_ipv4: H) -> Self {
        Self {
            guest_mac: None,
            reply_queue: VecDeque::new(),
            outbound_ipv4,
            stats: NatStats::default(),
        }
    }

    pub fn guest_mac(&self) -> Option<MacAddr> {
        self.guest_mac
    }

    pub fn pending_receive_len(&self) -> usize {
        self.reply_queue.len()
    }

    pub fn outbound_ipv4_handler(&self) -> &H {
        &self.outbound_ipv4
    }

    pub fn outbound_ipv4_handler_mut(&mut self) -> &mut H {
        &mut self.outbound_ipv4
    }

    pub fn stats(&self) -> NatStats
    where
        H: OutboundIpv4Handler,
    {
        let (tcp_flow_count, udp_flow_count) = self.outbound_ipv4.active_flow_counts();
        NatStats {
            tcp_flow_count,
            udp_flow_count,
            pending_replies: self.reply_queue.len(),
            dns_server_changes: self.outbound_ipv4.dns_server_changes(),
            ..self.stats
        }
    }
}

impl<H: OutboundIpv4Handler + Send> NetBackend for NatBackend<H> {
    fn transmit(&mut self, frame: &[u8]) {
        self.stats.guest_frames = self.stats.guest_frames.saturating_add(1);
        let Some(eth) = EthernetFrame::parse(frame) else {
            self.stats.dropped_malformed_frames =
                self.stats.dropped_malformed_frames.saturating_add(1);
            return;
        };
        if eth.payload.len() > ETHERNET_PAYLOAD_MTU {
            self.stats.dropped_oversize_frames =
                self.stats.dropped_oversize_frames.saturating_add(1);
            return;
        }
        if self.guest_mac.is_none() {
            self.guest_mac = Some(eth.src);
        }

        match eth.ethertype {
            ETHERTYPE_ARP => self.handle_arp(&eth),
            ETHERTYPE_IPV4 => self.handle_ipv4(&eth),
            ETHERTYPE_IPV6 => {
                self.stats.dropped_unsupported_ipv6 =
                    self.stats.dropped_unsupported_ipv6.saturating_add(1);
            }
            _ => {
                self.stats.other = self.stats.other.saturating_add(1);
            }
        }
    }

    fn poll_receive(&mut self) -> Option<Vec<u8>> {
        self.reply_queue.pop_front()
    }

    fn poll_receive_into(&mut self, out: &mut Vec<u8>) -> bool {
        let Some(mut frame) = self.reply_queue.pop_front() else {
            return false;
        };
        out.clear();
        out.append(&mut frame);
        true
    }

    fn poll_host_sockets(&mut self) {
        if self.reply_queue.len() >= REPLY_BACKLOG_SKIPS_SWEEP {
            self.stats.sweeps_deferred = self.stats.sweeps_deferred.saturating_add(1);
            return;
        }
        self.outbound_ipv4.poll_host_sockets(
            self.guest_mac,
            &mut self.reply_queue,
            &mut self.stats,
        );
    }
}

impl<H: OutboundIpv4Handler> NatBackend<H> {
    pub(crate) fn handle_arp(&mut self, eth: &EthernetFrame<'_>) {
        let Some(request) = ArpPacket::parse(eth.payload) else {
            self.stats.dropped_malformed_frames =
                self.stats.dropped_malformed_frames.saturating_add(1);
            return;
        };
        if request.opcode != ARP_OPCODE_REQUEST {
            self.stats.other = self.stats.other.saturating_add(1);
            return;
        }
        self.stats.arp_requests = self.stats.arp_requests.saturating_add(1);
        if request.target_ip != GATEWAY_IP && request.target_ip != DNS_IP {
            return;
        }
        let reply_ip = request.target_ip;

        let Some(dst_mac) = self.guest_mac else {
            self.stats.dropped_no_guest_mac = self.stats.dropped_no_guest_mac.saturating_add(1);
            return;
        };
        self.queue_reply(build_arp_reply_frame(
            dst_mac,
            GATEWAY_MAC,
            reply_ip,
            request.sender_mac,
            request.sender_ip,
        ));
        self.stats.arp_replies = self.stats.arp_replies.saturating_add(1);
    }

    pub(crate) fn handle_ipv4(&mut self, eth: &EthernetFrame<'_>) {
        let Some(packet) = Ipv4Packet::parse(eth.payload) else {
            self.stats.dropped_malformed_frames =
                self.stats.dropped_malformed_frames.saturating_add(1);
            return;
        };
        if packet.flags_fragment & IPV4_FRAGMENT_MASK != 0 {
            self.stats.dropped_ipv4_fragments = self.stats.dropped_ipv4_fragments.saturating_add(1);
            return;
        }

        if packet.protocol == IPV4_PROTOCOL_UDP {
            let Some(udp) = UdpDatagram::parse(packet.payload) else {
                self.stats.dropped_malformed_frames =
                    self.stats.dropped_malformed_frames.saturating_add(1);
                return;
            };
            if udp.src_port == DHCP_CLIENT_PORT && udp.dst_port == DHCP_SERVER_PORT {
                self.handle_dhcp(&packet, &udp);
                return;
            }
            if packet.dst == DNS_IP && udp.dst_port == 53 {
                self.stats.dns_queries = self.stats.dns_queries.saturating_add(1);
            } else {
                self.stats.udp_datagrams = self.stats.udp_datagrams.saturating_add(1);
            }
            if (packet.dst == DNS_IP && udp.dst_port == 53)
                || is_non_local_ipv4_destination(packet.dst)
            {
                self.outbound_ipv4.handle_outbound_ipv4(&packet);
            } else {
                self.stats.other = self.stats.other.saturating_add(1);
            }
            return;
        }

        if packet.protocol == IPV4_PROTOCOL_ICMP {
            self.handle_icmp(&packet);
            return;
        }

        // The gateway too: forwarded connections come from it, so the guest's
        // side of them is addressed to it (handle_tcp opens no flow there).
        if packet.protocol == IPV4_PROTOCOL_TCP
            && (is_non_local_ipv4_destination(packet.dst) || packet.dst == GATEWAY_IP)
        {
            self.stats.tcp_segments = self.stats.tcp_segments.saturating_add(1);
            self.outbound_ipv4.handle_outbound_ipv4(&packet);
        } else {
            self.stats.other = self.stats.other.saturating_add(1);
        }
    }

    pub(crate) fn handle_dhcp(&mut self, packet: &Ipv4Packet<'_>, udp: &UdpDatagram<'_>) {
        let Some(request) = DhcpRequest::parse(udp.payload) else {
            return;
        };
        let reply_type = match request.message_type {
            DHCP_DISCOVER => {
                self.stats.dhcp_discover = self.stats.dhcp_discover.saturating_add(1);
                DHCP_OFFER
            }
            DHCP_REQUEST => {
                self.stats.dhcp_request = self.stats.dhcp_request.saturating_add(1);
                DHCP_ACK
            }
            _ => return,
        };

        let dst_ip = dhcp_reply_destination(&request);
        let Some(dst_mac) = self.guest_mac else {
            self.stats.dropped_no_guest_mac = self.stats.dropped_no_guest_mac.saturating_add(1);
            return;
        };
        let Some(frame) = build_dhcp_reply_frame(
            dst_mac,
            GATEWAY_MAC,
            dst_ip,
            &request,
            reply_type,
            packet.identification,
        ) else {
            return;
        };
        self.queue_reply(frame);
        self.stats.dhcp_lease_ip = GUEST_IP;
        if reply_type == DHCP_OFFER {
            self.stats.dhcp_offers = self.stats.dhcp_offers.saturating_add(1);
        } else {
            self.stats.dhcp_acks = self.stats.dhcp_acks.saturating_add(1);
        }
    }

    pub(crate) fn handle_icmp(&mut self, packet: &Ipv4Packet<'_>) {
        match classify_icmp_echo(packet.dst, packet.payload) {
            IcmpEchoRoute::Gateway => {
                self.stats.icmp_echo = self.stats.icmp_echo.saturating_add(1);
                if self.queue_gateway_icmp_echo_reply(packet) {
                    self.stats.icmp_replies = self.stats.icmp_replies.saturating_add(1);
                }
            }
            IcmpEchoRoute::External => {
                self.stats.icmp_echo = self.stats.icmp_echo.saturating_add(1);
                self.stats.icmp_forwarded = self.stats.icmp_forwarded.saturating_add(1);
                self.outbound_ipv4.handle_outbound_ipv4(packet);
            }
            IcmpEchoRoute::Other => {
                self.stats.other = self.stats.other.saturating_add(1);
            }
        }
    }

    pub(crate) fn queue_gateway_icmp_echo_reply(&mut self, packet: &Ipv4Packet<'_>) -> bool {
        let Some(dst_mac) = self.guest_mac else {
            self.stats.dropped_no_guest_mac = self.stats.dropped_no_guest_mac.saturating_add(1);
            return false;
        };
        let Some(frame) = build_icmp_echo_reply_frame(
            dst_mac,
            GATEWAY_MAC,
            GATEWAY_IP,
            packet.src,
            packet.payload,
            packet.identification,
        ) else {
            return false;
        };
        self.queue_reply(frame);
        true
    }
}

impl NatBackend<HostSocketOutboundIpv4Handler> {
    /// The live NAT, with the port forwards `BRIDGEVM_NET_FORWARD` lists.
    pub fn new_host_socket() -> Self {
        let handler = HostSocketOutboundIpv4Handler::new().with_forwards(&forwards_from_env());
        Self::with_outbound_handler(handler)
    }
}
