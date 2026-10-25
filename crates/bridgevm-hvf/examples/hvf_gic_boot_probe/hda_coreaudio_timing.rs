//! AudioQueue callback timing for the `hda CoreAudio timing:` record.
//!
//! AudioQueue holds three 10 ms buffers. A callback that runs more than 20 ms
//! after it was due finds the other two played out and the device starved,
//! which neither the stats nor the continuity record can see, and nothing
//! showed how full the ring ran. For every callback before teardown this
//! counts:
//!
//! - the interval since the previous callback (<=10, <=15, <=20, <=30, <=50
//!   ms and above) and the longest;
//! - `late_callbacks`: intervals more than 20 ms beyond the 10 ms buffer
//!   period, that is over 30 ms, so exactly the last two interval buckets;
//! - `trylock_misses`: every failed ring try_lock, inside a playback span or
//!   not (the continuity record counts only those inside one);
//! - the ring level a callback found before draining it, from the first
//!   callback that plays guest PCM until the guest stops its stream:
//!   `ring_samples` and the minimum, maximum and final level in ms.
//!
//! The callback thread is the only writer. It reads the host clock
//! (`Instant::now`, a user-space clock read on macOS, not a system call) and
//! updates relaxed atomics inside a sequence count, so a reader gets a
//! consistent record even while callbacks continue; the callback never locks,
//! allocates or waits, and what it copies into its buffer is unchanged.
//! `CoreAudioPcmSink::install` adds the record to the platform's HDA
//! diagnostic records, which the controller prints as the platform drops it
//! after the final report, just before this sink's teardown prints the
//! lifecycle, stats and continuity records (hda/diagnostics_teardown.rs).

use super::super::hda_coreaudio_stats::Shared;
use super::super::{CoreAudioPcmSink, BYTES_PER_FRAME, SAMPLE_RATE};
use bridgevm_hvf::platform_virt::VirtPlatform;
use std::sync::atomic::Ordering::{Acquire, Relaxed, Release};
use std::sync::atomic::{fence, AtomicBool, AtomicU64};
use std::sync::{Arc, Weak};
use std::time::Instant;

const MS: u64 = 1_000_000;
/// Upper bounds of the interval buckets; a longer interval is in the last.
const INTERVAL_BOUNDS_NS: [u64; 5] = [10 * MS, 15 * MS, 20 * MS, 30 * MS, 50 * MS];
/// The 10 ms buffer period plus the 20 ms the other two buffers cover.
const LATE_NS: u64 = 30 * MS;
const NO_CALLBACK: u64 = u64::MAX;
const BYTES_PER_MS: u64 = SAMPLE_RATE as u64 * BYTES_PER_FRAME as u64 / 1000;

pub(in crate::hda_coreaudio) struct CallbackTiming {
    base: Instant,
    /// Odd while the callback thread is updating the counters below.
    sequence: AtomicU64,
    last_ns: AtomicU64,
    callbacks: AtomicU64,
    intervals: [AtomicU64; 6],
    max_interval_ns: AtomicU64,
    late: AtomicU64,
    trylock_misses: AtomicU64,
    playing: AtomicBool,
    ring_samples: AtomicU64,
    ring_min: AtomicU64,
    ring_max: AtomicU64,
    ring_final: AtomicU64,
}

impl Default for CallbackTiming {
    fn default() -> Self {
        Self::starting_at(Instant::now())
    }
}

impl CallbackTiming {
    fn starting_at(base: Instant) -> Self {
        Self {
            base,
            sequence: AtomicU64::new(0),
            last_ns: AtomicU64::new(NO_CALLBACK),
            callbacks: AtomicU64::new(0),
            intervals: Default::default(),
            max_interval_ns: AtomicU64::new(0),
            late: AtomicU64::new(0),
            trylock_misses: AtomicU64::new(0),
            playing: AtomicBool::new(false),
            ring_samples: AtomicU64::new(0),
            ring_min: AtomicU64::new(0),
            ring_max: AtomicU64::new(0),
            ring_final: AtomicU64::new(0),
        }
    }

    /// One update no snapshot sees half done. Callback thread only.
    fn write(&self, update: impl FnOnce()) {
        let sequence = self.sequence.load(Relaxed);
        self.sequence.store(sequence.wrapping_add(1), Relaxed);
        fence(Release);
        update();
        self.sequence.store(sequence.wrapping_add(2), Release);
    }

