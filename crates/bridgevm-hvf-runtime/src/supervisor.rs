//! The supervisor's restart rule: no receipt, no restart.
//!
//! PLAN.md R1 orders a product reset: flush, receipt, helper exit, THEN a new
//! helper process. The decision of whether a helper exit warrants a restart
//! is policy, so it lives here with the receipt and generation types; the
//! process spawning lives with the process owner (hvf-runner). Keeping the
//! decision pure also makes the dangerous cases table-testable: a crashed
//! helper must never be restarted into a disk whose flush state is unknown,
//! and a stale receipt from generation N proves nothing about N+1.

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

/// Decide from the two facts the supervisor has: whether the helper's exit
/// claimed to be a guest-requested reset, and whether the receipt on disk
/// proves the current generation's flush.
///
/// On `Restart` the generation has been advanced: events stamped by the dead
/// helper are stale from this moment, before any new process exists.
pub fn decide_restart(
    reset_requested: bool,
    receipt: &Path,
    generation: &ResetGeneration,
) -> RestartDecision {
    if !reset_requested {
        // A crash or a normal shutdown. Restarting a crashed helper would
        // hand the new VM a disk whose last writes may still be in the dead
        // process's page cache -- the exact corruption the receipt exists to
        // prevent -- and auto-restarting a clean shutdown ignores the user.
        return RestartDecision::Stop {
            reason: "helper exit was not a guest-requested reset",
        };
    }
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
