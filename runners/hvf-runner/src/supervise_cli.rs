//! `--supervise CMD`: run a real helper process under the reset-cycle loop.
//!
//! The A15 harness needs what the unit tests cannot give: real PIDs from
//! real processes. This flag wires `supervise_reset_cycles` to a spawned
//! helper (argv, no shell) and prints one line per cycle -- pid and
//! generation -- so a soak can assert "new helper PID, increasing reset
//! generation" from the transcript alone.
//!
//! The helper signals "the guest asked for SYSTEM_RESET" with exit code 42
//! (any other success is a clean shutdown, any failure stops the loop).
//! The probe does not speak this protocol yet; until it does, the flag is
//! exercised with stub helpers and proves the supervisor side end to end.

use anyhow::{Context, Result};
use bridgevm_hvf_runtime::{supervise_reset_cycles, HelperExit, RuntimeError};
use std::path::Path;
use std::process::Command;

/// Exit code by which a helper reports a guest-requested SYSTEM_RESET.
pub(crate) const RESET_REQUESTED_EXIT: i32 = 42;

pub(crate) fn run_supervise(argv: &[String], receipt: &str, max_cycles: u32) -> Result<()> {
    let (program, args) = argv.split_first().context("--supervise needs a command")?;
    let cycles = supervise_reset_cycles(
        |generation| {
            let mut child = Command::new(program)
                .args(args)
                .env("BRIDGEVM_RESET_GENERATION", generation.to_string())
                .spawn()
                .map_err(|source| RuntimeError::Io {
                    context: "spawn supervised helper",
                    source,
                })?;
            let pid = child.id();
            let status = child.wait().map_err(|source| RuntimeError::Io {
                context: "wait for supervised helper",
                source,
            })?;
            let reset_requested = status.code() == Some(RESET_REQUESTED_EXIT);
            if !reset_requested && !status.success() {
                return Err(RuntimeError::Io {
                    context: "supervised helper failed",
                    source: std::io::Error::other(format!("exit status {status}")),
                });
            }
            Ok(HelperExit {
                pid,
                reset_requested,
            })
        },
        &[],
        Path::new(receipt),
        max_cycles,
    )
    .map_err(|error| anyhow::anyhow!("{error}"))?;
    for cycle in &cycles {
        println!(
            "supervised cycle: generation={} helper_pid={}",
            cycle.generation, cycle.pid
        );
    }
    println!("supervised cycles complete: {}", cycles.len());
    Ok(())
}

#[cfg(test)]
#[path = "supervise_cli_tests.rs"]
mod tests;
