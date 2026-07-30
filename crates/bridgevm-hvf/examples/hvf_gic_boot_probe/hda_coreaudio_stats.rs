//! Shared CoreAudio ring state and A5 telemetry.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

pub(super) struct Shared {
    pub(super) ring: Mutex<VecDeque<u8>>,
    /// Guest PCM frames successfully copied into the host CoreAudio ring.
    /// This distinguishes real audio flow from an idle device, for which every
    /// error counter would also remain zero.
    pub(super) frames_rendered: AtomicU64,
    dropped_writes: AtomicU64,
    dropped_bytes: AtomicU64,
    format_drops: AtomicU64,
    ring_full_drops: AtomicU64,
    pub(super) callback_errors: AtomicU64,
}

impl Shared {
    pub(super) fn new(ring_capacity_bytes: usize) -> Self {
        Self {
            ring: Mutex::new(VecDeque::with_capacity(ring_capacity_bytes)),
            frames_rendered: AtomicU64::new(0),
            dropped_writes: AtomicU64::new(0),
            dropped_bytes: AtomicU64::new(0),
            format_drops: AtomicU64::new(0),
            ring_full_drops: AtomicU64::new(0),
            callback_errors: AtomicU64::new(0),
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

    pub(super) fn print_stats(&self) {
        println!(
            "hda CoreAudio stats: frames_rendered={} drops={} dropped_bytes={} format_drops={} ring_full_drops={} callback_errors={}",
            self.frames_rendered.load(Ordering::Relaxed),
            self.dropped_writes.load(Ordering::Relaxed),
            self.dropped_bytes.load(Ordering::Relaxed),
            self.format_drops.load(Ordering::Relaxed),
            self.ring_full_drops.load(Ordering::Relaxed),
            self.callback_errors.load(Ordering::Relaxed)
        );
    }
}
