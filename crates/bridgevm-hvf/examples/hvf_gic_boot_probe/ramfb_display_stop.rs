//! The stop request the ramfb display exporter's thread sleeps on between
//! ticks, split out of ramfb_display_thread.rs unchanged.

use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};
use std::time::Instant;

#[derive(Default)]
pub(super) struct StopSignal {
    stopped: Mutex<bool>,
    wake: Condvar,
}

impl StopSignal {
    fn flag(&self) -> MutexGuard<'_, bool> {
        self.stopped.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(super) fn stop(&self) {
        *self.flag() = true;
        self.wake.notify_all();
    }

    /// Sleep until `deadline`; true as soon as a stop is requested.
    pub(super) fn wait_until(&self, deadline: Instant) -> bool {
        let mut stopped = self.flag();
        while !*stopped {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return false;
            }
            stopped = self
                .wake
                .wait_timeout(stopped, left)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
        true
    }
}
