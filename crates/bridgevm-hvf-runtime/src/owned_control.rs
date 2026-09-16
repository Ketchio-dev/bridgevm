//! Run-scoped cancellation and explicit failure to observe owned cleanup.

pub use crate::controlled_run::{run_prepared_vm, ControlledRun, RunTermination};
pub use crate::vtpm::start_swtpm_controlled;
use std::cell::Cell;

#[cfg(test)]
#[path = "owned_process_test_support.rs"]
pub(crate) mod test_support;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChildRole {
    Helper,
    Swtpm,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CleanupUnconfirmed {
    pub role: ChildRole,
    pub pid: u32,
    pub reason: &'static str,
}

pub struct RuntimeControl<'a> {
    requested: &'a dyn Fn() -> bool,
    unconfirmed: &'a dyn Fn(CleanupUnconfirmed),
    latched: Cell<bool>,
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

impl Default for RuntimeControl<'static> {
    fn default() -> Self {
        Self::new(&|| false, &|value| {
            eprintln!(
                "owned cleanup unconfirmed: {:?} pid={} {}; retaining ownership",
                value.role, value.pid, value.reason
            );
        })
    }
}
