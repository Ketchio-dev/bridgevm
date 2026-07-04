use bridgevm_hvf::xhci::XhciEventLifecycleStats;

use super::InterruptEndpointLifecycleSummary;

impl InterruptEndpointLifecycleSummary {
    pub(in super::super) fn summary_lines(
        &self,
        event_stats: XhciEventLifecycleStats,
    ) -> Vec<String> {
        if !self.has_observations() && event_stats == XhciEventLifecycleStats::default() {
            return Vec::new();
        }
        vec![
            self.format_set_tr_dequeue_pointer_summary(),
            self.format_dci5_set_tr_dequeue_pointer_summary(),
            self.format_dci3_doorbell_summary(),
            self.format_dci5_doorbell_summary(),
            self.format_transfer_ring_summary(),
            self.format_dci5_transfer_ring_summary(),
            self.format_guest_erdp_summary(),
            format_event_lifecycle_stats(event_stats),
        ]
    }

    fn format_set_tr_dequeue_pointer_summary(&self) -> String {
        match self.last_set_tr_dequeue_pointer {
            Some(last) => format!(
                "dci3_set_tr_dequeue_pointer count={} last_slot={} last_endpoint={} last_raw_dequeue={:#x} last_dequeue={:#x} last_dcs={}",
                self.set_tr_dequeue_pointer_count,
                last.slot,
                last.endpoint,
                last.raw_dequeue,
                last.dequeue,
                last.dcs
            ),
            None => format!(
                "dci3_set_tr_dequeue_pointer count={} last=none",
                self.set_tr_dequeue_pointer_count
            ),
        }
    }

    fn format_dci5_set_tr_dequeue_pointer_summary(&self) -> String {
        match self.last_dci5_set_tr_dequeue_pointer {
            Some(last) => format!(
                "dci5_set_tr_dequeue_pointer count={} last_slot={} last_endpoint={} last_raw_dequeue={:#x} last_dequeue={:#x} last_dcs={}",
                self.dci5_set_tr_dequeue_pointer_count,
                last.slot,
                last.endpoint,
                last.raw_dequeue,
                last.dequeue,
                last.dcs
            ),
            None => format!(
                "dci5_set_tr_dequeue_pointer count={} last=none",
                self.dci5_set_tr_dequeue_pointer_count
            ),
        }
    }

    fn format_dci3_doorbell_summary(&self) -> String {
        match self.last_doorbell {
            Some(last) => format!(
                "dci3_doorbell count={} last_slot={} last_target={:#x} last_value={:#x} last_ep0_dequeue={:#x} last_dci3_dequeue={:#x} last_dci5_dequeue={:#x}",
                self.doorbell_count,
                last.slot,
                last.target,
                last.value,
                last.ep0_dequeue,
                last.dci3_dequeue,
                last.dci5_dequeue
            ),
            None => format!("dci3_doorbell count={} last=none", self.doorbell_count),
        }
    }

    fn format_dci5_doorbell_summary(&self) -> String {
        match self.last_dci5_doorbell {
            Some(last) => format!(
                "dci5_doorbell count={} last_slot={} last_target={:#x} last_value={:#x} last_ep0_dequeue={:#x} last_dci3_dequeue={:#x} last_dci5_dequeue={:#x}",
                self.dci5_doorbell_count,
                last.slot,
                last.target,
                last.value,
                last.ep0_dequeue,
                last.dci3_dequeue,
                last.dci5_dequeue
            ),
            None => format!(
                "dci5_doorbell count={} last=none",
                self.dci5_doorbell_count
            ),
        }
    }

    fn format_transfer_ring_summary(&self) -> String {
        match self.last_transfer_ring_snapshot {
            Some(last) => format!(
                "dci3_transfer_ring_snapshot count={} last_slot={} last_target={:#x} last_dequeue={:#x} trbs_read={} nonzero_trbs={} zero_type0_trbs={} first_nonzero_index={} first_zero_type0_index={} overflow={} unreadable={}",
                self.transfer_ring_snapshot_count,
                last.slot,
                last.target,
                last.dequeue,
                last.trbs_read,
                last.nonzero_trbs,
                last.zero_type0_trbs,
                format_optional_index(last.first_nonzero_index),
                format_optional_index(last.first_zero_type0_index),
                last.overflow,
                last.unreadable
            ),
            None => format!(
                "dci3_transfer_ring_snapshot count={} last=none",
                self.transfer_ring_snapshot_count
            ),
        }
    }

    fn format_dci5_transfer_ring_summary(&self) -> String {
        match self.last_dci5_transfer_ring_snapshot {
            Some(last) => format!(
                "dci5_transfer_ring_snapshot count={} last_slot={} last_target={:#x} last_dequeue={:#x} trbs_read={} nonzero_trbs={} zero_type0_trbs={} first_nonzero_index={} first_zero_type0_index={} overflow={} unreadable={}",
                self.dci5_transfer_ring_snapshot_count,
                last.slot,
                last.target,
                last.dequeue,
                last.trbs_read,
                last.nonzero_trbs,
                last.zero_type0_trbs,
                format_optional_index(last.first_nonzero_index),
                format_optional_index(last.first_zero_type0_index),
                last.overflow,
                last.unreadable
            ),
            None => format!(
                "dci5_transfer_ring_snapshot count={} last=none",
                self.dci5_transfer_ring_snapshot_count
            ),
        }
    }

    fn format_guest_erdp_summary(&self) -> String {
        format!(
            "guest_erdp0_writes count={} last_erdp0={}",
            self.guest_erdp0_write_count,
            format_optional_hex(self.last_guest_erdp0)
        )
    }
}

fn format_event_lifecycle_stats(stats: XhciEventLifecycleStats) -> String {
    format!(
        "xhci_event_posts attempts={} successes={} failures={} command_completion={} transfer={} port_status_change={} model_erdp_updates={} model_erdp_ehb_consumed={} model_last_erdp={:#x} last_event_interrupter={} last_event_gpa={:#x} last_event_parameter={:#x} last_event_status={:#x} last_event_control={:#x}",
        stats.event_post_attempts,
        stats.event_post_successes,
        stats.event_post_failures,
        stats.command_completion_event_posts,
        stats.transfer_event_posts,
        stats.port_status_change_event_posts,
        stats.erdp_updates,
        stats.erdp_ehb_consumed,
        stats.last_erdp,
        stats.last_event_interrupter,
        stats.last_event_gpa,
        stats.last_event_parameter,
        stats.last_event_status,
        stats.last_event_control
    )
}

fn format_optional_hex(value: Option<u64>) -> String {
    value.map_or_else(|| "none".to_string(), |value| format!("{value:#x}"))
}

fn format_optional_index(value: Option<u64>) -> String {
    value.map_or_else(|| "none".to_string(), |value| value.to_string())
}
