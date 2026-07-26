//! Timer-driven fence-poll stall watchdog.
//!
//! `PollStallDetector` (see `poll_stall.rs`) can only judge a gap *when the
//! next poll happens*. That makes it structurally blind to the failure this
//! stack actually suffers: the guest blocks on its first Venus ring
//! round-trip, so no vCPU exit occurs, so `poll_fences` is never called
//! again, so nothing is ever observed. The run ends stalled having emitted
//! zero records -- which is exactly what three 600 s runs showed.
//!
//! This watchdog is sampled by an independent host thread, so it fires on
//! elapsed time with **zero further polls**. That is the whole point; the
//! passive detector cannot do it at any threshold.
//!
//! Observe-only: it records and logs. It never retires a fence, changes poll
//! cadence, or touches submit acceptance. Turning the observation into a
//! recovery kick is a separate, later change.

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Emitted at most this many times per process, so a permanently stalled run
/// cannot flood the log.
pub(crate) const MAX_WATCHDOG_RECORDS: u32 = 16;

/// No poll for this long, while armed, is a stall.
pub(crate) const DEFAULT_WATCHDOG_THRESHOLD: Duration = Duration::from_secs(5);

/// How often the sampling thread wakes. Well below the threshold so the
/// reported age is close to the real one.
pub(crate) const WATCHDOG_SAMPLE_INTERVAL: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PollWatchdogRecord {
    /// How long the host has gone without polling.
    pub(crate) age: Duration,
    /// Polls completed before the stall began.
    pub(crate) polls: u64,
    /// Live contexts when the stall was noticed.
    pub(crate) contexts: usize,
    /// Fences the host still expects to retire.
    pub(crate) outstanding_fences: usize,
}

impl PollWatchdogRecord {
    /// One-line, grep-able. `polls=0` means the host never polled at all
    /// (start-up starvation); a nonzero `polls` with outstanding fences means
    /// polling stopped while work was still in flight -- the deadlock shape.
    pub(crate) fn format(&self, label: &str) -> String {
        format!(
            "{label}: fence-poll watchdog age_ms={} polls={} contexts={} outstanding_fences={} \
             suspect={}",
            self.age.as_millis(),
            self.polls,
            self.contexts,
            self.outstanding_fences,
            if self.polls == 0 {
                "host-never-polled"
            } else if self.outstanding_fences == 0 {
                "idle-no-outstanding-fence"
            } else {
                "poll-stopped-with-fences-outstanding"
            }
        )
    }
}

/// Shared, lock-free watchdog state.
///
/// Mirrors `VblankWakeState`'s pattern (atomics shared with a host thread)
/// rather than introducing a second concurrency style.
#[derive(Debug)]
pub(crate) struct PollWatchdog {
    /// Base for all timestamps; `Instant` is not atomically storable.
    base: Instant,
    /// Micros since `base` at the last observed poll.
    last_poll_micros: AtomicU64,
    /// True once a poll has been observed at least once.
    seen_poll: AtomicBool,
    /// Set when the watchdog should be sampled at all.
    armed: AtomicBool,
    polls: AtomicU64,
    contexts: AtomicUsize,
    outstanding_fences: AtomicUsize,
    /// Prevents re-reporting the same continuous stall every sample.
    stall_reported: AtomicBool,
    records_emitted: AtomicU64,
}

impl PollWatchdog {
    pub(crate) fn new() -> Self {
        Self {
            base: Instant::now(),
            last_poll_micros: AtomicU64::new(0),
            seen_poll: AtomicBool::new(false),
            armed: AtomicBool::new(false),
            polls: AtomicU64::new(0),
            contexts: AtomicUsize::new(0),
            outstanding_fences: AtomicUsize::new(0),
            stall_reported: AtomicBool::new(false),
            records_emitted: AtomicU64::new(0),
        }
    }

    /// Begin sampling. Called once the renderer is live; before this, a
    /// quiet host is normal rather than a stall.
    pub(crate) fn arm(&self) {
        self.mark_poll_at(self.now_micros());
        self.armed.store(true, Ordering::Release);
    }

    /// Stop sampling (teardown), so shutdown quiet is never reported.
    pub(crate) fn disarm(&self) {
        self.armed.store(false, Ordering::Release);
    }

    pub(crate) fn is_armed(&self) -> bool {
        self.armed.load(Ordering::Acquire)
    }

    fn now_micros(&self) -> u64 {
        self.base.elapsed().as_micros().min(u128::from(u64::MAX)) as u64
    }

    fn mark_poll_at(&self, micros: u64) {
        self.last_poll_micros.store(micros, Ordering::Release);
        self.seen_poll.store(true, Ordering::Release);
        // A fresh poll ends any stall, so the next one is reportable.
        self.stall_reported.store(false, Ordering::Release);
    }

