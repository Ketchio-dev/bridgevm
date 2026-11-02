#if canImport(AppKit)
import Darwin

/// Asks the display file's writer for a frame on every display tick (F1 B2,
/// display-export-20261023).
///
/// A writer that answers requests sets bytes 40..44 of the BVFB header to
/// `marker` (`crates/bridgevm-hvf/src/display_fb_requests.rs`). The request is
/// the tick's `CLOCK_UPTIME_RAW` nanoseconds, written to bytes 32..40 with
/// `pwrite`; the write wakes the writer, which copies the guest frame at once,
/// and the next tick shows it. Without the mark nothing is written, so an older
/// writer keeps its own cadence. The header's other bytes are never written.
final class HvfFramebufferFrameRequester {
    static let marker: UInt32 = 0x5145_5242

    var clock: () -> UInt64 = { clock_gettime_nsec_np(CLOCK_UPTIME_RAW) }
    private let lease = HvfFramebufferRequestLease()

    /// `mapping` must be the current mapping of the file at `path`.
    func post(_ mapping: HvfFramebufferMapping, path: String?) {
        guard let path,
              HvfFramebufferFrameReader.readUInt32(mapping.pointer, at: 0) == 0x42564642,
              HvfFramebufferFrameReader.readUInt32(mapping.pointer, at: 40) == Self.marker else {
            return
        }
        guard let descriptor = lease.descriptor(for: mapping, path: path) else { return }
        var request = clock().littleEndian
        _ = withUnsafeBytes(of: &request) { Darwin.pwrite(descriptor, $0.baseAddress, 8, 32) }
    }

    func reset() {
        lease.reset()
    }

}
#endif
