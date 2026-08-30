//! Platform-lock-free vblank deadline state and host-thread notification.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::OnceLock;
use std::thread::{self, Thread};
use std::time::{Duration, Instant};

/// The device publishes a parked vsync NOP and its deadline while holding the
/// platform mutex. The host thread reads only atomics and sleeps on its own
/// park token, so it never contends for the platform lock and never polls while
/// no NOP is parked.
#[derive(Debug)]
pub struct VblankWakeState {
    base: Instant,
    parked: AtomicBool,
    deadline_ns: AtomicU64,
    waiter: OnceLock<Thread>,
}

impl VblankWakeState {
    pub fn new() -> Self {
        Self {
            base: Instant::now(),
            parked: AtomicBool::new(false),
            deadline_ns: AtomicU64::new(0),
            waiter: OnceLock::new(),
        }
    }

    pub(crate) fn publish(&self, parked: bool, deadline: Option<Instant>) {
        let deadline_ns = deadline
            .map(|value| {
                u64::try_from(value.saturating_duration_since(self.base).as_nanos())
                    .unwrap_or(u64::MAX)
            })
            .unwrap_or(0);
        self.deadline_ns.store(deadline_ns, Ordering::SeqCst);
        self.parked.store(parked, Ordering::SeqCst);
        self.notify_waiter();
    }

    pub fn register_current_thread(&self) {
        let _ = self.waiter.set(thread::current());
    }

    fn notify_waiter(&self) {
        if let Some(waiter) = self.waiter.get() {
            waiter.unpark();
        }
    }

    pub fn parked(&self) -> bool {
        self.parked.load(Ordering::SeqCst)
    }

    /// Time remaining until the parked NOP is due, `Duration::ZERO` when due
    /// now, or `None` when nothing is parked.
    pub fn time_to_deadline(&self, now: Instant) -> Option<Duration> {
        if !self.parked() {
            return None;
        }
        let deadline_ns = self.deadline_ns.load(Ordering::SeqCst);
        let now_ns =
            u64::try_from(now.saturating_duration_since(self.base).as_nanos()).unwrap_or(u64::MAX);
        Some(Duration::from_nanos(deadline_ns.saturating_sub(now_ns)))
    }
}

impl Default for VblankWakeState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[test]
fn publication_before_park_leaves_a_wake_token() {
    let state = std::sync::Arc::new(VblankWakeState::new());
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let (awake_tx, awake_rx) = std::sync::mpsc::channel();
    let proceed = std::sync::Arc::new(AtomicBool::new(false));
    let waiter_state = std::sync::Arc::clone(&state);
    let waiter_proceed = std::sync::Arc::clone(&proceed);
    let waiter = std::thread::spawn(move || {
        waiter_state.register_current_thread();
        ready_tx.send(()).unwrap();
        while !waiter_proceed.load(Ordering::Acquire) {
            std::thread::yield_now();
        }
        std::thread::park();
        awake_tx.send(()).unwrap();
    });
    ready_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    state.publish(true, Some(Instant::now()));
    proceed.store(true, Ordering::Release);
    awake_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    waiter.join().unwrap();
}
