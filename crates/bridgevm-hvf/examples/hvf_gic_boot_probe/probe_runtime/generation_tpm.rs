//! This helper generation's TPM backend, prepared before the VM exists.

use crate::{SwtpmUnixBackend, Tpm2Backend};
use std::path::Path;

/// Connect the swtpm backend for this helper generation.
///
/// A guest that boots from reset gets a power-cycled TPM (CMD_INIT) whenever
/// a control socket is configured, because swtpm keeps the previous
/// generation's PCRs and platform authorization across reconnects. A
/// checkpoint restore resumes a running guest, so it connects without
/// CMD_INIT. Checkpoints carry no TPM state, so the resumed guest sees
/// whatever swtpm holds. On failure this returns the stop line; the caller
/// exits before creating the VM, so no vCPU runs.
pub(crate) fn generation_tpm(
    data: Option<&Path>,
    control: Option<&Path>,
    restoring: bool,
) -> Result<Option<Box<dyn Tpm2Backend>>, String> {
    let Some(data) = data else {
        return Ok(None);
    };
    match control {
        Some(control) => println!(
            "TPM2 TIS backend: swtpm data socket {} control socket {}",
            data.display(),
            control.display()
        ),
        None => println!("TPM2 TIS backend: swtpm data socket {}", data.display()),
    }
    let backend = if restoring {
        println!("TPM2 TIS backend: checkpoint restore, no power cycle");
        SwtpmUnixBackend::connect_with_control(data, control)
    } else {
        SwtpmUnixBackend::connect_for_boot(data, control)
    };
    let backend = backend.map_err(|error| {
        format!("stop: TPM backend setup failed before the first guest instruction: {error}")
    })?;
    if !restoring && control.is_some() {
        println!("TPM2 TIS backend: power-cycled with CMD_INIT before boot");
    }
    Ok(Some(Box::new(backend)))
}

#[cfg(test)]
#[path = "generation_tpm_tests.rs"]
mod tests;