    /// Record that `poll_fences` ran, with the state it saw.
    pub(crate) fn observe_poll(&self, contexts: usize, outstanding_fences: usize) {
        self.polls.fetch_add(1, Ordering::Relaxed);
        self.contexts.store(contexts, Ordering::Relaxed);
        self.outstanding_fences
            .store(outstanding_fences, Ordering::Relaxed);
        self.mark_poll_at(self.now_micros());
    }

    /// Sample at an explicit time. Returns a record the first time a
    /// continuous stall crosses `threshold`, then stays quiet until a poll
    /// resets it. Split out from `check` so tests need no sleeping.
    pub(crate) fn check_at(
        &self,
        now_micros: u64,
        threshold: Duration,
    ) -> Option<PollWatchdogRecord> {
        if !self.is_armed() || !self.seen_poll.load(Ordering::Acquire) {
            return None;
        }
        let last = self.last_poll_micros.load(Ordering::Acquire);
        let age = Duration::from_micros(now_micros.saturating_sub(last));
        if age < threshold {
            return None;
        }
        if self.stall_reported.swap(true, Ordering::AcqRel) {
            return None;
        }
        if self.records_emitted.load(Ordering::Relaxed) >= u64::from(MAX_WATCHDOG_RECORDS) {
            return None;
        }
        self.records_emitted.fetch_add(1, Ordering::Relaxed);
        Some(PollWatchdogRecord {
            age,
            polls: self.polls.load(Ordering::Relaxed),
            contexts: self.contexts.load(Ordering::Relaxed),
            outstanding_fences: self.outstanding_fences.load(Ordering::Relaxed),
        })
    }

    pub(crate) fn check(&self, threshold: Duration) -> Option<PollWatchdogRecord> {
        self.check_at(self.now_micros(), threshold)
    }
}

/// Start the sampling thread. Detached: it exits when the last `Arc` drops.
pub(crate) fn spawn_poll_watchdog(watchdog: Arc<PollWatchdog>, label: &'static str) {
    std::thread::Builder::new()
        .name("bv-venus-poll-watchdog".to_string())
        .spawn(move || loop {
            std::thread::sleep(WATCHDOG_SAMPLE_INTERVAL);
            // Sole owner means the backend is gone; stop sampling.
            if Arc::strong_count(&watchdog) == 1 {
                return;
            }
            if let Some(record) = watchdog.check(DEFAULT_WATCHDOG_THRESHOLD) {
                eprintln!("{}", record.format(label));
            }
        })
        .ok();
}

/// Build an armed watchdog with its sampling thread already running.
pub(crate) fn armed_poll_watchdog(label: &'static str) -> Arc<PollWatchdog> {
    let watchdog = Arc::new(PollWatchdog::new());
    watchdog.arm();
    spawn_poll_watchdog(watchdog.clone(), label);
    watchdog
}

#[cfg(test)]
mod tests {
    use super::*;

    const MICROS_PER_SEC: u64 = 1_000_000;

    fn armed() -> PollWatchdog {
        let watchdog = PollWatchdog::new();
        watchdog.arm();
        watchdog.observe_poll(1, 1);
        watchdog
    }

    /// The reason this module exists: the passive detector cannot fire
    /// without a subsequent poll, so the "guest blocked, no exits ever
    /// again" deadlock produced zero records in three 600 s runs.
    #[test]
    fn fires_with_zero_polls_after_threshold() {
        let watchdog = armed();
        let last = watchdog.last_poll_micros.load(Ordering::Acquire);
        let record = watchdog
            .check_at(last + 5 * MICROS_PER_SEC, DEFAULT_WATCHDOG_THRESHOLD)
            .expect("watchdog must fire on elapsed time with no further poll");
        assert_eq!(record.polls, 1);
        assert_eq!(record.outstanding_fences, 1);
        assert!(record.age >= Duration::from_secs(5));
    }

    /// Negative control against the module this one replaces. The passive
    /// detector is driven only from inside `poll_fences`, so in the deadlock
    /// shape -- polling stops and never resumes -- it is never called again
    /// and cannot emit anything, at any threshold. Same scenario, same
    /// elapsed time, opposite outcome from the watchdog test above.
    #[test]
    fn passive_detector_cannot_see_a_stall_with_no_further_poll() {
        use super::super::poll_stall::{PollStallDetector, DEFAULT_STALL_THRESHOLD};
        let mut detector = PollStallDetector::new();
        // One poll happens, then the guest wedges and no exit ever occurs.
        assert_eq!(
            detector.observe_poll(None, DEFAULT_STALL_THRESHOLD, 1, 1),
            None
        );
        // Time passes. There is no call site to report it: the only way to
        // get a record out of the detector is to poll again, which is
        // precisely what a stalled host cannot do.
        let watchdog = armed();
        let last = watchdog.last_poll_micros.load(Ordering::Acquire);
        assert!(
            watchdog
                .check_at(last + 600 * MICROS_PER_SEC, DEFAULT_WATCHDOG_THRESHOLD)
                .is_some(),
            "the timer-driven watchdog sees the 600s stall"
        );
    }

