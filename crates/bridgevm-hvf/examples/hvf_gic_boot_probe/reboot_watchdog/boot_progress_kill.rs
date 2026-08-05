//! Opt-in kill mode for the boot-progress watchdog.
//!
//! Kept apart from the sampler so the default path -- observe and report --
//! stays readable without the FFI teardown next to it.

use crate::{hv_vcpus_exit, HvVcpuT};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Opt-in: end the run once a stall is confirmed instead of only reporting it.
///
/// Off by default, deliberately. A boot that is merely slow must not be turned
/// into a failure, and the floor is calibrated against averages, not proofs.
/// Gate scripts turn this on because for them a confirmed stall is already a
/// lost run -- waiting out the remaining budget only costs wall-clock time.
pub(crate) fn boot_progress_kill_requested() -> bool {
    kill_requested_from(std::env::var("BRIDGEVM_BOOT_PROGRESS_KILL").ok().as_deref())
}

/// Split from the env lookup so the policy is testable without touching
/// process-wide state.
fn kill_requested_from(value: Option<&str>) -> bool {
    value == Some("1")
}

/// What a confirmed stall does to the run when kill mode is on.
///
/// Breaks CPU0 out of `hv_vcpu_run` the same way the deadline watchdog does.
///
/// It deliberately does not touch the deadline watchdog's `watchdog_fired`
/// flag: that one is rebuilt on every reboot generation, while this watchdog
/// lives for the whole run, so borrowing it would tie a run-scoped signal to a
/// generation-scoped one. The stall verdict is already printed before this
/// fires, so the run log says why it ended.
///
/// It carries its own run-scoped flag instead. Waking the vCPU is not the same
/// as stopping the run: an exit with no flag set is treated as a surplus
/// cancel and the loop continues, which is correct for automation wakes and
/// was wrong here. Measured before this flag existed: a boot that logged
/// "ending run (kill mode)" was still running 8 minutes later, at 48 minutes
/// total against a 40-minute deadline.
#[derive(Clone)]
pub(crate) struct BootProgressKill {
    pub(crate) vcpu: HvVcpuT,
    pub(crate) fired: Arc<AtomicBool>,
}

impl BootProgressKill {
    pub(crate) fn fire(self) {
        // Set before the wake, so the vCPU thread cannot observe the exit
        // without the reason for it.
        self.fired.store(true, Ordering::SeqCst);
        let v = self.vcpu;
        // SAFETY: Category 8 - FFI boundary. `vcpu` is a live HVF vCPU handle
        // owned by the probe until shutdown, and `&v` points to one
        // initialized handle for the duration of this call.
        unsafe {
            hv_vcpus_exit(&v, 1);
        }
    }
}

/// Kill handle only when the run asked for it, so the default stays
/// observation-only.
pub(crate) fn boot_progress_kill_for(vcpu: HvVcpuT) -> Option<BootProgressKill> {
    boot_progress_kill_requested().then_some(BootProgressKill {
        vcpu,
        fired: Arc::new(AtomicBool::new(false)),
    })
}

#[cfg(test)]
#[path = "boot_progress_kill_tests.rs"]
mod boot_progress_kill_tests;
