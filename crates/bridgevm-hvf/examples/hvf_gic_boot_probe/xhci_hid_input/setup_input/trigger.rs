use std::time::{Duration, Instant};

use bridgevm_hvf::platform_virt::VirtPlatform;
use bridgevm_hvf::xhci::{SetupInputAction, XhciSetupInputQueueError};

#[path = "trigger/ramfb.rs"]
mod ramfb;

use super::actions::parse_setup_input_actions;
#[cfg(test)]
use super::delay::ramfb_delay_checkpoints_from_ms;
use super::delay::{
    default_ramfb_delay_checkpoints, parse_setup_input_fire_delay_env,
    parse_setup_input_ramfb_delay_env, RamfbDelayCheckpoint,
};
use super::{XhciSetupInputEnvError, SETUP_INPUT_DEFAULT_MARKER};
use crate::xhci_hid_input::marker::ProbeMarker;
use crate::xhci_hid_input::report_text::{contains_bytes, format_action_names, queue_error_name};

#[derive(Debug)]
pub(crate) struct XhciSetupInputTrigger {
    name: &'static str,
    marker: ProbeMarker,
    actions: Vec<SetupInputAction>,
    fired: bool,
    reported_queue_rejection: bool,
    fired_at: Option<Instant>,
    marker_seen_at: Option<Instant>,
    fire_delay: Duration,
    ramfb_delay_checkpoints: Vec<RamfbDelayCheckpoint>,
}

impl XhciSetupInputTrigger {
    pub(crate) fn from_env(
        name: &'static str,
        actions_env: &'static str,
        marker_env: &'static str,
    ) -> Option<Result<Self, XhciSetupInputEnvError>> {
        let value = std::env::var(actions_env).ok()?;
        let marker = match ProbeMarker::custom_from_env(marker_env) {
            Ok(Some(marker)) => marker,
            Ok(None) => ProbeMarker::default_bytes(SETUP_INPUT_DEFAULT_MARKER),
            Err(error) => return Some(Err(XhciSetupInputEnvError::Marker(error))),
        };
        let mut trigger = match Self::from_env_value_with_marker(name, &value, marker) {
            Ok(trigger) => trigger,
            Err(error) => return Some(Err(error)),
        };
        trigger.ramfb_delay_checkpoints = match parse_setup_input_ramfb_delay_env() {
            Ok(checkpoints) => checkpoints,
            Err(error) => return Some(Err(error)),
        };
        trigger.fire_delay = match parse_setup_input_fire_delay_env() {
            Ok(delay) => delay,
            Err(error) => return Some(Err(error)),
        };
        Some(Ok(trigger))
    }

    fn from_env_value_with_marker(
        name: &'static str,
        value: &str,
        marker: ProbeMarker,
    ) -> Result<Self, XhciSetupInputEnvError> {
        Ok(Self {
            name,
            marker,
            actions: parse_setup_input_actions(value)?,
            fired: false,
            reported_queue_rejection: false,
            fired_at: None,
            marker_seen_at: None,
            fire_delay: Duration::ZERO,
            ramfb_delay_checkpoints: default_ramfb_delay_checkpoints(),
        })
    }

    #[cfg(test)]
    pub(crate) fn maybe_fire(&mut self, platform: &mut VirtPlatform) -> bool {
        self.maybe_fire_at(platform, Instant::now())
    }

    #[cfg(test)]
    fn maybe_fire_at(&mut self, platform: &mut VirtPlatform, now: Instant) -> bool {
        self.maybe_fire_by_at(platform, now, |platform, actions| {
            platform.queue_xhci_setup_input_actions(actions)
        })
    }

    fn maybe_fire_by_at<F>(&mut self, platform: &mut VirtPlatform, now: Instant, queue: F) -> bool
    where
        F: FnOnce(&mut VirtPlatform, &[SetupInputAction]) -> Result<(), XhciSetupInputQueueError>,
    {
        if !self.fire_delay_elapsed_at(platform, now) {
            return false;
        }
        match queue(platform, &self.actions) {
            Ok(()) => self.record_fire(platform),
            Err(error) => {
                self.report_queue_rejection(error);
                false
            }
        }
    }

