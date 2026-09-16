//! Prepared media remains borrowed through every helper's observed teardown.

#[path = "controlled_generation.rs"]
mod generation;
use crate::{
    decide_restart, flush_and_write_receipt, HelperLaunch, PreparedVm, ResetCycle, RestartDecision,
    RuntimeControl, RuntimeError, RESET_EXIT_CODE,
};
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunTermination {
    Completed,
    Cancelled,
}

#[derive(Debug)]
pub struct ControlledRun {
    pub cycles: Vec<ResetCycle>,
    pub termination: RunTermination,
}

pub fn run_prepared_vm(
    prepared: &PreparedVm,
    launch: &HelperLaunch,
    receipt: &Path,
    max_cycles: u32,
    control: &RuntimeControl<'_>,
) -> Result<ControlledRun, RuntimeError> {
    let mut cycles = Vec::new();
    let cancelled = |cycles| ControlledRun {
        cycles,
        termination: RunTermination::Cancelled,
    };
    for _ in 0..max_cycles {
        if control.is_cancelled() {
            return Ok(cancelled(cycles));
        }
        let tag = prepared.generation().stamp();
        let (pid, exit) = generation::run(prepared, launch, tag.value(), control)?;
        if exit.cancelled || control.is_cancelled() {
            return Ok(cancelled(cycles));
        }
        let reset_requested = exit.status.code() == Some(RESET_EXIT_CODE);
        if !reset_requested && !exit.status.success() {
            return Err(RuntimeError::Io {
                context: "VM helper failed",
                source: std::io::Error::other(format!("exit status {}", exit.status)),
            });
        }
        cycles.push(ResetCycle {
            pid,
            generation: tag.value(),
        });
        if !reset_requested {
            return Ok(ControlledRun {
                cycles,
                termination: RunTermination::Completed,
            });
        }
        if control.is_cancelled() {
            return Ok(cancelled(cycles));
        }
        flush_and_write_receipt(
            &[
                Path::new(prepared.manifest().disk()),
                Path::new(prepared.manifest().uefi_vars()),
            ],
            receipt,
            tag,
        )?;
        if control.is_cancelled() {
            return Ok(cancelled(cycles));
        }
        match decide_restart(true, receipt, prepared.generation()) {
            RestartDecision::Restart => {}
            RestartDecision::Stop { reason } => {
                return Err(RuntimeError::RestartRefused { reason })
            }
        }
    }
    Ok(ControlledRun {
        cycles,
        termination: if control.is_cancelled() {
            RunTermination::Cancelled
        } else {
            RunTermination::Completed
        },
    })
}

#[cfg(test)]
#[path = "controlled_run_tests.rs"]
mod tests;