    /// One output callback at `now`; `contended` when its ring try_lock failed.
    fn note_callback(&self, now: Instant, contended: bool) {
        let since = now.saturating_duration_since(self.base).as_nanos();
        let at = u64::try_from(since).unwrap_or(NO_CALLBACK - 1);
        self.write(|| {
            self.callbacks.fetch_add(1, Relaxed);
            self.trylock_misses.fetch_add(u64::from(contended), Relaxed);
            let last = self.last_ns.swap(at, Relaxed);
            if last == NO_CALLBACK {
                return;
            }
            let interval = at.saturating_sub(last);
            let bucket = INTERVAL_BOUNDS_NS
                .iter()
                .position(|&bound| interval <= bound);
            self.intervals[bucket.unwrap_or(INTERVAL_BOUNDS_NS.len())].fetch_add(1, Relaxed);
            self.max_interval_ns.fetch_max(interval, Relaxed);
            self.late.fetch_add(u64::from(interval > LATE_NS), Relaxed);
        });
    }

    /// A callback found `ring_bytes` in the ring and copied `pcm_bytes`.
    fn note_drain(&self, ring_bytes: usize, pcm_bytes: usize, stream_idle: bool) {
        if stream_idle {
            self.playing.store(false, Relaxed);
            return;
        }
        if pcm_bytes > 0 {
            self.playing.store(true, Relaxed);
        }
        if !self.playing.load(Relaxed) {
            return;
        }
        let level = ring_bytes as u64;
        self.write(|| {
            let first = self.ring_samples.fetch_add(1, Relaxed) == 0;
            let min = self.ring_min.load(Relaxed);
            self.ring_min
                .store(if first { level } else { min.min(level) }, Relaxed);
            self.ring_max.fetch_max(level, Relaxed);
            self.ring_final.store(level, Relaxed);
        });
    }

    /// A consistent snapshot, taken while callbacks may still run: a read the
    /// callback thread overlapped is retried, never returned. Not real-time.
    fn snapshot(&self) -> TimingSnapshot {
        loop {
            let before = self.sequence.load(Acquire);
            if before % 2 == 0 {
                let snapshot = self.read();
                fence(Acquire);
                if self.sequence.load(Relaxed) == before {
                    return snapshot;
                }
            }
            std::thread::yield_now();
        }
    }

    fn read(&self) -> TimingSnapshot {
        let load = |counter: &AtomicU64| counter.load(Relaxed);
        TimingSnapshot {
            callbacks: load(&self.callbacks),
            intervals: self.intervals.each_ref().map(load),
            max_interval_ms: load(&self.max_interval_ns) / MS,
            late_callbacks: load(&self.late),
            trylock_misses: load(&self.trylock_misses),
            ring_samples: load(&self.ring_samples),
            ring_min_ms: load(&self.ring_min) / BYTES_PER_MS,
            ring_max_ms: load(&self.ring_max) / BYTES_PER_MS,
            ring_final_ms: load(&self.ring_final) / BYTES_PER_MS,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct TimingSnapshot {
    callbacks: u64,
    intervals: [u64; 6],
    max_interval_ms: u64,
    late_callbacks: u64,
    trylock_misses: u64,
    ring_samples: u64,
    ring_min_ms: u64,
    ring_max_ms: u64,
    ring_final_ms: u64,
}

impl TimingSnapshot {
    fn record(&self) -> String {
        let [le10, le15, le20, le30, le50, over] = self.intervals;
        format!(
            "hda CoreAudio timing: callbacks={} interval_le_10ms={le10} interval_le_15ms={le15} interval_le_20ms={le20} interval_le_30ms={le30} interval_le_50ms={le50} interval_gt_50ms={over} max_interval_ms={} late_callbacks={} trylock_misses={} ring_samples={} ring_min_ms={} ring_max_ms={} ring_final_ms={}",
            self.callbacks,
            self.max_interval_ms,
            self.late_callbacks,
            self.trylock_misses,
            self.ring_samples,
            self.ring_min_ms,
            self.ring_max_ms,
            self.ring_final_ms
        )
    }
}

/// The live sink's timing record, or none once the sink is gone.
fn host_records(shared: &Weak<Shared>) -> Vec<String> {
    let timing = |shared: Arc<Shared>| shared.continuity.timing.snapshot().record();
    shared.upgrade().map(timing).into_iter().collect()
}

impl CoreAudioPcmSink {
    /// Install this sink on the platform's HDA controller, with its timing
    /// record among the diagnostic records the controller prints at drop.
    pub fn install(self, platform: &mut VirtPlatform) {
        let shared = Arc::downgrade(&self.shared);
        platform.set_hda_diagnostic_host_records(Box::new(move || host_records(&shared)));
        platform.set_hda_pcm_sink(Some(Box::new(self)));
    }
}

#[path = "hda_coreaudio_timing_callback.rs"]
pub mod callback;
#[cfg(test)]
#[path = "hda_coreaudio_timing_tests.rs"]
mod tests;