    pub(crate) fn pending_host_wake_deadline_at(
        &mut self,
        platform: &VirtPlatform,
        now: Instant,
    ) -> Option<Instant> {
        if self.fired || !contains_bytes(platform.uart_output(), self.marker.as_bytes()) {
            return None;
        }
        let marker_seen_at = *self.marker_seen_at.get_or_insert(now);
        let deadline = marker_seen_at.checked_add(self.fire_delay)?;
        (deadline > now).then_some(deadline)
    }

    pub(crate) fn print_summary(&self, platform: &VirtPlatform) {
        let stats = platform.xhci_setup_input_report_stats();
        let marker_seen = contains_bytes(platform.uart_output(), self.marker.as_bytes());
        println!(
            "xHCI setup-input injection {}: fired={} marker_seen={} actions={} queued_actions={} queued_reports={} emitted_key_reports={} emitted_release_reports={} empty_sequence_rejections={} too_many_action_rejections={} busy_rejections={} {} ramfb_marker_intent=observe-only",
            self.name,
            self.fired,
            marker_seen,
            self.action_names(),
            stats.queued_actions,
            stats.queued_reports,
            stats.emitted_key_reports,
            stats.emitted_release_reports,
            stats.empty_sequence_rejections,
            stats.too_many_action_rejections,
            stats.busy_rejections,
            self.marker.log_summary()
        );
    }

    pub(crate) fn action_names(&self) -> String {
        format_action_names(&self.actions)
    }

    fn fire_delay_elapsed_at(&mut self, platform: &VirtPlatform, now: Instant) -> bool {
        if self.fired || !contains_bytes(platform.uart_output(), self.marker.as_bytes()) {
            return false;
        }
        let marker_seen_at = *self.marker_seen_at.get_or_insert(now);
        match now.checked_duration_since(marker_seen_at) {
            Some(elapsed) => elapsed >= self.fire_delay,
            None => false,
        }
    }

    fn record_fire(&mut self, platform: &VirtPlatform) -> bool {
        self.fired = true;
        let stats = platform.xhci_setup_input_report_stats();
        println!(
            "xHCI setup-input injection {} fired: actions={} queued_actions={} queued_reports={} emitted_key_reports={} emitted_release_reports={} rejected_count=0 {} ramfb_marker_intent=observe-only",
            self.name,
            self.action_names(),
            stats.queued_actions,
            stats.queued_reports,
            stats.emitted_key_reports,
            stats.emitted_release_reports,
            self.marker.log_summary()
        );
        true
    }

    fn report_queue_rejection(&mut self, error: XhciSetupInputQueueError) {
        if self.reported_queue_rejection {
            return;
        }
        self.reported_queue_rejection = true;
        println!(
            "xHCI setup-input injection {} rejected: actions={} queue_error={} queued_actions=0 queued_reports=0 rejected_count=1 {} ramfb_marker_intent=observe-only",
            self.name,
            self.action_names(),
            queue_error_name(error),
            self.marker.log_summary()
        );
    }

    #[cfg(test)]
    pub(crate) fn from_env_value(
        name: &'static str,
        value: &str,
    ) -> Result<Self, XhciSetupInputEnvError> {
        Self::from_env_value_with_marker(
            name,
            value,
            ProbeMarker::default_bytes(SETUP_INPUT_DEFAULT_MARKER),
        )
    }

    #[cfg(test)]
    pub(crate) fn from_env_value_with_custom_marker(
        name: &'static str,
        value: &str,
        marker: &[u8],
    ) -> Result<Self, XhciSetupInputEnvError> {
        let marker =
            ProbeMarker::custom_for_test(marker).map_err(XhciSetupInputEnvError::Marker)?;
        Self::from_env_value_with_marker(name, value, marker)
    }

    #[cfg(test)]
    pub(crate) fn marker(&self) -> &ProbeMarker {
        &self.marker
    }

    #[cfg(test)]
    pub(crate) fn from_env_value_with_ramfb_delay_ms(
        name: &'static str,
        value: &str,
        delays_ms: &[u64],
    ) -> Result<Self, XhciSetupInputEnvError> {
        let mut trigger = Self::from_env_value_with_marker(
            name,
            value,
            ProbeMarker::default_bytes(SETUP_INPUT_DEFAULT_MARKER),
        )?;
        trigger.ramfb_delay_checkpoints = ramfb_delay_checkpoints_from_ms(delays_ms)?;
        Ok(trigger)
    }
}
