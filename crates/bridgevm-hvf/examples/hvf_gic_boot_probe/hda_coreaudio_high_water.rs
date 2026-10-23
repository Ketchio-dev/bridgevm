//! The ring's 70 ms high-water mark: guest PCM the ring cannot play soon is
//! trimmed instead of queued.
//!
//! The callback holds playback 40 ms behind the guest (hda_coreaudio_prefill.rs),
//! and nothing brought a fuller ring back: one capped poll writes 100 ms at
//! once, and that latency stayed. T17 r92's ring ended at 104 ms with no
//! contention and no late callback. So the producer stops appending once the
//! ring would pass 70 ms, the 40 ms reserve plus the 10.7 ms sawtooth of the
//! device's 512-frame cycle plus margin, and trims the whole frames beyond it.
//! A ring riding the mark trims only net excess, such as a burst or a device
//! clock slower than the host's; correcting drift itself would take a
//! sample-rate converter. Below the mark nothing changes.
//!
//! A trim is still guest PCM that never reached the ring, which A5 and T17
//! read as `drops`: before the mark such excess grew the 500 ms ring until it
//! rejected whole batches as `ring_full_drops`, and the mark makes that
//! overflow unreachable. So each trimming write is published as one
//! ring-full drop of its trimmed bytes, keeping `drops == format_drops +
//! ring_full_drops` and `drops == 0` exactly as strict as before, and the
//! timing record prints the trimmed total as `ring_trimmed_ms`. The partial
//! frame after the trimmed ones stays private for the next write, so accepted
//! bytes keep their frame phase.

use super::{PcmFrames, Publication, FRAME};
use std::collections::VecDeque;

/// 70 ms of 48 kHz stereo s16le.
pub(in crate::hda_coreaudio) const RING_HIGH_WATER_BYTES: usize = 70 * 48 * FRAME;

impl PcmFrames {
    /// `write`, stopping at `high_water` bytes in the ring: the whole frames
    /// beyond it are trimmed, published as dropped and returned beside it.
    pub(in crate::hda_coreaudio) fn write_bounded(
        &mut self,
        samples: &[u8],
        ring: &mut VecDeque<u8>,
        capacity: usize,
        high_water: usize,
    ) -> (Publication, usize) {
        let complete = (samples.len() + self.len) / FRAME * FRAME;
        let room = high_water.saturating_sub(ring.len()) / FRAME * FRAME;
        if complete <= room || complete > capacity.saturating_sub(ring.len()) {
            return (self.write(samples, ring, capacity), 0);
        }
        let consumed = complete - self.len;
        let source = self.tail[..self.len].iter().chain(&samples[..consumed]);
        ring.extend(source.take(room).copied());
        self.len = samples.len() - consumed;
        self.tail[..self.len].copy_from_slice(&samples[consumed..]);
        let trimmed = complete - room;
        let published = Publication {
            guest_frames: (room / FRAME) as u64,
            dropped_bytes: trimmed,
        };
        (published, trimmed)
    }
}
