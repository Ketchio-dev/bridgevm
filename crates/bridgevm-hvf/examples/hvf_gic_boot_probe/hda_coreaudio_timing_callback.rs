//! Where the output callback reaches the timing counters, kept apart from
//! the record in hda_coreaudio_timing.rs. Both run on the callback thread.

use super::super::super::hda_coreaudio_stats::Shared;
use super::super::{CallbackFill, ContinuityCounters};
use std::collections::VecDeque;
use std::sync::atomic::Ordering::Relaxed;
use std::time::Instant;

impl ContinuityCounters {
    /// Count one output callback: its timing unless teardown began, then its
    /// continuity exactly as before. Callback thread only.
    pub(in crate::hda_coreaudio) fn record_callback(&self, fill: CallbackFill) {
        if !self.stopping.load(Relaxed) {
            let contended = matches!(fill, CallbackFill::Contended { .. });
            self.timing.note_callback(Instant::now(), contended);
        }
        self.record(fill);
    }
}

/// Drain the ring into one callback buffer exactly as `Prefill::drain` does,
/// noting the level the callback found. Callback thread, under the ring lock.
pub(in crate::hda_coreaudio) fn timed_drain(
    shared: &Shared,
    ring: &mut VecDeque<u8>,
    destination: &mut [u8],
    stream_idle: bool,
) -> usize {
    let ring_bytes = ring.len();
    let pcm_bytes = shared.prefill.drain(ring, destination, stream_idle);
    let continuity = &shared.continuity;
    if !continuity.stopping.load(Relaxed) {
        continuity
            .timing
            .note_drain(ring_bytes, pcm_bytes, stream_idle);
    }
    pcm_bytes
}
