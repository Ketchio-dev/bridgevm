//! The reset-cycle supervisor loop: the A15 harness contract.
//!
//! PLAN.md R1 orders a product reset as flush -> receipt -> helper exit ->
//! new helper process. The pure decision (`decide_restart`) lives in
//! supervisor.rs; this loop drives it across N cycles and records
//! what A15's soak must later assert per cycle: a new helper PID and an
//! increasing generation. The helper is a closure so the loop's contract is
//! testable without a VM; the live soak wires the real helper in.
//!
//! Flush placement: the supervisor syncs the images and writes the receipt
//! after the helper exits. Page-cache durability is per inode, not per
//! process, so a sync here makes the dead helper's writes durable before
//! any new writer exists -- the ordering the receipt proves. The in-helper
//! receipt write remains the product design (it distinguishes crash from
//! reset without trusting exit plumbing); this loop trusts `reset_requested`
//! from the helper's exit status, which for the harness is the probe's
//! documented stop reason.

use std::path::Path;

use crate::error::RuntimeError;
use crate::reset_generation::ResetGeneration;
use crate::reset_receipt::flush_and_write_receipt;
use crate::supervisor::{decide_restart, RestartDecision};

/// What one helper run reported.
pub struct HelperExit {
    pub pid: u32,
    pub reset_requested: bool,
}

/// One completed reset cycle, for the soak's per-cycle assertions.
#[derive(Debug, PartialEq, Eq)]
pub struct ResetCycle {
    pub pid: u32,
    pub generation: u64,
}

/// Run helpers until one does not request a reset or `max_cycles` is hit.
///
/// Per cycle: run the helper for the current generation; on a reset exit,
/// sync every image, write the generation-tagged receipt, and let
/// `decide_restart` authorize (and advance into) the next cycle. Any refusal
/// stops the loop with the operator-facing reason.
pub fn supervise_reset_cycles(
    mut run_helper: impl FnMut(u64) -> Result<HelperExit, RuntimeError>,
    flushed_images: &[&Path],
    receipt: &Path,
    max_cycles: u32,
) -> Result<Vec<ResetCycle>, RuntimeError> {
    let generation = ResetGeneration::new();
    let mut cycles = Vec::new();
    for _ in 0..max_cycles {
        let tag = generation.stamp();
        let exit = run_helper(tag.value())?;
        cycles.push(ResetCycle {
            pid: exit.pid,
            generation: tag.value(),
        });
        if !exit.reset_requested {
            return Ok(cycles);
        }
        flush_and_write_receipt(flushed_images, receipt, tag)?;
        match decide_restart(true, receipt, &generation) {
            RestartDecision::Restart => {}
            RestartDecision::Stop { reason } => {
                return Err(RuntimeError::RestartRefused { reason });
            }
        }
    }
    Ok(cycles)
}

#[cfg(test)]
#[path = "reset_cycles_tests.rs"]
mod tests;
