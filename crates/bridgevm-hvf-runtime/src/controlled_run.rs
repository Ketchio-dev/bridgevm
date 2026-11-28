//! Prepared media remains borrowed through every helper's observed teardown.

#[path = "controlled_generation.rs"]
mod generation;
use crate::{
    decide_restart, flush_and_write_receipt, HelperLaunch, PreparedVm, RestartDecision,
    RuntimeControl, RuntimeError,
};
use bridgevm_hvf::snapshot_pair::managed::LockedPair;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;

/// Exit code by which the helper reports a guest-requested SYSTEM_RESET.
/// The other side of this contract is the probe's `RESET_EXIT_CODE`.
const RESET_EXIT_CODE: i32 = 42;

/// One completed helper generation.
#[derive(Debug, PartialEq, Eq)]
pub struct ResetCycle {
    pub pid: u32,
    pub generation: u64,
}

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
        let reset_requested = exit.status.code() == Some(RESET_EXIT_CODE);
        if exit.cancelled || !(reset_requested || exit.status.success()) {
            sync_after_unclean_exit(prepared, launch);
        }
        if exit.cancelled || control.is_cancelled() {
            return Ok(cancelled(cycles));
        }
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
        match decide_restart(receipt, prepared.generation()) {
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

/// A helper that did not end by itself through its own stop save (it was
/// cancelled, failed or was killed) may have acknowledged a guest FLUSH after
/// F_BARRIERFSYNC and died before the F_FULLFSYNC it owed. `sync_all` is
/// F_FULLFSYNC on macOS. The helper used the pair's selected generation,
/// which after a snapshot restore is not the logical path; the writer leases
/// `prepare` holds keep a restore from changing it here. A failure is
/// reported as the helper reports a failed final save.
fn sync_after_unclean_exit(prepared: &PreparedVm, launch: &HelperLaunch) {
    let manifest = prepared.manifest();
    let logical = [Path::new(manifest.disk()), Path::new(manifest.uefi_vars())];
    // The pair is held through the sync: its paths are valid only while it lives.
    let selected =
        LockedPair::open(logical[0], logical[1]).and_then(|pair| Ok((pair.paths()?, pair)));
    let paths = match &selected {
        Ok(((disk, vars), _pair)) => [disk.as_path(), vars.as_path()],
        Err(error) => {
            // F_FULLFSYNC flushes the drive's cache, which settles what the
            // helper's barrier syncs left there on that drive.
            eprintln!("selected VM media not resolved ({error}); syncing the logical files");
            logical
        }
    };
    let mut failed = false;
    for path in paths {
        if let Err(error) = File::open(path).and_then(|file| file.sync_all()) {
            let path = path.display();
            eprintln!("flush storage after the VM helper stopped: {path}: {error}");
            failed = true;
        }
    }
    if failed {
        report_final_save_failed(launch);
    }
}

/// The helper's own line for a failed final save, in the run.log the app
/// reads (vm_process.rs), on a line of its own even after a killed helper's
/// partial last line.
fn report_final_save_failed(launch: &HelperLaunch) {
    const LINE: &str = "BVSTORAGE final-save-failed";
    let Some(surfaces) = &launch.surfaces else {
        eprintln!("{LINE}");
        return;
    };
    let written = OpenOptions::new()
        .append(true)
        .open(surfaces.evidence_dir.join("run.log"))
        .and_then(|mut log| writeln!(log, "\n{LINE}"));
    if let Err(error) = written {
        eprintln!("{LINE} (run.log: {error})");
    }
}

#[cfg(test)]
#[path = "controlled_run_tests.rs"]
mod tests;