    #[test]
    fn quiet_below_threshold() {
        let watchdog = armed();
        let last = watchdog.last_poll_micros.load(Ordering::Acquire);
        assert_eq!(
            watchdog.check_at(last + 4 * MICROS_PER_SEC, DEFAULT_WATCHDOG_THRESHOLD),
            None
        );
    }

    #[test]
    fn one_record_per_stall_not_per_sample() {
        let watchdog = armed();
        let last = watchdog.last_poll_micros.load(Ordering::Acquire);
        assert!(watchdog
            .check_at(last + 6 * MICROS_PER_SEC, DEFAULT_WATCHDOG_THRESHOLD)
            .is_some());
        for extra in 7..20 {
            assert_eq!(
                watchdog.check_at(last + extra * MICROS_PER_SEC, DEFAULT_WATCHDOG_THRESHOLD),
                None,
                "a continuous stall must report once, not every sample"
            );
        }
    }

    #[test]
    fn rearms_after_recovery() {
        let watchdog = armed();
        let last = watchdog.last_poll_micros.load(Ordering::Acquire);
        assert!(watchdog
            .check_at(last + 6 * MICROS_PER_SEC, DEFAULT_WATCHDOG_THRESHOLD)
            .is_some());
        watchdog.observe_poll(2, 3);
        let resumed = watchdog.last_poll_micros.load(Ordering::Acquire);
        let record = watchdog
            .check_at(resumed + 6 * MICROS_PER_SEC, DEFAULT_WATCHDOG_THRESHOLD)
            .expect("a second, distinct stall must be reported");
        assert_eq!(record.polls, 2);
        assert_eq!(record.contexts, 2);
        assert_eq!(record.outstanding_fences, 3);
    }

    #[test]
    fn records_are_bounded() {
        let watchdog = armed();
        let mut emitted = 0;
        for step in 0..(u64::from(MAX_WATCHDOG_RECORDS) + 50) {
            watchdog.observe_poll(1, 1);
            let last = watchdog.last_poll_micros.load(Ordering::Acquire);
            if watchdog
                .check_at(
                    last + (6 + step) * MICROS_PER_SEC,
                    DEFAULT_WATCHDOG_THRESHOLD,
                )
                .is_some()
            {
                emitted += 1;
            }
        }
        assert_eq!(emitted, MAX_WATCHDOG_RECORDS);
    }

    #[test]
    fn unarmed_never_fires() {
        let watchdog = PollWatchdog::new();
        watchdog.observe_poll(1, 1);
        let last = watchdog.last_poll_micros.load(Ordering::Acquire);
        assert_eq!(
            watchdog.check_at(last + 60 * MICROS_PER_SEC, DEFAULT_WATCHDOG_THRESHOLD),
            None
        );
    }

    #[test]
    fn disarm_stops_reporting() {
        let watchdog = armed();
        watchdog.disarm();
        let last = watchdog.last_poll_micros.load(Ordering::Acquire);
        assert_eq!(
            watchdog.check_at(last + 60 * MICROS_PER_SEC, DEFAULT_WATCHDOG_THRESHOLD),
            None
        );
    }

    #[test]
    fn suspect_names_the_failing_side() {
        let never = PollWatchdogRecord {
            age: Duration::from_secs(9),
            polls: 0,
            contexts: 1,
            outstanding_fences: 1,
        };
        assert!(never.format("venus").contains("suspect=host-never-polled"));

        let stopped = PollWatchdogRecord {
            age: Duration::from_secs(9),
            polls: 4321,
            contexts: 2,
            outstanding_fences: 7,
        };
        let line = stopped.format("venus");
        assert!(line.contains("suspect=poll-stopped-with-fences-outstanding"));
        assert!(line.contains("age_ms=9000"));
        assert!(line.contains("outstanding_fences=7"));

        let idle = PollWatchdogRecord {
            age: Duration::from_secs(9),
            polls: 10,
            contexts: 1,
            outstanding_fences: 0,
        };
        assert!(idle
            .format("venus")
            .contains("suspect=idle-no-outstanding-fence"));
    }
}
