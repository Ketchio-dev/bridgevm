//! Host wake deadline for a paced, queued DCI5 report.

use super::VirtPlatform;
use std::time::Instant;

impl VirtPlatform {
    /// Exact host-time wake for the next queued DCI5 report. The guest TD may
    /// still NAK then, but sleeping past this point would stretch a click.
    /// Pacing runs in VM time (vm_clock); the deadline is converted once here,
    /// so after a pause the probe's host wake does not see it as long past.
    pub fn xhci_pointer_report_deadline(&self) -> Option<Instant> {
        if !self.xhci.has_queued_pointer_input_report()
            || self.xhci_pointer_report_interval.is_zero()
        {
            return None;
        }
        let due = self
            .xhci_dci5_last_emission?
            .checked_add(self.xhci_pointer_report_interval)?;
        Some(crate::vm_clock::host_at(due))
    }
}
