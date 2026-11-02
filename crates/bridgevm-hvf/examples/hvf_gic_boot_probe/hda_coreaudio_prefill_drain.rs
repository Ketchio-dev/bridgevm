//! Threshold and tail-flush policy over one consumer's captured publication.

use super::super::hda_coreaudio_ring::Consumer;
use super::Prefill;
use std::sync::atomic::Ordering::Relaxed;

impl Prefill {
    /// Drain the ring into one callback buffer once playback is primed, and
    /// return the PCM bytes copied. Callback thread only, under the consumer guard.
    pub(in crate::hda_coreaudio) fn drain(
        &self,
        ring: &mut Consumer<'_>,
        destination: &mut [u8],
        stream_idle: bool,
    ) -> usize {
        let flush = stream_idle && !ring.is_empty();
        if !self.primed.load(Relaxed) && ring.len() < self.threshold && !flush {
            return 0;
        }
        let pcm_bytes = ring.drain_into(destination);
        self.primed
            .store(!(stream_idle && pcm_bytes < destination.len()), Relaxed);
        pcm_bytes
    }
}
