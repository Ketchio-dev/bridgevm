#if canImport(AppKit)
import Darwin
import Dispatch
import Foundation

/// Offers the display file's writer the latest display tick's frame request.
///
/// A writer that answers requests sets bytes 40..44 of the BVFB header to
/// `marker` (`crates/bridgevm-hvf/src/display_fb_requests.rs`). The request is
/// the tick's `CLOCK_UPTIME_RAW` nanoseconds, written to bytes 32..40 with
/// `pwrite`; the write wakes the writer, which copies the guest frame at once,
/// and the next tick shows it. Without the mark nothing is written, so an older
/// writer keeps its own cadence. Only the request word changes.
/// `lock` protects the mailbox and the dispatch source serializes the file lease and writes.
final class HvfFramebufferFrameRequester: @unchecked Sendable {
    static let marker: UInt32 = 0x5145_5242
    @MainActor var clock: () -> UInt64 = { clock_gettime_nsec_np(CLOCK_UPTIME_RAW) }

    private struct Request {
        let mapping: HvfFramebufferMapping
        let path: String
        let timestamp: UInt64
    }

    private let lock = NSLock()
    private var latest: Request?
    private var retireLease = false
    private let lease = HvfFramebufferRequestLease()
    private let notification = DispatchSource.makeUserDataOrSource(
        queue: DispatchQueue(label: "dev.bridgevm.frame-request", qos: .userInitiated))
    private let writeRequest: @Sendable (Int32, UInt64) -> Void

    init(writeRequest: @escaping @Sendable (Int32, UInt64) -> Void = { descriptor, timestamp in
        var request = timestamp.littleEndian
        _ = withUnsafeBytes(of: &request) { Darwin.pwrite(descriptor, $0.baseAddress, 8, 32) }
    }) {
        self.writeRequest = writeRequest
        notification.setEventHandler { [weak self] in self?.sendLatest() }
        notification.activate()
    }

    /// The view reads the marker while it holds the mapping. The worker only uses the pinned file descriptor.
    @MainActor
    func post(_ mapping: HvfFramebufferMapping, path: String?) {
        guard let path,
              HvfFramebufferFrameReader.readUInt32(mapping.pointer, at: 0) == 0x42564642,
              HvfFramebufferFrameReader.readUInt32(mapping.pointer, at: 40) == Self.marker else {
            return
        }
        let request = Request(mapping: mapping, path: path, timestamp: clock())
        lock.withLock { latest = request }
        notification.or(data: 1)
    }

    @MainActor
    func reset() {
        lock.withLock {
            latest = nil
            retireLease = true
        }
        notification.or(data: 1)
    }

    private func sendLatest() {
        let (request, reset) = lock.withLock {
            let work = (latest, retireLease)
            latest = nil
            retireLease = false
            return work
        }
        if reset { lease.reset() }
        // An in-flight write still names the pinned old file if the view replaces its mapping.
        guard let request, let descriptor = lease.descriptor(for: request.mapping, path: request.path) else { return }
        writeRequest(descriptor, request.timestamp)
    }

    deinit { notification.cancel() }
}
#endif
