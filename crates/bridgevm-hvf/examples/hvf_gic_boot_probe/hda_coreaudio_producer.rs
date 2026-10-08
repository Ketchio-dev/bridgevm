//! Producer-side PCM framing. The callback never observes partial frames.

use super::hda_coreaudio_frames::{PcmFrames, Publication};
use super::hda_coreaudio_stats::Shared;
use super::{CoreAudioPcmSink, BITS_PER_CHANNEL, CHANNELS, RING_CAPACITY_BYTES, SAMPLE_RATE};
use bridgevm_hvf::hda::HdaPcmSink;
use std::sync::atomic::Ordering;

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
        // Producer waits for this short copy; the realtime callback still uses
        // try_lock and substitutes silence rather than waiting on the vCPU.
        let mut ring = shared.ring.lock().unwrap_or_else(|p| p.into_inner());
        let result = self.frames.write(samples, &mut ring, capacity);
        shared.continuity.note_pcm_written();
        drop(ring);
        Self::record(shared, result);
    }

    pub(super) fn finish(&mut self, shared: &Shared, capacity: usize) {
        let mut ring = shared.ring.lock().unwrap_or_else(|p| p.into_inner());
        let result = self.frames.finish(&mut ring, capacity);
        // Even a reset after a DMA halt must let a sub-threshold tail flush.
        // This is not another guest RUN-stop event.
        shared.continuity.note_stream_reset();
        drop(ring);
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

impl HdaPcmSink for CoreAudioPcmSink {
    fn write_pcm(&mut self, samples: &[u8], rate: u32, channels: u8, bits: u8) {
        self.producer.write_format(
            samples,
            (rate, channels, bits),
            &self.shared,
            RING_CAPACITY_BYTES,
        );
    }

    fn stream_stopped(&mut self) {
        // RUN pause resumes at the saved DMA cursor, including any partial frame.
        self.shared.continuity.note_stream_stopped();
    }

    fn stream_reset(&mut self) {
        self.producer.finish(&self.shared, RING_CAPACITY_BYTES);
    }
}
