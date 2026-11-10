#if canImport(AppKit)
/// The pixels a BVFB frame changed against the file's previous frame, as its writer records them
/// (crates/bridgevm-hvf/src/virtio_gpu/fb_sink_header.rs): x, y, width and height as little-endian u16
/// from byte 56, tagged at byte 44 with the low 32 bits of the sequence of the frame they belong to.
struct HvfFramebufferDamage: Equatable {
    static let tagOffset = 44
    static let rectOffset = 56

    var x: Int, y: Int, width: Int, height: Int

    static func whole(width: Int, height: Int) -> Self { Self(x: 0, y: 0, width: width, height: height) }

    var isEmpty: Bool { width == 0 || height == 0 }

    /// The damage a reader may trust for the frame at `sequence`, read under its seqlock: tagged with that
    /// sequence, published right after the frame `shown` (no frame skipped), and inside the frame. nil means
    /// copy the whole frame: a writer without damage words, a skipped or first frame, or words from
    /// another frame.
    static func trusted(_ pointer: UnsafeRawPointer, sequence: UInt64, shown: UInt64,
                        width: Int, height: Int) -> Self? {
        guard sequence == shown &+ 2,
              HvfFramebufferFrameReader.readUInt32(pointer, at: tagOffset) == UInt32(truncatingIfNeeded: sequence)
        else { return nil }
        func field(_ index: Int) -> Int {
            Int(UInt16(littleEndian: pointer.load(fromByteOffset: rectOffset + 2 * index, as: UInt16.self)))
        }
        let damage = Self(x: field(0), y: field(1), width: field(2), height: field(3))
        guard damage.x + damage.width <= width, damage.y + damage.height <= height else { return nil }
        return damage
    }

    /// The smallest rectangle holding both.
    func union(_ other: Self) -> Self {
        if isEmpty { return other }
        if other.isEmpty { return self }
        let left = min(x, other.x), top = min(y, other.y)
        return Self(x: left, y: top, width: max(x + width, other.x + other.width) - left,
                    height: max(y + height, other.y + other.height) - top)
    }
}
#endif
