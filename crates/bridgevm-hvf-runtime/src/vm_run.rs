//! The composed product lifecycle: manifest -> leases -> helper generations.
//!
//! One call ties the R1 slices together. `prepare` takes the exclusive
//! writer leases (a second writer fails here, by name). Each generation
//! spawns the helper directly from the manifest (argv, env_clear allowlist,
//! no shell). A helper exiting with the reset code means the guest asked
//! for SYSTEM_RESET: flush the leased images, write the generation-tagged
//! receipt, let `decide_restart` authorize the next generation. Any other
//! clean exit ends the run; a crash refuses restart by construction.

use crate::manifest::LaunchManifest;
use crate::reset_cycles::ResetCycle;
use crate::vm_builder::prepare;
use crate::vm_process::HelperLaunch;
use crate::RuntimeError;
use std::path::Path;

/// Exit code by which the helper reports a guest-requested SYSTEM_RESET.
/// The other side of this contract is the probe's `RESET_EXIT_CODE`.
pub const RESET_EXIT_CODE: i32 = 42;

/// Run the full lifecycle for one manifest. Returns the completed cycles;
/// the transcript (helper stdio is inherited) carries the boot evidence.
pub fn run_vm(
    manifest: LaunchManifest,
    launch: &HelperLaunch,
    receipt: &Path,
    max_cycles: u32,
    holder: &str,
) -> Result<Vec<ResetCycle>, RuntimeError> {
    let prepared = prepare(manifest, holder)?;
    Ok(crate::run_prepared_vm(
        &prepared,
        launch,
        receipt,
        max_cycles,
        &crate::RuntimeControl::default(),
    )?
    .cycles)
}

#[cfg(test)]
#[path = "vm_run_tests.rs"]
mod tests;
