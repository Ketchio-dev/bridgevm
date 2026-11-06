//! Where captured PCM comes from: the host's side of the capture stream.

/// A host capture source for the HDA input stream (SD0). The controller asks
/// it for the next bytes of one continuous s16le stream in the guest's format
/// and writes silence wherever the source has none, so a source never blocks
/// and never has to align its reads to frames.
pub trait HdaPcmSource: Send {
    /// Fill `out` from its start with the next captured bytes in this format,
    /// and return how many it filled. Called under the platform lock.
    fn read_pcm(&mut self, out: &mut [u8], rate: u32, channels: u8, bits: u8) -> usize;

    /// The guest started its capture stream (RUN set).
    fn stream_started(&mut self) {}

    /// The guest stopped its capture stream: RUN clear, a stream or controller
    /// reset, or a DMA fault halt.
    fn stream_stopped(&mut self) {}
}
