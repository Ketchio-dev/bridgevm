//! The reset generation: which boot an event belongs to.
//!
//! PLAN.md R1: stale wake/IRQ/MSI/vblank/agent events must carry a
//! `ResetGeneration` and be discarded on mismatch. The probe learned this
//! the hard way -- a cancel requested for the previous boot counted as an
//! answer for the new one -- and its fix lives in example code. This is the
//! product-side version: a monotonic counter plus a tag, so the check is a
//! comparison, not a convention.

use std::sync::atomic::{AtomicU64, Ordering};

/// Monotonic generation counter for one VM's lifetime.
#[derive(Debug, Default)]
pub struct ResetGeneration {
    current: AtomicU64,
}

/// The generation an event was created under. Deliberately not `Default`:
/// an event without a stamp should not typecheck, because an unstamped
/// event is exactly the stale-delivery bug this exists to remove.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GenerationTag(u64);

impl ResetGeneration {
    pub fn new() -> Self {
        Self::default()
    }

    /// Tag an event with the boot it belongs to.
    pub fn stamp(&self) -> GenerationTag {
        GenerationTag(self.current.load(Ordering::SeqCst))
    }

    /// A reset happened: everything stamped before this is now stale.
    /// Returns the new generation's tag.
    pub fn advance(&self) -> GenerationTag {
        GenerationTag(self.current.fetch_add(1, Ordering::SeqCst) + 1)
    }

    /// Whether an event stamped `tag` belongs to the current boot.
    pub fn is_current(&self, tag: GenerationTag) -> bool {
        tag.0 == self.current.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
#[path = "reset_generation_tests.rs"]
mod tests;
