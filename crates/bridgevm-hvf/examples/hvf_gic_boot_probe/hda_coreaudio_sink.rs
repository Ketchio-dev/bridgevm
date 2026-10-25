//! The CoreAudio sink's HdaPcmSink entry points, kept apart from the
//! producer's framing in hda_coreaudio_producer.rs.

use super::super::{CoreAudioPcmSink, RING_CAPACITY_BYTES};
use bridgevm_hvf::hda::HdaPcmSink;

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
