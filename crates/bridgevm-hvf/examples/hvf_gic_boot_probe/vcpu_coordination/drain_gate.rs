//! Optionally coalesce secondary-vCPU drains while preserving asynchronous wake drains.
//!
//! CPU0 deliberately does not use this gate: host workers can complete async
//! device work between its post-MMIO drain and the following pre-run drain.

use crate::*;

pub(crate) struct PreRunDrainGate {
    enabled: bool,
    secondary_pending: AtomicBool,
}

impl PreRunDrainGate {
    pub(crate) fn from_env() -> Self {
        Self::new(env_flag_default("BRIDGEVM_DRAIN_GATE", false))
    }

    pub(crate) const fn new(enabled: bool) -> Self {
        Self {
            enabled,
            secondary_pending: AtomicBool::new(true),
        }
    }

    pub(crate) fn should_drain_secondary_pre_run(&self) -> bool {
        if !self.enabled {
            return true;
        }
        self.secondary_pending.swap(false, Ordering::AcqRel)
    }

    pub(crate) fn mark_secondary_pending(&self) {
        if self.enabled {
            self.secondary_pending.store(true, Ordering::Release);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secondary_drains_initially_and_after_pending_marker() {
        let gate = PreRunDrainGate::new(true);

        assert!(gate.should_drain_secondary_pre_run());
        assert!(!gate.should_drain_secondary_pre_run());
        gate.mark_secondary_pending();
        assert!(gate.should_drain_secondary_pre_run());
        assert!(!gate.should_drain_secondary_pre_run());
    }

    #[test]
    fn disabled_gate_preserves_every_drain() {
        let gate = PreRunDrainGate::new(false);

        assert!(gate.should_drain_secondary_pre_run());
        assert!(gate.should_drain_secondary_pre_run());
    }
}
