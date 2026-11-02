//! Start threshold for CoreAudio playback.
//!
//! HDA polling supplies guest PCM in bursts while the output callback empties
//! the ring every 10 ms.
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

use std::sync::atomic::AtomicBool;

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
}

#[path = "hda_coreaudio_prefill_drain.rs"]
mod drain;

#[cfg(test)]
#[path = "hda_coreaudio_prefill_tests.rs"]
mod tests;
