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
use crate::reset_cycles::{supervise_reset_cycles, HelperExit, ResetCycle};
use crate::vm_builder::prepare;
use crate::vm_process::{spawn_helper, HelperLaunch};
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
    let disk = prepared.manifest().disk().to_string();
    let vars = prepared.manifest().uefi_vars().to_string();
    let cycles = supervise_reset_cycles(
        |generation| {
            let mut child =
                spawn_helper(prepared.manifest(), launch, generation).map_err(|source| {
                    RuntimeError::Io {
                        context: "spawn VM helper",
                        source,
                    }
                })?;
            let pid = child.id();
            let status = child.wait().map_err(|source| RuntimeError::Io {
                context: "wait for VM helper",
                source,
            })?;
            let reset_requested = status.code() == Some(RESET_EXIT_CODE);
            if !reset_requested && !status.success() {
                return Err(RuntimeError::Io {
                    context: "VM helper failed",
                    source: std::io::Error::other(format!("exit status {status}")),
                });
            }
            Ok(HelperExit {
                pid,
                reset_requested,
            })
        },
        &[Path::new(&disk), Path::new(&vars)],
        receipt,
        max_cycles,
    )?;
    Ok(cycles)
}

#[cfg(test)]
#[path = "vm_run_tests.rs"]
mod tests;
