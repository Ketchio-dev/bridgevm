#if canImport(AppKit)
import Darwin
import _Builtin_stdatomic

/// A published frame inside the reader's seqlock attempt, for its destination to copy.
struct HvfFramebufferFrame {
    /// The pixels, `height` rows of `stride` bytes from byte 64 of the mapping.
    let pixels: UnsafeRawPointer
    let width: Int, height: Int, stride: Int
    let sequence: UInt64
    /// What changed since the frame shown, when the writer's damage words can be trusted; nil: all of it.
    let damage: HvfFramebufferDamage?
}

/// Where the reader copies a frame: the display's IOSurfaces (HvfFramebufferSurfaces.swift), or a buffer
/// for a CGImage (HvfFramebufferImageFactory.swift).
protocol HvfFramebufferFrameDestination {
    /// Copies the frame's pixels; false when it cannot, which keeps what is shown.
    mutating func copy(_ frame: HvfFramebufferFrame) -> Bool
    /// The copy just made is torn: the writer published during it.
    mutating func discard()
}

/// Reads a mapped BVFB display file under the seqlock its writers keep
/// (`crates/bridgevm-hvf/src/virtio_gpu/fb_sink_write.rs`): the sequence at
/// byte 24 is odd while a frame is being written and advances by 2 per frame.
///
/// A tick that finds the writer mid-frame, or sees the sequence move during
/// its copy, tries again in the same tick: at most `retries` times, and only
/// while `budgetNanoseconds` have not passed since its first read.
struct HvfFramebufferFrameReader {
    enum Outcome {
        /// A complete frame, copied into the read's destination, with its publish time read under the
        /// same seqlock (HvfFramebufferHeaderFields.swift).
        case frame(width: Int, height: Int, sequence: UInt64, published: UInt64)
        /// The published frame is the one already shown.
        case unchanged
        /// The writer held the frame for the whole budget, or the destination could not take it: keep
        /// what is shown.
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

    func read<Destination: HvfFramebufferFrameDestination>(
        _ pointer: UnsafeRawPointer, length: Int, shown: UInt64, into destination: inout Destination
    ) -> Outcome {
        let deadline = clock() &+ Self.budgetNanoseconds
        var retried = 0
        while true {
            if let outcome = attempt(pointer, length: length, shown: shown, into: &destination) {
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
    private func attempt<Destination: HvfFramebufferFrameDestination>(
        _ pointer: UnsafeRawPointer, length: Int, shown: UInt64, into destination: inout Destination
    ) -> Outcome? {
        guard Self.readUInt32(pointer, at: 0) == 0x42564642 else {
            return .invalid
        }
        let sequence = loadSequence(pointer)
        guard sequence & 1 == 0 else {
            return nil
        }
        // Skip the copy when no new frame was published: an idle desktop
        // leaves the sequence unchanged for long stretches.
        guard sequence != shown else {
            return .unchanged
        }
        guard let (width, height, stride) = Self.geometry(pointer, length: length) else {
            return .invalid
        }
        let frame = HvfFramebufferFrame(
            pixels: pointer.advanced(by: 64), width: width, height: height, stride: stride, sequence: sequence,
            damage: HvfFramebufferDamage.trusted(pointer, sequence: sequence, shown: shown, width: width, height: height))
        guard destination.copy(frame) else {
            return .busy
        }
        let published = Self.publishTime(pointer)
        // No pixel, damage or time read above may be satisfied after the sequence is re-read.
        atomic_thread_fence(memory_order_acquire)
        guard loadSequence(pointer) == sequence else {
            destination.discard()
            return nil
        }
        return .frame(width: width, height: height, sequence: sequence, published: published)
    }
}
#endif
