//! Host PCM destinations for the HDA playback stream.

/// Host-provided destination for decoded interleaved PCM stream bytes.
///
/// Implementations run on the vCPU thread while the platform lock is held, so
/// live sinks must return promptly and move potentially blocking work elsewhere.
pub trait HdaPcmSink: Send {
    fn write_pcm(&mut self, samples: &[u8], rate: u32, channels: u8, bits: u8);

    /// The guest stopped its running playback stream: it cleared RUN, or reset
    /// the stream or the controller. A sink that measures playback continuity
    /// uses it to tell an intended stop from an underrun; the default ignores it.
    /// A stream the device itself halts on a DMA error is not reported.
    fn stream_stopped(&mut self) {}
}
