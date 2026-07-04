use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Instant;

use crate::EXIT_CANCELED;

pub(crate) struct SetupInputHostWake {
    fired: Arc<AtomicBool>,
    armed_deadlines: Vec<Instant>,
}

impl SetupInputHostWake {
    pub(crate) fn new() -> Self {
        Self {
            fired: Arc::new(AtomicBool::new(false)),
            armed_deadlines: Vec::new(),
        }
    }

    pub(crate) fn arm<F>(&mut self, deadline: Instant, wake: F) -> bool
    where
        F: FnOnce() + Send + 'static,
    {
        if self.armed_deadlines.contains(&deadline) {
            return false;
        }
        self.armed_deadlines.push(deadline);
        let delay = deadline.saturating_duration_since(Instant::now());
        let fired = Arc::clone(&self.fired);
        std::thread::spawn(move || {
            std::thread::sleep(delay);
            fired.store(true, Ordering::SeqCst);
            wake();
        });
        true
    }

    pub(crate) fn canceled_by_host_wake(
        &self,
        exit_reason: u32,
        watchdog_fired: &AtomicBool,
    ) -> bool {
        exit_reason == EXIT_CANCELED
            && self.fired.swap(false, Ordering::SeqCst)
            && !watchdog_fired.load(Ordering::SeqCst)
    }

    #[cfg(test)]
    pub(crate) fn fired_for_test() -> Self {
        let wake = Self::new();
        wake.fired.store(true, Ordering::SeqCst);
        wake
    }
}
