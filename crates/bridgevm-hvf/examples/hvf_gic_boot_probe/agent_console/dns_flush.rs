//! Clears Windows' DNS cache after the NAT moves guest DNS to another server.
//!
//! The guest always asks 10.0.2.3, so it never sees the Mac change networks.
//! The NAT follows resolv.conf and counts each switch (`dns_server_changes`).
//! Windows keeps failed lookups for minutes: a name the old server could not
//! resolve (a VPN-only host before the VPN came up, a lookup that timed out
//! while the network was down) would keep failing. So a switch queues one
//! `ipconfig /flushdns` once the agent service runs, coalesced like the
//! desktop switch: at most one queued, and one running gets one follow-up.

use super::*;

pub(super) const DNS_FLUSH_COMMAND: &str = "ipconfig /flushdns";

#[derive(Default)]
pub(super) struct DnsFlush {
    /// The NAT's switch count at the last capture.
    pub(super) seen: u64,
    /// The count the last queued flush covers.
    pub(super) covered: u64,
}

pub(super) fn is_dns_flush(req: &ServiceReq) -> bool {
    matches!(req, ServiceReq::Ctl(command) if command == DNS_FLUSH_COMMAND)
}

impl AgentConsoleHarness {
    /// Under the platform guard, with the inbound capture.
    pub(super) fn observe_dns_server(&mut self, platform: &VirtPlatform) {
        if let Some(stats) = platform.virtio_net_nat_stats() {
            self.dns_flush.seen = stats.dns_server_changes;
        }
    }

    pub(super) fn enqueue_dns_flush(&mut self) {
        if self.dns_flush.seen == self.dns_flush.covered {
            return;
        }
        // A queued flush has not run yet, so it covers this switch too.
        self.dns_flush.covered = self.dns_flush.seen;
        if !self.queue.iter().any(is_dns_flush) {
            println!("BVDNS flush server_changes={}", self.dns_flush.seen);
            self.queue
                .push_back(ServiceReq::Ctl(DNS_FLUSH_COMMAND.to_string()));
        }
    }
}

#[cfg(test)]
#[path = "dns_flush_tests.rs"]
mod tests;
