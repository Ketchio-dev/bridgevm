#if canImport(AppKit)
/// BVFB header fields beyond the magic and the sequence (HvfFramebufferScalarReads.swift), read inside
/// the frame reader's seqlock attempt.
extension HvfFramebufferFrameReader {
    /// Byte offset of the writer's publish time (crates/bridgevm-hvf/src/virtio_gpu/fb_sink_header.rs):
    /// its CLOCK_UPTIME_RAW nanoseconds when the frame's bytes were complete, 0 from a writer without it.
    static let publishTimeOffset = 48

    /// Width, height and stride as Ints, or nil for a geometry the mapping of `length` bytes cannot hold.
    static func geometry(_ pointer: UnsafeRawPointer, length: Int) -> (Int, Int, Int)? {
        let width = Int(readUInt32(pointer, at: 8))
        let height = Int(readUInt32(pointer, at: 12))
        let stride = Int(readUInt32(pointer, at: 16))
        guard width > 0, height > 0, stride > 0, stride >= width * 4,
              length >= 64, height <= (length - 64) / stride else { return nil }
        return (width, height, stride)
    }

    static func publishTime(_ pointer: UnsafeRawPointer) -> UInt64 {
        UInt64(littleEndian: pointer.load(fromByteOffset: publishTimeOffset, as: UInt64.self))
    }
}
#endif
