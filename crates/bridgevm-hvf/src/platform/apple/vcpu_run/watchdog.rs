//! Shared watchdog decision; injected waiting keeps deadline races deterministic in tests.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

pub(super) fn cancel_after_timeout<T>(
    done: &AtomicBool,
    timeout_ms: u64,
    mut wait: impl FnMut(Duration),
    cancel: impl FnOnce() -> T,
) -> Option<T> {
    for _ in 0..timeout_ms.max(1) {
        if done.load(Ordering::SeqCst) {
            return None;
        }
        wait(Duration::from_millis(1));
    }
    // Completion published before this CAS wins; once timeout claims cancellation,
    // the vCPU may return before the provider call. Join before any re-entry.
    done.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .ok()
        .map(|_| cancel())
}

#[cfg(test)]
mod tests;
