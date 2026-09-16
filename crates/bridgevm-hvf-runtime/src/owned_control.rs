//! Run-scoped cancellation and explicit failure to observe owned cleanup.

pub use crate::controlled_run::{run_prepared_vm, ControlledRun, RunTermination};
pub use crate::vtpm::start_swtpm_controlled;
use std::cell::Cell;

#[cfg(test)]
#[path = "owned_process_test_support.rs"]
pub(crate) mod test_support;

#[path = "owned_lifecycle.rs"]
mod lifecycle;
pub use lifecycle::*;

pub struct RuntimeControl<'a> {
    requested: &'a dyn Fn() -> bool,
    unconfirmed: &'a dyn Fn(CleanupUnconfirmed),
    latched: Cell<bool>,
    observer: Option<&'a dyn Fn(RuntimeLifecycleEvent)>,
}

impl<'a> RuntimeControl<'a> {
    /// Once observed, cancellation stays latched for this whole run.
    pub fn new(
        requested: &'a dyn Fn() -> bool,
        unconfirmed: &'a dyn Fn(CleanupUnconfirmed),
    ) -> Self {
        Self {
            requested,
            unconfirmed,
            latched: Cell::new(false),
            observer: None,
        }
    }
    pub fn with_observer(
        requested: &'a dyn Fn() -> bool,
        unconfirmed: &'a dyn Fn(CleanupUnconfirmed),
        observer: &'a dyn Fn(RuntimeLifecycleEvent),
    ) -> Self {
        Self {
            observer: Some(observer),
            ..Self::new(requested, unconfirmed)
        }
    }
    pub(crate) fn observe(&self, event: RuntimeLifecycleEvent) {
        if let Some(observer) = self.observer {
            observer(event);
        }
    }
    pub fn is_cancelled(&self) -> bool {
        if !self.latched.get() && (self.requested)() {
            self.latched.set(true);
        }
        self.latched.get()
    }
    pub(crate) fn report(&self, value: CleanupUnconfirmed) {
        (self.unconfirmed)(value)
    }
}
