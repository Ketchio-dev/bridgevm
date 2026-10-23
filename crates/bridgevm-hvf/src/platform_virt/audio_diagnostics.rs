//! The HDA diagnostic records (hda/diagnostics.rs), which the controller
//! prints as the platform drops it once a host sink added its records.

use super::*;
use crate::hda::{HdaController, HdaHostRecords};

impl VirtPlatform {
    /// `hda poll:`, `hda stream:` and `hda repeat:`, then the host sink's
    /// records; none without an HDA controller.
    pub fn hda_diagnostic_records(&self) -> Vec<String> {
        self.hda
            .as_ref()
            .map(HdaController::diagnostic_records)
            .unwrap_or_default()
    }

    /// Add the host sink's records, such as CoreAudio callback timing, after
    /// the controller's own; with them the controller prints all at drop.
    pub fn set_hda_diagnostic_host_records(&mut self, records: HdaHostRecords) {
        if let Some(hda) = self.hda.as_mut() {
            hda.set_diagnostic_host_records(Some(records));
        }
    }
}
