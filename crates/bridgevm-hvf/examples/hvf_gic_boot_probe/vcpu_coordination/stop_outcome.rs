//! Resolve joined secondary failures before any guest reset decision.

use crate::*;

impl SecondaryVcpuSet {
    pub(crate) fn psci_stop_reason(&self) -> Option<(String, bool)> {
        self.terminal_action().map(|action| match action {
            PsciTerminalAction::SystemOff => {
                (format!("PSCI {PSCI_SYSTEM_OFF:#x} (system off)"), false)
            }
            PsciTerminalAction::SystemReset => {
                (format!("PSCI {PSCI_SYSTEM_RESET:#x} (system reset)"), true)
            }
        })
    }
}

impl SecondaryVcpuStopResult {
    pub(crate) fn merge_run_error(
        &self,
        fatal_run_error: &mut bool,
        requested_reset: &mut bool,
        stop_reason: &mut String,
    ) {
        // A failure may be published after CPU0 accepted PSCI SYSTEM_RESET.
        // Resolve the joined result before reboot/recreate, keeping any actual
        // primary error already selected as the stop reason.
        if self.run_error && !*fatal_run_error {
            *stop_reason = "secondary vCPU fatal run error".into();
        }
        *fatal_run_error |= self.run_error;
        if *fatal_run_error {
            *requested_reset = false;
        }
    }
}

#[cfg(test)]
#[path = "stop_outcome_tests.rs"]
mod tests;
