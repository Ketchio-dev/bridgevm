//! Bounded-cadence idle-flow reclamation for the per-exit NAT poll path.

use super::*;

impl HostSocketOutboundIpv4Handler {
    const MAX_IDLE_SWEEP_INTERVAL_MS: u64 = 1_000;

    pub(crate) fn evict_idle_flows(&mut self) {
        let now = self.now_ms();
        let sweep_interval = self
            .idle_timeout_ms
            .clamp(1, Self::MAX_IDLE_SWEEP_INTERVAL_MS);
        let sweep_due = self.idle_timeout_ms == 0
            || now.saturating_sub(self.last_idle_sweep_ms) >= sweep_interval;
        if sweep_due {
            self.last_idle_sweep_ms = now;
            let timeout = self.idle_timeout_ms;
            self.udp_flows
                .retain(|_, flow| now.saturating_sub(flow.last_activity) <= timeout);
            self.tcp_flows
                .retain(|_, flow| now.saturating_sub(flow.last_activity) <= timeout);
            self.icmp_flows
                .retain(|_, flow| now.saturating_sub(flow.last_activity) <= timeout);
            #[cfg(test)]
            {
                self.idle_sweep_count = self.idle_sweep_count.saturating_add(1);
            }
        }

        // Capacity is a resource-safety boundary, not an idle-time policy.
        // Enforce it on every insertion/poll call even between timed sweeps.
        evict_lru(&mut self.udp_flows, self.max_flows);
        evict_lru(&mut self.tcp_flows, self.max_flows);
        evict_lru(&mut self.icmp_flows, self.max_icmp_flows);
    }
}
