use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::thread::{self, Thread};
use std::time::{Duration, Instant};

/// Work that may leave the Windows Venus driver waiting on shared feedback.
/// Stay active for one Windows default timer quantum and room for the next
/// renderer notification. The idle worker holds no platform lock.
#[derive(Debug)]
pub struct VenusWorkWake {
    origin: Instant,
    active_until: AtomicU64,
    waiter: OnceLock<Thread>,
}

const ACTIVE_NS: u64 = 26_000_000;

impl Default for VenusWorkWake {
    fn default() -> Self {
        Self {
            origin: Instant::now(),
            active_until: AtomicU64::new(0),
            waiter: OnceLock::new(),
        }
    }
}

impl VenusWorkWake {
    pub fn register_waiter(&self) {
        let _ = self.waiter.set(thread::current());
    }

    pub(crate) fn submitted(&self) {
        let until = self.elapsed_ns(Instant::now()).saturating_add(ACTIVE_NS);
        self.active_until.store(until, Ordering::Release);
        self.notify();
    }

    pub fn clear(&self) {
        self.active_until.store(0, Ordering::Release);
        self.notify();
    }

    pub fn remaining(&self, now: Instant) -> Option<Duration> {
        let until = self.active_until.load(Ordering::Acquire);
        until
            .checked_sub(self.elapsed_ns(now))
            .filter(|&ns| ns != 0)
            .map(Duration::from_nanos)
    }

    fn elapsed_ns(&self, time: Instant) -> u64 {
        u64::try_from(time.saturating_duration_since(self.origin).as_nanos()).unwrap_or(u64::MAX)
    }

    fn notify(&self) {
        if let Some(waiter) = self.waiter.get() {
            waiter.unpark();
        }
    }
}
