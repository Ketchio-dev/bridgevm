//! Bounded-cadence idle-flow reclamation for the per-exit NAT poll path.

use super::reply_queue::MAX_PENDING_REPLY_FRAMES;
use super::*;

use std::collections::VecDeque;

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
            let resets = &mut self.pending_tcp_resets;
            self.tcp_flows.retain(|key, flow| {
                let keep = now.saturating_sub(flow.last_activity) <= flow.idle_limit_ms(timeout);
                if !keep {
                    queue_eviction_reset(resets, *key, flow);
                }
                keep
            });
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
        while let Some((key, flow)) = pop_lru(&mut self.tcp_flows, self.max_flows) {
            queue_eviction_reset(&mut self.pending_tcp_resets, key, &flow);
        }
        evict_lru(&mut self.icmp_flows, self.max_icmp_flows);
    }
}

/// Tells the guest the NAT dropped this TCP flow, so the Windows app's next
/// send fails at once instead of retrying into silence until its own timeout.
/// A forwarded connection the guest never accepted (`Opening`) has nothing to
/// reset.
fn queue_eviction_reset(resets: &mut VecDeque<PendingTcpReset>, key: TcpFlowKey, flow: &TcpFlow) {
    if flow.state != TcpProxyState::Opening && resets.len() < MAX_PENDING_REPLY_FRAMES {
        resets.push_back(PendingTcpReset {
            key,
            seq: flow.send.next,
            ack: flow.guest_next,
            reason: TcpResetReason::Evicted,
            error: None,
        });
    }
}
