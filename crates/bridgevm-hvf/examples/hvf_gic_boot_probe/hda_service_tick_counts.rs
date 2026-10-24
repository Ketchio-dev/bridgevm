//! What the HDA service tick did, printed as its `hda service tick: stopped`
//! line when the thread ends.
//!
//! `max_busy_run_ms` attributes the poll gaps the tick could not close: the
//! longest run of consecutive ticks that found the platform lock held, times
//! the tick interval. T17 r94's first-ready session skipped 835 of 1199 ticks
//! as busy and still had five capped gaps of up to 394 ms; a run as long as
//! such a gap says one holder kept the lock throughout, while short runs say
//! the gaps came from somewhere else. A quiesced tick or a new stream ends a
//! run, so a run never spans a reset or a stopped stream.

use std::time::Duration;

/// What the thread did, printed as it ends.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TickCounts {
    /// Wakes for a running stream.
    pub(crate) streams: u64,
    /// Ticks that tried the platform lock.
    pub(crate) ticks: u64,
    pub(crate) busy: u64,
    /// Ticks a reset's `quiesce()` skipped.
    pub(crate) quiesced: u64,
    pub(crate) msis: u64,
    pub(crate) max_late: Duration,
    /// Busy ticks since the last tick that was not busy.
    pub(crate) busy_run: u64,
    /// The longest such run.
    pub(crate) max_busy_run: u64,
}

impl TickCounts {
    /// The guest's stream started running again.
    pub(crate) fn note_stream(&mut self) {
        self.streams += 1;
        self.busy_run = 0;
    }

    /// A reset's `quiesce()` held the gate.
    pub(crate) fn note_quiesced(&mut self) {
        self.quiesced += 1;
        self.busy_run = 0;
    }

    /// A tick tried the platform lock and found it held (`busy`) or not.
    pub(crate) fn note_try(&mut self, busy: bool) {
        self.ticks += 1;
        if busy {
            self.busy += 1;
            self.busy_run += 1;
            self.max_busy_run = self.max_busy_run.max(self.busy_run);
        } else {
            self.busy_run = 0;
        }
    }

    /// The `hda service tick: stopped` line for ticks `interval` apart.
    pub(crate) fn stopped_record(&self, interval: Duration) -> String {
        let interval_ms = u64::try_from(interval.as_millis()).unwrap_or(u64::MAX);
        format!(
            "hda service tick: stopped streams={} ticks={} busy={} quiesced={} msis={} \
             max_late_us={} max_busy_run_ms={}",
            self.streams,
            self.ticks,
            self.busy,
            self.quiesced,
            self.msis,
            self.max_late.as_micros(),
            self.max_busy_run.saturating_mul(interval_ms)
        )
    }
}
