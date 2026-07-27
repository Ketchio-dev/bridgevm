//! Opt-in kill mode for the boot-progress watchdog.
//!
//! Kept apart from the sampler so the default path -- observe and report --
//! stays readable without the FFI teardown next to it.

use crate::{hv_vcpus_exit, HvVcpuT};

/// Opt-in: end the run once a stall is confirmed instead of only reporting it.
///
/// Off by default, deliberately. A boot that is merely slow must not be turned
/// into a failure, and the floor is calibrated against averages, not proofs.
/// Gate scripts turn this on because for them a confirmed stall is already a
/// lost run -- waiting out the remaining budget only costs wall-clock time
/// (observed: ~40 min per run, versus ~5 min with this on).
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
#[derive(Clone, Copy)]
pub(crate) struct BootProgressKill {
    pub(crate) vcpu: HvVcpuT,
}

impl BootProgressKill {
    pub(crate) fn fire(self) {
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
    boot_progress_kill_requested().then_some(BootProgressKill { vcpu })
}

#[cfg(test)]
mod boot_progress_kill_tests {
    use super::*;

    /// Guards the default. Kill mode changes a reported stall into an ended
    /// run, so anything other than an explicit opt-in must leave it off.
    #[test]
    fn kill_is_off_unless_explicitly_requested() {
        for value in ["", "0", "true", "yes", "2", "01"] {
            assert!(
                !kill_requested_from(Some(value)),
                "{value:?} must not enable kill mode"
            );
        }
        assert!(!kill_requested_from(None));
        assert!(kill_requested_from(Some("1")));
    }
}
