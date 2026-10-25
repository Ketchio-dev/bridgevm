#if canImport(AppKit)
import Darwin
import _Builtin_stdatomic

/// Reads a mapped BVFB display file under the seqlock its writers keep
/// (`crates/bridgevm-hvf/src/virtio_gpu/fb_sink_write.rs`): the sequence at
/// byte 24 is odd while a frame is being written and advances by 2 per frame.
///
/// A tick that finds the writer mid-frame, or sees the sequence move during
/// its copy, tries again in the same tick: at most `retries` times, and only
/// while `budgetNanoseconds` have not passed since its first read.
struct HvfFramebufferFrameReader {
    enum Outcome {
        /// A complete frame, copied once into `buffer`, which the caller owns.
        case frame(buffer: UnsafeMutableRawPointer, byteCount: Int,
                   width: Int, height: Int, stride: Int, sequence: UInt64)
        /// The published frame is the one already shown.
        case unchanged
        /// The writer held the frame for the whole budget, or no memory: keep what is shown.
        case busy
        /// Not a BVFB file, or a geometry the mapping cannot hold.
        case invalid
    }

    static let retries = 2
    static let budgetNanoseconds: UInt64 = 500_000

    var clock: () -> UInt64 = { clock_gettime_nsec_np(CLOCK_UPTIME_RAW) }
    /// The sequence word's load, an acquire: no later header or pixel read is
    /// satisfied from before it.
    var loadSequence: (UnsafeRawPointer) -> UInt64 = HvfFramebufferFrameReader.acquireSequence
    /// Runs between two attempts. By default it spins until the writer has
    /// left the frame or the budget has passed.
    var pause: (_ writing: () -> Bool, _ expired: () -> Bool) -> Void = { writing, expired in
        while writing(), !expired() {}
    }

    func read(_ pointer: UnsafeRawPointer, length: Int, shown: UInt64) -> Outcome {
        let deadline = clock() &+ Self.budgetNanoseconds
        var retried = 0
        while true {
            if let outcome = attempt(pointer, length: length, shown: shown) {
                return outcome
            }
            guard retried < Self.retries, clock() < deadline else {
                return .busy
            }
            retried += 1
            pause({ loadSequence(pointer) & 1 == 1 }, { clock() >= deadline })
        }
    }

    /// One seqlock read; nil when the writer was mid-frame or published during the copy.
    private func attempt(_ pointer: UnsafeRawPointer, length: Int, shown: UInt64) -> Outcome? {
        guard Self.readUInt32(pointer, at: 0) == 0x42564642 else {
            return .invalid
        }
        let sequence = loadSequence(pointer)
        guard sequence & 1 == 0 else {
            return nil
        }
        // Skip the copy and CGImage build when no new frame was published: an
        // idle desktop leaves the sequence unchanged for long stretches.
        guard sequence != shown else {
            return .unchanged
        }
        let width = Int(Self.readUInt32(pointer, at: 8))
        let height = Int(Self.readUInt32(pointer, at: 12))
        let stride = Int(Self.readUInt32(pointer, at: 16))
        guard width > 0, height > 0, stride > 0, stride >= width * 4,
              length >= 64, height <= (length - 64) / stride else {
            return .invalid
        }
        // One copy into a buffer the CGDataProvider frees with the CGImage.
        let byteCount = height * stride
        guard let buffer = malloc(byteCount) else {
            return .busy
        }
        Darwin.memcpy(buffer, pointer.advanced(by: 64), byteCount)
        // No pixel read above may be satisfied after the sequence is re-read.
        atomic_thread_fence(memory_order_acquire)
        guard loadSequence(pointer) == sequence else {
            free(buffer)
            return nil
        }
        return .frame(buffer: buffer, byteCount: byteCount,
                      width: width, height: height, stride: stride, sequence: sequence)
    }
}
#endif
