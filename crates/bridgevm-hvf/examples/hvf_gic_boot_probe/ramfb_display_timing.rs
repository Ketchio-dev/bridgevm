//! The ramfb display exporter's `ramfb display timing:` record.
//!
//! Each export tick counts how late it woke against its deadline, and each
//! published frame the interval since the previous one, so one run measures
//! the cadence at which frames reached the app's display file. The probe joins
//! the exporter before the final report, whose footer its stop record must
//! precede, so the thread does not print this record: a running exporter
//! leaves it with the platform (`set_teardown_host_records`), which prints it
//! once as it drops, after the HDA controller's records, in the post-footer
//! host tail that scripts/live-gates/hvf_host_display.py admits. The counts
//! use only the instants the loop already reads; they change nothing the
//! exporter publishes or when, and the guest cannot observe them.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use crate::VirtPlatform;

/// Upper bounds of the tick-lateness buckets; a later wake is late_gt_16ms.
const LATE_BOUNDS: [Duration; 4] = [ms(1), ms(4), ms(8), ms(16)];
/// Upper bounds of the publish-interval buckets; a longer one is interval_gt_70ms.
const INTERVAL_BOUNDS: [Duration; 3] = [ms(20), ms(40), ms(70)];

const fn ms(value: u64) -> Duration {
    Duration::from_millis(value)
}

/// The exporter's counts, shared by its thread and the platform's record.
pub(super) type SharedTiming = Arc<Mutex<RamfbDisplayTiming>>;

#[derive(Debug, Default)]
pub(super) struct RamfbDisplayTiming {
    ticks: u64,
    late: [u64; 5],
    max_late: Duration,
    published: u64,
    intervals: [u64; 4],
    max_interval: Duration,
    last_published: Option<Instant>,
}

impl RamfbDisplayTiming {
    /// One tick that woke at `woke` for `deadline` and, when it published a
    /// frame, finished publishing it at `published`.
    pub(super) fn note_tick(
        &mut self,
        deadline: Instant,
        woke: Instant,
        published: Option<Instant>,
    ) {
        let late = woke.saturating_duration_since(deadline);
        self.ticks = self.ticks.saturating_add(1);
        count(&mut self.late, &LATE_BOUNDS, late);
        self.max_late = self.max_late.max(late);
        let Some(at) = published else { return };
        self.published = self.published.saturating_add(1);
        if let Some(previous) = self.last_published.replace(at) {
            let interval = at.saturating_duration_since(previous);
            count(&mut self.intervals, &INTERVAL_BOUNDS, interval);
            self.max_interval = self.max_interval.max(interval);
        }
    }

    pub(super) fn record(&self) -> String {
        let (ticks, published) = (self.ticks, self.published);
        let [le_1, le_4, le_8, le_16, gt_16] = self.late;
        let [le_20, le_40, le_70, gt_70] = self.intervals;
        let max_late = u64::try_from(self.max_late.as_micros()).unwrap_or(u64::MAX);
        let max_interval = u64::try_from(self.max_interval.as_millis()).unwrap_or(u64::MAX);
        format!(
            "ramfb display timing: ticks={ticks} late_le_1ms={le_1} late_le_4ms={le_4} late_le_8ms={le_8} late_le_16ms={le_16} late_gt_16ms={gt_16} max_late_us={max_late} published={published} interval_le_20ms={le_20} interval_le_40ms={le_40} interval_le_70ms={le_70} interval_gt_70ms={gt_70} max_interval_ms={max_interval}"
        )
    }
}

/// Count `value` in the first bucket whose upper bound it does not exceed.
fn count(buckets: &mut [u64], bounds: &[Duration], value: Duration) {
    let bucket = bounds
        .iter()
        .position(|bound| value <= *bound)
        .unwrap_or(bounds.len());
    buckets[bucket] = buckets[bucket].saturating_add(1);
}

pub(super) fn lock(timing: &Mutex<RamfbDisplayTiming>) -> MutexGuard<'_, RamfbDisplayTiming> {
    timing.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Leave a running exporter's record with the platform, read as it drops.
pub(super) fn install(platform: &Mutex<VirtPlatform>, timing: &SharedTiming) {
    let timing = Arc::clone(timing);
    if let Ok(mut platform) = platform.lock() {
        platform.set_teardown_host_records(Box::new(move || vec![lock(&timing).record()]));
    }
}

#[cfg(test)]
#[path = "ramfb_display_timing_tests.rs"]
mod tests;
