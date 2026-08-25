//! Typed outcomes: synchronous Stop invokes callbacks; Dispose returns after callbacks cease.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

pub(super) struct CoreAudioOutcomes {
    stopping: AtomicBool,
    unexpected_callback_errors: AtomicU64,
    teardown_reenqueue_refusals: AtomicU64,
    stop_errors: AtomicU64,
    dispose_errors: AtomicU64,
}

impl CoreAudioOutcomes {
    pub(super) fn new() -> Self {
        Self { stopping: AtomicBool::new(false), unexpected_callback_errors: AtomicU64::new(0),
            teardown_reenqueue_refusals: AtomicU64::new(0), stop_errors: AtomicU64::new(0),
            dispose_errors: AtomicU64::new(0) }
    }
    pub(super) fn begin_stopping(&self) { self.stopping.store(true, Ordering::SeqCst); }
    pub(super) fn record_reenqueue(&self, status: i32) {
        if status == 0 { return; }
        let counter = if self.stopping.load(Ordering::SeqCst) {
            &self.teardown_reenqueue_refusals
        } else { &self.unexpected_callback_errors };
        counter.fetch_add(1, Ordering::SeqCst);
    }
    pub(super) fn record_stop(&self, status: i32) {
        if status != 0 { self.stop_errors.fetch_add(1, Ordering::SeqCst); }
    }
    pub(super) fn record_dispose(&self, status: i32) {
        if status != 0 { self.dispose_errors.fetch_add(1, Ordering::SeqCst); }
    }
    pub(super) fn snapshot(&self) -> (u64, u64, u64, u64) {
        (self.unexpected_callback_errors.load(Ordering::SeqCst),
            self.teardown_reenqueue_refusals.load(Ordering::SeqCst),
            self.stop_errors.load(Ordering::SeqCst), self.dispose_errors.load(Ordering::SeqCst))
    }
}

#[cfg(test)]
#[test]
fn callback_failures_are_classified_by_the_stop_boundary() {
    let outcomes = CoreAudioOutcomes::new();
    outcomes.record_reenqueue(-1); outcomes.record_reenqueue(0);
    outcomes.begin_stopping(); outcomes.record_reenqueue(-2);
    outcomes.record_stop(-3); outcomes.record_dispose(-4);
    assert_eq!(outcomes.snapshot(), (1, 1, 1, 1));
}
