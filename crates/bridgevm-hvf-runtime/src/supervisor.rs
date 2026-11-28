//! The supervisor's restart rule: no receipt, no restart.
//!
//! PLAN.md R1 orders a product reset: flush, receipt, helper exit, THEN a new
//! helper process. The decision of whether a helper exit warrants a restart
//! is policy, so it lives here with the receipt and generation types; the
//! process spawning lives with the process owner (hvf-runner). Keeping the
//! decision pure also makes the dangerous cases table-testable: a missing
//! receipt, or a stale one from generation N, proves nothing about N+1. A
//! crashed helper never gets here: `run_prepared_vm` stops on any exit that
//! is not a guest-requested reset.

use std::path::Path;

use crate::reset_generation::ResetGeneration;
use crate::reset_receipt::receipt_proves_flush;

/// What the supervisor does after a helper exit.
#[derive(Debug, PartialEq, Eq)]
pub enum RestartDecision {
    /// Start a fresh helper for the (already advanced) new generation.
    Restart,
    /// Do not start another helper; the reason is operator-facing.
    Stop { reason: &'static str },
}

/// After a guest-requested reset exit: restart only if the receipt on disk
/// proves the current generation's flush.
///
/// On `Restart` the generation has been advanced: events stamped by the dead
/// helper are stale from this moment, before any new process exists.
pub fn decide_restart(receipt: &Path, generation: &ResetGeneration) -> RestartDecision {
    if !receipt_proves_flush(receipt, generation.stamp()) {
        return RestartDecision::Stop {
            reason: "no reset receipt proves this generation's flush; refusing to restart",
        };
    }
    generation.advance();
    RestartDecision::Restart
}

#[cfg(test)]
#[path = "supervisor_tests.rs"]
mod tests;
