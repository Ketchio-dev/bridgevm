#if canImport(AppKit)
import AppKit
import XCTest
@testable import BridgeVMControl

/// The display tick asks a writer that answers requests for a frame and leaves any other writer's file alone.
@MainActor
final class HvfFramebufferFrameRequestTests: XCTestCase {
    func testIdleDisplayTicksContinueToOfferRequests() async throws {
        try await withView(marker: 0x5145_5242) { view, path, original in
            view.refreshFrame(at: 0)
            XCTAssertNotNil(view.layer?.contents, "the published frame is still taken")
            let first = try await Self.nextRequest(path, after: 0)
            XCTAssertGreaterThan(first, 0, "the tick asked the writer for a frame")
            let now = clock_gettime_nsec_np(CLOCK_UPTIME_RAW)
            XCTAssertGreaterThanOrEqual(now, first, "stamped with CLOCK_UPTIME_RAW")
            XCTAssertLessThan(now &- first, 10_000_000_000, "stamped with CLOCK_UPTIME_RAW")
            view.refreshFrame(at: 1)
            _ = try await Self.nextRequest(path, after: first)
            var written = try Data(contentsOf: path)
            written.replaceSubrange(32..<40, with: Data(count: 8))
            XCTAssertEqual(written, original, "only the request word is written")
        }
    }

    func testDisplayTickLeavesAWriterWithoutTheMarkAlone() async throws {
        try await withView(marker: 0) { view, path, original in
            view.frameRequester.clock = {
                XCTFail("An unmarked writer must not receive a request")
                return 1
            }
            view.refreshFrame(at: 0)
            view.refreshFrame(at: 1)
            XCTAssertNotNil(view.layer?.contents)
            XCTAssertEqual(try Data(contentsOf: path), original)
        }
    }

    func testInterruptedWriteLeavesDisplayResponsiveAndKeepsOnlyLatestTick() async throws {
        let requests = Requests()
        let writing = DispatchSemaphore(value: 0), release = DispatchSemaphore(value: 0)
        let requester = HvfFramebufferFrameRequester { descriptor, timestamp in
            if requests.record() {
                writing.signal()
                _ = release.wait(timeout: .now() + 5)
                errno = EINTR
                return
            }
            Self.write(descriptor, timestamp)
        }
        try await withView(marker: 0x5145_5242, requester: requester) { view, path, _ in
            defer { release.signal() }
            requester.clock = { 1 }
            view.refreshFrame(at: 0)
            let writerReached = await waitOffCooperativePool(writing, seconds: 2)
            XCTAssertTrue(writerReached)
            for tick in 1...12 {
                requester.clock = { UInt64(tick + 1) }
                view.refreshFrame(at: Double(tick))
            }
            XCTAssertNotNil(view.layer?.contents)
            XCTAssertEqual(requests.count, 1)
            XCTAssertEqual(try Self.request(path), 0)

            release.signal()
            let written = try await Self.nextRequest(path, after: 0)
            XCTAssertEqual(requests.count, 2)
            XCTAssertEqual(written, 13, "The write must use the final offered timestamp")
        }
    }

    private final class Requests: @unchecked Sendable {
        private let lock = NSLock()
        private var attempts = 0
        var count: Int { lock.withLock { attempts } }

        func record() -> Bool {
            lock.withLock {
                attempts += 1
                return attempts == 1
            }
        }
    }

    nonisolated private static func write(_ descriptor: Int32, _ timestamp: UInt64) {
        var little = timestamp.littleEndian
        _ = withUnsafeBytes(of: &little) { Darwin.pwrite(descriptor, $0.baseAddress, 8, 32) }
    }

    private static func nextRequest(_ path: URL, after prior: UInt64) async throws -> UInt64 {
        let deadline = ContinuousClock.now + .seconds(2)
        repeat {
            let posted = try request(path)
            if posted > prior { return posted }
            try await Task.sleep(for: .milliseconds(5))
        } while ContinuousClock.now < deadline
        XCTFail("The writer did not receive the request")
        return prior
    }

    /// A published 1x1 BVFB frame whose writer marks bytes 40..44 with `marker`.
    private func withView(marker: UInt32, requester: HvfFramebufferFrameRequester = HvfFramebufferFrameRequester(),
                          _ body: (FBLayerView, URL, Data) async throws -> Void) async throws {
        let root = LibraryDeletionTestDirectory.temporary.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        var frame = Data(count: 68)
        let words: [(Int, UInt32)] = [(0, 0x42564642), (4, 1), (8, 1), (12, 1), (16, 4), (24, 2), (40, marker), (64, 0xff112233)]
        for (offset, value) in words {
            var little = value.littleEndian
            withUnsafeBytes(of: &little) { frame.replaceSubrange(offset..<(offset + 4), with: $0) }
        }
        let path = root.appendingPathComponent("display.fb")
        try frame.write(to: path)
        let config = HvfEngineConfig(targetDiskPath: "target", uefiVarsPath: "vars", evidenceDir: root.path,
            watchdogMs: nil, ramMiB: 6144, smpCpus: 4, clipboardSync: false, shareHostDir: nil,
            shareGuestDir: nil, virtioNet: false, virtioGpu3d: false, nvmeBufferedIO: true,
            ctlFilePath: root.appendingPathComponent("agent.ctl").path)
        let session = HvfEngineSession(config: config, repoRoot: root) { _ in false }
        let view = FBLayerView(session: session, frameRequester: requester)
        defer { view.teardown() }
        try await body(view, path, frame)
        withExtendedLifetime(session) {}
    }

    private static func request(_ path: URL) throws -> UInt64 {
        let bytes = try Data(contentsOf: path)
        return bytes[32..<40].reversed().reduce(0) { $0 << 8 | UInt64($1) }
    }
}
#endif
