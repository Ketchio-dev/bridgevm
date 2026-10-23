//! Fatal stop reasons for a PSCI SYSTEM_RESET that cannot reset the machine.

use crate::{HvReturn, VirtPlatform};
use std::sync::Mutex;

/// Reset the platform for an in-process reboot unless the GIC reset already
/// failed, and return the fatal stop reason when either reset fails. The
/// reboot arm then sets `fatal_reset_error` and stops through the final
/// report instead of booting the next generation. A TPM backend that cannot
/// be power-cycled is such a failure: the platform is left as it was, and no
/// guest boots again with a TPM whose state is unknown.
pub(crate) fn system_reset_failure(
    gic_reset_status: HvReturn,
    reset_platform: bool,
    platform: &Mutex<VirtPlatform>,
) -> Option<String> {
    if gic_reset_status != 0 {
        return Some(format!(
            "hv_gic_reset failed during PSCI SYSTEM_RESET: {gic_reset_status:#x}"
        ));
    }
    if !reset_platform {
        return None;
    }
    let mut platform = platform.lock().expect("platform mutex");
    let error = platform.reset().err()?;
    Some(format!(
        "TPM backend reset failed during PSCI SYSTEM_RESET: {error}"
    ))
}

#[cfg(test)]
#[path = "system_reset_failure_tests.rs"]
mod tests;
