//! Avoid redundant full-device drains while preserving asynchronous wake drains.

use crate::*;

pub(crate) struct PreRunDrainGate {
    enabled: bool,
    secondary_pending: AtomicBool,
    primary_skip_once: AtomicBool,
}

impl PreRunDrainGate {
    pub(crate) fn from_env() -> Self {
        Self::new(env_flag_default("BRIDGEVM_DRAIN_GATE", true))
    }

    pub(crate) const fn new(enabled: bool) -> Self {
        Self {
            enabled,
            secondary_pending: AtomicBool::new(true),
            primary_skip_once: AtomicBool::new(false),
        }
    }

    /// Skip only the next CPU0 pre-run drain after the same thread completed
    /// the full post-MMIO drain. No guest instruction can execute between the
    /// two sites. The skip consumes itself, so a timer, host wake, cancellation,
    /// or any non-MMIO exit is followed by the ordinary pre-run drain.
    pub(crate) fn should_drain_primary_pre_run(&self) -> bool {
        !self.enabled || !self.primary_skip_once.swap(false, Ordering::AcqRel)
    }

    pub(crate) fn note_primary_post_mmio_drain(&self) {
        if self.enabled {
            self.primary_skip_once.store(true, Ordering::Release);
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
    fn primary_skips_exactly_one_drain_after_post_mmio_drain() {
        let gate = PreRunDrainGate::new(true);

        assert!(gate.should_drain_primary_pre_run());
        gate.note_primary_post_mmio_drain();
        assert!(!gate.should_drain_primary_pre_run());
        assert!(gate.should_drain_primary_pre_run());
    }

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

        gate.note_primary_post_mmio_drain();
        assert!(gate.should_drain_primary_pre_run());
        assert!(gate.should_drain_primary_pre_run());
        assert!(gate.should_drain_secondary_pre_run());
        assert!(gate.should_drain_secondary_pre_run());
    }
}
