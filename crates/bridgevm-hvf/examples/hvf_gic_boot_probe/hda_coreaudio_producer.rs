//! Producer-side PCM framing. The callback never observes partial frames.

use super::hda_coreaudio_frames::{PcmFrames, Publication};
use super::hda_coreaudio_stats::Shared;
use super::{BITS_PER_CHANNEL, CHANNELS, SAMPLE_RATE};
use std::sync::atomic::Ordering;
#[path = "hda_coreaudio_sink.rs"]
mod sink;

#[derive(Default)]
pub(super) struct PcmProducer {
    frames: PcmFrames,
}

impl PcmProducer {
    pub(super) fn write_format(
        &mut self,
        samples: &[u8],
        format: (u32, u8, u8),
        shared: &Shared,
        capacity: usize,
    ) {
        if samples.is_empty() {
            return;
        }
        if format != (SAMPLE_RATE, CHANNELS, BITS_PER_CHANNEL) {
            self.finish(shared, capacity);
            shared.record_format_drop(samples.len());
            return;
        }
        self.write(samples, shared, capacity);
    }

    pub(super) fn write(&mut self, samples: &[u8], shared: &Shared, capacity: usize) {
        let mut ring = shared.ring.producer();
        // Publish the activity flag before the ring's release publication.
        shared.continuity.note_pcm_written();
        let result = self.frames.write(samples, &mut ring, capacity);
        Self::record(shared, result);
    }

    pub(super) fn finish(&mut self, shared: &Shared, capacity: usize) {
        let mut ring = shared.ring.producer();
        // A reset flushes its terminal fragment, even below the start threshold.
        shared.continuity.note_stream_reset();
        let result = self.frames.finish(&mut ring, capacity);
        Self::record(shared, result);
    }

    fn record(shared: &Shared, result: Publication) {
        if result.dropped_bytes != 0 {
            // One rejected publication batch, counted in real source bytes.
            shared.record_ring_full_drop(result.dropped_bytes);
        }
        shared
            .frames_rendered
            .fetch_add(result.guest_frames, Ordering::Relaxed);
    }
}

#[cfg(test)]
#[path = "hda_coreaudio_dma_tests.rs"]
mod tests;
