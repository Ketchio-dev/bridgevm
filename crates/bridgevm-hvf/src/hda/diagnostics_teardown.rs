//! The controller prints its diagnostic records as it is dropped.
//!
//! The probe's CoreAudio sink installs host records (its callback timing), and
//! the platform drops the controller after the final report's footer. A drop
//! body runs before the value's fields drop, so `hda poll:`, `hda stream:`,
//! `hda repeat:` and then the host's records print once each, in that order,
//! directly before the PCM sink drops and prints its lifecycle, stats and
//! continuity records: one post-footer host tail, which
//! scripts/live-gates/hvf_host_diagnostics.py admits. A controller without host
//! records prints nothing. Resets keep the records and the host records with
//! the controller (`replace_keeping_diagnostics`), so the value a reset
//! replaces prints nothing either. Printing happens after the guest stopped
//! running and changes no device state.

use super::*;

impl HdaController {
    /// What the controller prints as it is dropped: every diagnostic record
    /// once a host installed its records, else nothing.
    pub(crate) fn teardown_records(&self) -> Vec<String> {
        if self.diagnostics.host_records.is_none() {
            return Vec::new();
        }
        self.diagnostic_records()
    }
}

impl Drop for HdaController {
    fn drop(&mut self) {
        for record in self.teardown_records() {
            println!("{record}");
        }
    }
}
