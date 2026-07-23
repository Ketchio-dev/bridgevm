//! xHCI HID injection: boot-key and pointer action queueing, report draining, host-time pacing.

use super::*;
use crate::fwcfg::GuestMemoryMut;
use crate::xhci::PointerInputAction;
use crate::xhci::SetupInputAction;
use crate::xhci::XhciEventLifecycleStats;
use crate::xhci::XhciHidSemanticStats;
use crate::xhci::XhciPointerInputQueueError;
use crate::xhci::XhciPointerInputReportStats;
use crate::xhci::XhciSetupInputQueueError;
use crate::xhci::XhciSetupInputReportStats;
use std::time::Duration;
use std::time::Instant;

pub(crate) const HID_BOOT_KEYBOARD_USAGE_SPACE: u8 = 0x2c;

pub(crate) const MAX_XHCI_SETUP_INPUT_DRAIN_ATTEMPTS: usize = 16;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct XhciHidBootKeyReportStats {
    pub queued_space_reports: u64,
    pub unsupported_usage_rejections: u64,
    pub busy_rejections: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XhciHidBootKeyQueueError {
    UnsupportedUsage { usage: u8 },
    Busy,
}

/// Report-pacing decision. A zero interval or a not-yet-emitted endpoint always
/// permits the next report; otherwise the caller must wait until `interval` has
/// elapsed since `last_emission`. Kept as a free function so the gate is unit
/// tested deterministically with synthetic `Instant`s.
pub(crate) fn report_pacing_allows_emission(
    interval: Duration,
    last_emission: Option<Instant>,
    now: Instant,
) -> bool {
    if interval.is_zero() {
        return true;
    }
    match last_emission {
        None => true,
        Some(last) => now.saturating_duration_since(last) >= interval,
    }
}

impl VirtPlatform {
    /// Set the minimum host-time interval between consecutive HID interrupt-IN
    /// report emissions on DCI3/DCI5. `Duration::ZERO` disables pacing (drain a
    /// queued sequence as fast as the guest arms transfer descriptors). Live
    /// runs set this to avoid bursting keystrokes the guest then drops.
    pub fn set_xhci_report_interval(&mut self, interval: Duration) {
        self.xhci_report_interval = interval;
    }

    pub fn queue_xhci_hid_boot_key_usage(
        &mut self,
        usage: u8,
    ) -> Result<(), XhciHidBootKeyQueueError> {
        if !self.devices.xhci_present {
            self.xhci_hid_boot_key_report_stats.busy_rejections = self
                .xhci_hid_boot_key_report_stats
                .busy_rejections
                .saturating_add(1);
            return Err(XhciHidBootKeyQueueError::Busy);
        }
        if usage != HID_BOOT_KEYBOARD_USAGE_SPACE {
            self.xhci_hid_boot_key_report_stats
                .unsupported_usage_rejections = self
                .xhci_hid_boot_key_report_stats
                .unsupported_usage_rejections
                .saturating_add(1);
            return Err(XhciHidBootKeyQueueError::UnsupportedUsage { usage });
        }
        self.queue_xhci_setup_input_actions(&[SetupInputAction::Space])
            .map_err(|error| match error {
                XhciSetupInputQueueError::Busy => XhciHidBootKeyQueueError::Busy,
                XhciSetupInputQueueError::EmptySequence
                | XhciSetupInputQueueError::TooManyActions { .. } => XhciHidBootKeyQueueError::Busy,
            })?;
        self.xhci_hid_boot_key_report_stats.queued_space_reports = self
            .xhci_hid_boot_key_report_stats
            .queued_space_reports
            .saturating_add(1);
        Ok(())
    }

    pub fn queue_xhci_setup_input_actions(
        &mut self,
        actions: &[SetupInputAction],
    ) -> Result<(), XhciSetupInputQueueError> {
        if !self.devices.xhci_present {
            self.xhci_hid_boot_key_report_stats.busy_rejections = self
                .xhci_hid_boot_key_report_stats
                .busy_rejections
                .saturating_add(1);
            return Err(XhciSetupInputQueueError::Busy);
        }
        match self.xhci.queue_setup_input_actions(actions) {
            Ok(()) => Ok(()),
            Err(XhciSetupInputQueueError::Busy) => {
                self.xhci_hid_boot_key_report_stats.busy_rejections = self
                    .xhci_hid_boot_key_report_stats
                    .busy_rejections
                    .saturating_add(1);
                Err(XhciSetupInputQueueError::Busy)
            }
            Err(error) => Err(error),
        }
    }

    pub fn queue_xhci_setup_input_actions_with_mem(
        &mut self,
        actions: &[SetupInputAction],
        mem: &mut dyn GuestMemoryMut,
    ) -> Result<(), XhciSetupInputQueueError> {
        self.queue_xhci_setup_input_actions(actions)?;
        self.drain_xhci_setup_input_reports(mem);
        Ok(())
    }

    pub fn drain_xhci_setup_input_reports(&mut self, mem: &mut dyn GuestMemoryMut) -> bool {
        if !self.devices.xhci_present {
            return false;
        }
        let mut posted_completion = false;
        let stats = self.xhci.setup_input_report_stats();
        let emitted_reports = stats
            .emitted_key_reports
            .saturating_add(stats.emitted_release_reports);
        let pending_reports = stats.queued_reports.saturating_sub(emitted_reports);
        for _ in 0..pending_reports.min(MAX_XHCI_SETUP_INPUT_DRAIN_ATTEMPTS as u64) {
            if !self.report_pacing_allows_emission(self.xhci_dci3_last_emission) {
                break;
            }
            if !self.xhci.process_queued_dci3_input(mem) {
                break;
            }
            posted_completion = true;
            self.queue_xhci_completion_msix();
            self.xhci_dci3_last_emission = self.host_now.or(self.xhci_dci3_last_emission);
        }
        if posted_completion {
            self.flush_xhci_pending_msix();
        }
        posted_completion
    }

    /// Report-pacing gate: while `host_now` is unset (unit tests) or the interval
    /// is zero, every emission is allowed (unpaced). Otherwise an emission is
    /// held off until the configured interval has elapsed since this endpoint's
    /// last emission. Because a single drain call sees one fixed `host_now`, this
    /// releases at most one report per run-loop iteration once pacing is active.
    pub(crate) fn report_pacing_allows_emission(&self, last_emission: Option<Instant>) -> bool {
        match self.host_now {
            None => true,
            Some(now) => {
                report_pacing_allows_emission(self.xhci_report_interval, last_emission, now)
            }
        }
    }

    pub fn xhci_hid_boot_key_report_stats(&self) -> XhciHidBootKeyReportStats {
        self.xhci_hid_boot_key_report_stats
    }

    pub fn xhci_setup_input_report_stats(&self) -> XhciSetupInputReportStats {
        self.xhci.setup_input_report_stats()
    }

    pub fn queue_xhci_pointer_input_actions(
        &mut self,
        actions: &[PointerInputAction],
    ) -> Result<(), XhciPointerInputQueueError> {
        if !self.devices.xhci_present {
            return Err(XhciPointerInputQueueError::Busy);
        }
        self.xhci.queue_pointer_input_actions(actions)
    }

    pub fn queue_xhci_pointer_input_actions_with_mem(
        &mut self,
        actions: &[PointerInputAction],
        mem: &mut dyn GuestMemoryMut,
    ) -> Result<(), XhciPointerInputQueueError> {
        self.queue_xhci_pointer_input_actions(actions)?;
        self.drain_xhci_pointer_input_reports(mem);
        Ok(())
    }

    pub fn drain_xhci_pointer_input_reports(&mut self, mem: &mut dyn GuestMemoryMut) -> bool {
        if !self.devices.xhci_present {
            return false;
        }
        let mut posted_completion = false;
        let stats = self.xhci.pointer_input_report_stats();
        let emitted_reports = stats
            .emitted_move_reports
            .saturating_add(stats.emitted_button_reports)
            .saturating_add(stats.emitted_release_reports)
            .saturating_add(stats.emitted_wheel_reports);
        let pending_reports = stats.queued_reports.saturating_sub(emitted_reports);
        for _ in 0..pending_reports.min(MAX_XHCI_SETUP_INPUT_DRAIN_ATTEMPTS as u64) {
            if !self.report_pacing_allows_emission(self.xhci_dci5_last_emission) {
                break;
            }
            if !self.xhci.process_queued_dci5_pointer_input(mem) {
                break;
            }
            posted_completion = true;
            self.queue_xhci_completion_msix();
            self.xhci_dci5_last_emission = self.host_now.or(self.xhci_dci5_last_emission);
        }
        if posted_completion {
            self.flush_xhci_pending_msix();
        }
        posted_completion
    }

    pub fn xhci_pointer_input_report_stats(&self) -> XhciPointerInputReportStats {
        self.xhci.pointer_input_report_stats()
    }

    pub fn xhci_event_lifecycle_stats(&self) -> XhciEventLifecycleStats {
        self.xhci.event_lifecycle_stats()
    }

    pub fn xhci_hid_semantic_stats(&self) -> XhciHidSemanticStats {
        self.xhci.hid_semantic_stats()
    }
}
