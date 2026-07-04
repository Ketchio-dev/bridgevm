use std::time::Instant;

use bridgevm_hvf::platform_virt::VirtPlatform;
use bridgevm_hvf::xhci::XhciSetupInputReportStats;

use super::XhciSetupInputTrigger;
use crate::xhci_hid_input::report_text::contains_bytes;

impl XhciSetupInputTrigger {
    pub(super) fn attempted_in_current_controller_generation(
        &self,
        platform: &VirtPlatform,
    ) -> bool {
        let stats = platform.xhci_setup_input_report_stats();
        self.attempted
            && self.attempted_controller_reset_generation == stats.controller_reset_generation
    }

    pub(super) fn mark_attempted_at_controller_generation(&mut self, platform: &VirtPlatform) {
        let stats = platform.xhci_setup_input_report_stats();
        self.attempted = true;
        self.attempted_controller_reset_generation = stats.controller_reset_generation;
    }

    pub(super) fn note_queue_success_without_immediate_emit(
        &mut self,
        before_stats: XhciSetupInputReportStats,
    ) {
        self.pending_emitted_reports_at_attempt = Some(emitted_reports(before_stats));
    }

    pub(crate) fn pending_host_wake_deadline_at(
        &mut self,
        platform: &VirtPlatform,
        now: Instant,
    ) -> Option<Instant> {
        self.complete_pending_fire_if_report_emitted_at(platform, now);
        if self.fired
            || self.attempted_in_current_controller_generation(platform)
            || !contains_bytes(platform.uart_output(), self.marker.as_bytes())
        {
            return None;
        }
        let marker_seen_at = *self.marker_seen_at.get_or_insert(now);
        let deadline = marker_seen_at.checked_add(self.fire_delay)?;
        (deadline > now).then_some(deadline)
    }

    pub(super) fn fire_delay_elapsed_at(&mut self, platform: &VirtPlatform, now: Instant) -> bool {
        self.complete_pending_fire_if_report_emitted_at(platform, now);
        if self.fired
            || self.attempted_in_current_controller_generation(platform)
            || !contains_bytes(platform.uart_output(), self.marker.as_bytes())
        {
            return false;
        }
        let marker_seen_at = *self.marker_seen_at.get_or_insert(now);
        match now.checked_duration_since(marker_seen_at) {
            Some(elapsed) => elapsed >= self.fire_delay,
            None => false,
        }
    }

    fn complete_pending_fire_if_report_emitted_at(
        &mut self,
        platform: &VirtPlatform,
        now: Instant,
    ) -> bool {
        let Some(emitted_at_attempt) = self.pending_emitted_reports_at_attempt else {
            return false;
        };
        let stats = platform.xhci_setup_input_report_stats();
        if emitted_reports(stats) <= emitted_at_attempt {
            return false;
        }
        self.fired_at.get_or_insert(now);
        self.record_fire(platform)
    }
}

fn emitted_reports(stats: XhciSetupInputReportStats) -> u64 {
    stats
        .emitted_key_reports
        .saturating_add(stats.emitted_release_reports)
}
