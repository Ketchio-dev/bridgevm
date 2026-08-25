//! Typed userspace-NAT counters, including fail-closed ingress decisions.

use super::Ipv4Addr;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NatStats {
    pub guest_frames: u64,
    pub arp_requests: u64,
    pub dhcp_discover: u64,
    pub dhcp_request: u64,
    pub dns_queries: u64,
    pub icmp_echo: u64,
    pub tcp_segments: u64,
    pub udp_datagrams: u64,
    pub other: u64,
    pub arp_replies: u64,
    pub dhcp_offers: u64,
    pub dhcp_acks: u64,
    pub dns_replies: u64,
    pub icmp_replies: u64,
    pub icmp_forwarded: u64,
    pub icmp_external_replies: u64,
    pub tcp_segments_out: u64,
    pub udp_datagrams_out: u64,
    pub dhcp_lease_ip: Ipv4Addr,
    pub tcp_flow_count: usize,
    pub udp_flow_count: usize,
    pub pending_replies: usize,
    pub dropped_malformed_frames: u64,
    pub dropped_oversize_frames: u64,
    pub dropped_ipv4_fragments: u64,
    pub dropped_unsupported_ipv6: u64,
    pub dropped_no_guest_mac: u64,
    pub udp_recv_again: u64,
    pub tcp_connect_again: u64,
    pub tcp_read_again: u64,
    pub tcp_write_again: u64,
    pub socket_errors: u64,
}
