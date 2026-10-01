//! Shared CoreAudio ring state and A5 telemetry.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use super::hda_coreaudio_continuity::ContinuityCounters;
use super::hda_coreaudio_prefill::Prefill;
use super::hda_coreaudio_teardown::CallbackFailureCounters;

pub(super) struct Shared {
    pub(super) ring: Mutex<VecDeque<u8>>,
    /// Guest PCM frames copied into the ring; nonzero tells real audio from an idle device.
    pub(super) frames_rendered: AtomicU64,
    dropped_writes: AtomicU64,
    dropped_bytes: AtomicU64,
    format_drops: AtomicU64,
    ring_full_drops: AtomicU64,
    pub(super) callback_failures: CallbackFailureCounters,
    pub(super) continuity: ContinuityCounters,
    pub(super) prefill: Prefill,
}

impl Shared {
    pub(super) fn new(ring_capacity_bytes: usize, prefill_bytes: usize) -> Self {
        Self {
            ring: Mutex::new(VecDeque::with_capacity(ring_capacity_bytes)),
            frames_rendered: AtomicU64::new(0),
            dropped_writes: AtomicU64::new(0),
            dropped_bytes: AtomicU64::new(0),
            format_drops: AtomicU64::new(0),
            ring_full_drops: AtomicU64::new(0),
            callback_failures: CallbackFailureCounters::new(),
            continuity: ContinuityCounters::default(),
            prefill: Prefill::new(prefill_bytes),
        }
    }

    fn record_drop(&self, bytes: usize) {
        self.dropped_writes.fetch_add(1, Ordering::Relaxed);
        self.dropped_bytes
            .fetch_add(bytes as u64, Ordering::Relaxed);
    }

    pub(super) fn record_format_drop(&self, bytes: usize) {
        self.format_drops.fetch_add(1, Ordering::Relaxed);
        self.record_drop(bytes);
    }

    pub(super) fn record_ring_full_drop(&self, bytes: usize) {
        self.ring_full_drops.fetch_add(1, Ordering::Relaxed);
        self.record_drop(bytes);
    }

    pub(super) fn print_stats(&self, lifecycle: [i32; 2]) {
        let load = |counter: &AtomicU64| counter.load(Ordering::Relaxed);
        let (frames, drops) = (load(&self.frames_rendered), load(&self.dropped_writes));
        let (bytes, format) = (load(&self.dropped_bytes), load(&self.format_drops));
        let ring_full = load(&self.ring_full_drops);
        self.callback_failures
            .print_stats(frames, drops, bytes, format, ring_full, lifecycle);
    }
}
