//! Start threshold for CoreAudio playback.
//!
//! Guest PCM reaches the ring only when a vCPU exit drains the HDA stream, so
//! the ring fills in bursts while the output callback empties it every 10 ms.
//! With no reserve, the first stall after playback starts left a partial buffer
//! silent: T17 r52 counted 13 and 14 gaps per probe process whether a span held
//! 447 or 1,789 callbacks. The callback therefore leaves its buffer silent until
//! the ring holds `PREFILL_BYTES`, and then drains it as before. That silence
//! comes before the continuity span opens, so it is start latency, not a gap.
//!
//! A stopped stream flushes whatever the ring still holds, so a sound shorter
//! than the threshold still plays. Playback re-primes only after the guest
//! stopped its stream and the ring ran short, which is where a span closes;
//! an underrun inside a running stream keeps playing whatever arrives.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};

use super::hda_coreaudio_ring::drain_ring_into;
use super::AUDIO_QUEUE_BUFFER_BYTES;

/// Four callback buffers, 40 ms at 48 kHz stereo s16le.
pub(super) const PREFILL_BYTES: usize = 4 * AUDIO_QUEUE_BUFFER_BYTES as usize;

pub(super) struct Prefill {
    threshold: usize,
    primed: AtomicBool,
}

impl Prefill {
    pub(super) const fn new(threshold: usize) -> Self {
        Self {
            threshold,
            primed: AtomicBool::new(false),
        }
    }

    /// Drain the ring into one callback buffer once playback is primed, and
    /// return the PCM bytes copied. Callback thread only, under the ring lock.
    pub(super) fn drain(&self, ring: &mut VecDeque<u8>, destination: &mut [u8], stream_idle: bool) -> usize {
        let flush = stream_idle && !ring.is_empty();
        if !self.primed.load(Relaxed) && ring.len() < self.threshold && !flush {
            return 0;
        }
        let pcm_bytes = drain_ring_into(ring, destination);
        self.primed
            .store(!(stream_idle && pcm_bytes < destination.len()), Relaxed);
        pcm_bytes
    }
}

#[cfg(test)]
#[path = "hda_coreaudio_prefill_tests.rs"]
mod tests;
