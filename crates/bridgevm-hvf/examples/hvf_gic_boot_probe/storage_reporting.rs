//! Storage persistence and block/network trace reporting.

use crate::*;

pub(crate) fn write_named_bytes(path: &str, bytes: &[u8], label: &str) {
    std::fs::write(path, bytes).unwrap_or_else(|e| panic!("{label} to {path}: {e}"));
    println!("{label}: {path} ({} bytes)", bytes.len());
}

pub(crate) fn print_media_writes(subject: &str, writes: &[MediaWrite]) {
    for write in writes {
        println!(
            "{}: {} ({} bytes)",
            write.kind.label(subject),
            write.path.display(),
            write.bytes
        );
    }
}

#[path = "storage_persistence.rs"]
mod persistence;
pub(crate) use persistence::*;

pub(crate) fn print_block_media_stats(label: &str, stats: VirtioMmioBlockStats) {
    println!(
        "{label}: version={} status={:#x} features={:#x} queue_ready={} queue_num={} qdesc={:#x} qavail={:#x} qused={:#x} notify={} requests={} reads={} unsupported={} io_errors={} bytes_read={} last_sector={:?} last_len={} last_status={:?}",
        stats.transport_version,
        stats.status,
        stats.driver_features,
        stats.queue_ready,
        stats.queue_num,
        stats.queue_desc,
        stats.queue_driver,
        stats.queue_device,
        stats.notify_count,
        stats.request_count,
        stats.read_count,
        stats.unsupported_count,
        stats.io_error_count,
        stats.bytes_read,
        stats.last_sector,
        stats.last_len,
        stats.last_status
    );
}

pub(crate) fn print_net_nat_stats(stats: NatStats) {
    println!(
        "virtio-net NAT stats: guest_frames={} arp_requests={} dhcp_discover={} dhcp_request={} dns_queries={} icmp_echo={} tcp_segments={} udp_datagrams={} other={} pending_replies={} lease={}.{}.{}.{} tcp_flows={} udp_flows={}",
        stats.guest_frames,
        stats.arp_requests,
        stats.dhcp_discover,
        stats.dhcp_request,
        stats.dns_queries,
        stats.icmp_echo,
        stats.tcp_segments,
        stats.udp_datagrams,
        stats.other,
        stats.pending_replies,
        stats.dhcp_lease_ip[0],
        stats.dhcp_lease_ip[1],
        stats.dhcp_lease_ip[2],
        stats.dhcp_lease_ip[3],
        stats.tcp_flow_count,
        stats.udp_flow_count,
    );
    println!(
        "virtio-net NAT replies: arp={} dhcp_offers={} dhcp_acks={} dns={} icmp={} tcp={} udp={}",
        stats.arp_replies,
        stats.dhcp_offers,
        stats.dhcp_acks,
        stats.dns_replies,
        stats.icmp_replies,
        stats.tcp_segments_out,
        stats.udp_datagrams_out,
    );
    println!(
        "virtio-net NAT drops/again: malformed={} oversize={} ipv4_fragments={} unsupported_ipv6={} no_guest_mac={} udp_recv_again={} tcp_connect_again={} tcp_read_again={} tcp_write_again={} socket_errors={}",
        stats.dropped_malformed_frames, stats.dropped_oversize_frames,
        stats.dropped_ipv4_fragments, stats.dropped_unsupported_ipv6, stats.dropped_no_guest_mac,
        stats.udp_recv_again,
        stats.tcp_connect_again,
        stats.tcp_read_again,
        stats.tcp_write_again,
        stats.socket_errors,
    );
}

pub(crate) fn print_block_request_trace(label: &str, trace: &[VirtioBlockRequestTrace]) {
    println!("{label}: {} entries", trace.len());
    for entry in trace {
        println!(
            "  seq={} type={} sector={} len={} status={:#x}",
            entry.sequence, entry.request_type, entry.sector, entry.data_len, entry.status
        );
    }
}
