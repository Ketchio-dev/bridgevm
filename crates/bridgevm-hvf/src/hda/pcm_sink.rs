//! Host PCM destinations for the HDA playback stream.

/// Host destination for interleaved PCM, called on the vCPU thread under the
/// platform lock: return promptly and move potentially blocking work elsewhere.
pub trait HdaPcmSink: Send {
    fn write_pcm(&mut self, samples: &[u8], rate: u32, channels: u8, bits: u8);

    /// Guest stopped a running stream (RUN clear or reset), not a DMA error halt.
    fn stream_stopped(&mut self) {}

    /// Unlike `stream_stopped`, ends a PCM generation on every controller/stream
    /// reset (even with RUN clear) or effective stopped-stream format change.
    /// Resolve partial frames. RUN pause/resume and DMA faults do not trigger it.
    fn stream_reset(&mut self) {}
}
