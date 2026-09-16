//! Signal-aware owner of the complete typed helper and swtpm lifetime.

pub(super) use super::{default_firmware_code, default_receipt_path};
use anyhow::{bail, Context, Result};
use bridgevm_hvf_runtime::{prepare, LaunchManifest, RunTermination, RuntimeControl};
use std::io::Read;
use std::path::Path;
#[path = "launch_spec_execution.rs"]
pub(super) mod execution;
#[path = "launch_spec_options.rs"]
mod options;
#[path = "cancellation_signals.rs"]
pub(super) mod signals;

pub(super) fn run(manifest: LaunchManifest, args: &crate::Args, helper: &Path) -> Result<()> {
    // No child exists while legacy stdin key material may wait for EOF.
    // Default TERM/INT behavior still interrupts this pre-owner read.
    let state_key = if args.typed.helper_vtpm_state.is_some() && args.typed.helper_vtpm_key_stdin {
        let mut key = Vec::new();
        std::io::stdin()
            .take(33)
            .read_to_end(&mut key)
            .context("read vTPM state key")?;
        if key.len() != 32 {
            bail!("vTPM state key must be exactly 32 bytes");
        }
        Some(key)
    } else {
        None
    };
    let _signals =
        signals::CancellationSignals::install().context("install runtime cancellation")?;
    let on_unconfirmed = |value: bridgevm_hvf_runtime::CleanupUnconfirmed| {
        eprintln!(
            "owned_cleanup=unconfirmed role={:?} pid={} reason={} leases=retained",
            value.role, value.pid, value.reason
        );
    };
    let control = RuntimeControl::new(&signals::requested, &on_unconfirmed);
    let prepared = prepare(manifest, "hvf-runner --launch-spec")
        .map_err(|error| anyhow::anyhow!("launch refused: {error}"))?;
    let (result, cleanup) = execution::execute(&prepared, args, helper, state_key, &control);
    drop(prepared);
    let run = match (result, cleanup) {
        (Err(primary), Err(cleanup)) => {
            return Err(primary.context(format!("swtpm cleanup also failed: {cleanup}")))
        }
        (Err(primary), _) => return Err(primary),
        (_, Err(cleanup)) => bail!("swtpm cleanup failed after child reap: {cleanup}"),
        (Ok(value), Ok(_)) => value,
    };
    if run.termination == RunTermination::Cancelled || control.is_cancelled() {
        bail!("runtime cancelled; owned helper/swtpm cleanup observed; guest shutdown unproven");
    }
    for cycle in &run.cycles {
        println!(
            "vm cycle: generation={} helper_pid={}",
            cycle.generation, cycle.pid
        );
    }
    println!(
        "vm run ended: {} generation(s); owned children reaped; guest shutdown unproven",
        run.cycles.len()
    );
    Ok(())
}
