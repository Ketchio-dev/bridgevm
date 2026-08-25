use super::super::*;
use super::helpers::*;
use crate::virtio_net::NetBackend;
use std::net::Ipv4Addr as StdIpv4Addr;

fn with_ipv4_fragment_bits(mut frame: Vec<u8>, bits: u16) -> Vec<u8> {
    frame[20..22].copy_from_slice(&bits.to_be_bytes());
    frame
}

#[test]
fn real_dispatch_counts_runt_as_malformed_and_emits_nothing() {
    let mut backend = NatBackend::new();
    backend.transmit(&[0; 13]);
    assert_eq!(backend.stats().dropped_malformed_frames, 1);
    assert_eq!(backend.queued_outbound_ipv4_len(), 0);
    assert!(backend.poll_receive().is_none());
}

#[test]
fn real_dispatch_accepts_mtu_boundary_and_drops_one_byte_over() {
    let mut backend = NatBackend::new();
    let at_mtu = EthernetFrame::build(
        BROADCAST_MAC,
        GUEST_MAC,
        0x88b5,
        &vec![0; ETHERNET_PAYLOAD_MTU],
    );
    let over_mtu = EthernetFrame::build(
        BROADCAST_MAC,
        GUEST_MAC,
        0x88b5,
        &vec![0; ETHERNET_PAYLOAD_MTU + 1],
    );
    backend.transmit(&at_mtu);
    backend.transmit(&over_mtu);
    let stats = backend.stats();
    assert_eq!(stats.other, 1);
    assert_eq!(stats.dropped_oversize_frames, 1);
    assert!(backend.poll_receive().is_none());
}

#[test]
fn real_dispatch_drops_first_and_nonfirst_fragments_before_transport_parse() {
    let mut backend = NatBackend::new();
    let full = udp_guest_frame(DNS_IP, 53, 53000, b"dns-query");
    backend.transmit(&with_ipv4_fragment_bits(full.clone(), 0x2000));
    backend.transmit(&with_ipv4_fragment_bits(full, 0x0001));
    let stats = backend.stats();
    assert_eq!(stats.dropped_ipv4_fragments, 2);
    assert_eq!(stats.dns_queries, 0);
    assert_eq!(backend.queued_outbound_ipv4_len(), 0);
    assert!(backend.poll_receive().is_none());
}

#[test]
fn real_dispatch_counts_ipv6_as_explicitly_unsupported() {
    let mut backend = NatBackend::new();
    let ipv6 = EthernetFrame::build(
        BROADCAST_MAC,
        GUEST_MAC,
        ETHERTYPE_IPV6,
        &[0x60, 0, 0, 0, 0, 0, 59, 64],
    );
    backend.transmit(&ipv6);
    let stats = backend.stats();
    assert_eq!(stats.dropped_unsupported_ipv6, 1);
    assert_eq!(stats.other, 0);
    assert!(backend.poll_receive().is_none());
}

#[test]
fn missing_dns_response_never_fabricates_a_guest_reply() {
    let mut backend = NatBackend::with_outbound_handler(
        HostSocketOutboundIpv4Handler::with_dns_resolver(StdIpv4Addr::UNSPECIFIED),
    );
    backend.transmit(&udp_guest_frame(DNS_IP, 53, 53000, b"query"));
    backend.poll_host_sockets();
    assert_eq!(backend.stats().dns_queries, 1);
    assert!(backend.poll_receive().is_none());
}
