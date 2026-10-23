#if canImport(AppKit)
import _Builtin_stdatomic

/// Little-endian header loads from a mapped BVFB display file.
extension HvfFramebufferFrameReader {
    static func readUInt32(_ pointer: UnsafeRawPointer, at offset: Int) -> UInt32 {
        UInt32(littleEndian: pointer.load(fromByteOffset: offset, as: UInt32.self))
    }

    /// The seqlock word at byte 24, loaded with acquire ordering.
    static func acquireSequence(_ pointer: UnsafeRawPointer) -> UInt64 {
        let sequence = pointer.load(fromByteOffset: 24, as: UInt64.self)
        atomic_thread_fence(memory_order_acquire)
        return UInt64(littleEndian: sequence)
    }
}
#endif
