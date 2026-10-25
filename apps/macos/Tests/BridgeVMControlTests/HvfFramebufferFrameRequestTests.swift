#if canImport(AppKit)
import AppKit
import XCTest
@testable import BridgeVMControl

/// The display tick asks a writer that answers requests for a frame
/// (display-export-20261023, F1 B2) and leaves any other writer's file alone.
@MainActor
final class HvfFramebufferFrameRequestTests: XCTestCase {
    func testEachDisplayTickPostsAFrameRequestToAWriterThatAnswersThem() throws {
        try withView(marker: 0x5145_5242) { view, path, original in
            view.refreshFrame(at: 0)
            XCTAssertNotNil(view.layer?.contents, "the published frame is still taken")
            let first = try Self.request(path)
            XCTAssertGreaterThan(first, 0, "the tick asked the writer for a frame")
            let now = clock_gettime_nsec_np(CLOCK_UPTIME_RAW)
            XCTAssertGreaterThanOrEqual(now, first, "stamped with CLOCK_UPTIME_RAW")
            XCTAssertLessThan(now &- first, 10_000_000_000, "stamped with CLOCK_UPTIME_RAW")
            view.refreshFrame(at: 1)
            let second = try Self.request(path)
            XCTAssertGreaterThan(second, first, "an idle tick asks again: one request per tick")
            var written = try Data(contentsOf: path)
            written.replaceSubrange(32..<40, with: Data(count: 8))
            XCTAssertEqual(written, original, "only the request word is written")
        }
    }

    func testDisplayTickLeavesAWriterWithoutTheMarkAlone() throws {
        try withView(marker: 0) { view, path, original in
            view.refreshFrame(at: 0)
            view.refreshFrame(at: 1)
            XCTAssertNotNil(view.layer?.contents)
            XCTAssertEqual(try Data(contentsOf: path), original)
        }
    }

    /// A published 1x1 BVFB frame whose writer marks bytes 40..44 with `marker`.
    private func withView(marker: UInt32, _ body: (FBLayerView, URL, Data) throws -> Void) throws {
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
        let view = FBLayerView(session: session)
        defer { view.teardown() }
        try body(view, path, frame)
        withExtendedLifetime(session) {}
    }

    private static func request(_ path: URL) throws -> UInt64 {
        let bytes = try Data(contentsOf: path)
        return bytes[32..<40].reversed().reduce(0) { $0 << 8 | UInt64($1) }
    }
}
#endif
