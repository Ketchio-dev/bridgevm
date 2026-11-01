#if canImport(AppKit)
import AppKit
import XCTest
@testable import BridgeVMControl

/// Each display tick reports what it did for the session's `app display pacing:` record, with the frame's BVFB
/// publish time read under the same seqlock as its pixels, and the session begins its records as it starts a
/// runtime and writes them once as it stops. No window is involved.
@MainActor
final class HvfFramebufferPacingTests: XCTestCase {
    func testEachTickReportsItsOutcomeAndTheFramesPublishTime() throws {
        try withView { view, path, _ in
            try Self.update(UInt64(123_456_789), at: 48, path: path)
            XCTAssertEqual(view.refreshFrame(at: 0), .presented(sequence: 2, previous: .max, published: 123_456_789))
            XCTAssertEqual(view.refreshFrame(at: 1), .repeated)
            // The writer holds the frame for the whole read budget: the shown frame stays.
            try Self.update(UInt64(5), at: 24, path: path)
            view.frameReader.clock = { 0 }
            view.frameReader.pause = { _, _ in }
            XCTAssertEqual(view.refreshFrame(at: 2), .busy)
            try Self.update(UInt64(987_654_321), at: 48, path: path)
            try Self.update(UInt64(6), at: 24, path: path)
            XCTAssertEqual(view.refreshFrame(at: 3), .presented(sequence: 6, previous: 2, published: 987_654_321))
            // A writer from before the publish time leaves 0, which the record counts as unknown.
            try Self.update(UInt64(0), at: 48, path: path)
            try Self.update(UInt64(8), at: 24, path: path)
            XCTAssertEqual(view.refreshFrame(at: 4), .presented(sequence: 8, previous: 6, published: 0))
            try Self.update(UInt32(0), at: 0, path: path)
            XCTAssertEqual(view.refreshFrame(at: 5), .unmapped, "no longer a BVFB file")
        }
    }

    func testTheSessionCountsOnlyWhileARuntimeIsObservedAndWritesItsRecordsOnceAsItStops() throws {
        try withView { _, path, session in
            let file = path.deletingLastPathComponent().appendingPathComponent("app-pacing.log")
            final class Link {}
            session.appPacing.display(.repeated, timestamp: 1, target: 1.015625, link: ObjectIdentifier(Link.self), work: 0)
            session.markStopped()
            XCTAssertFalse(FileManager.default.fileExists(atPath: file.path), "no runtime was observed")
            session.resetObservedRuntimeState(clearEvents: false)
            session.appPacing.display(.repeated, timestamp: 1, target: 1.015625, link: ObjectIdentifier(Link.self), work: 0)
            XCTAssertTrue(session.sendKey("enter"), "an unowned router takes the legacy path")
            session.markStopped()
            let written = try String(contentsOf: file, encoding: .utf8).split(separator: "\n")
            XCTAssertEqual(written.count, 2)
            XCTAssertTrue(written[0].hasPrefix("app display pacing: ticks=1 presented=0 repeated=1 "))
            XCTAssertTrue(written[1].hasPrefix("app ordered input: queued=0 coalesced=0 legacy=1 refused=0 "))
            let identity = try FileManager.default.attributesOfItem(atPath: file.path)[.systemFileNumber] as? NSNumber
            session.markStopped()
            XCTAssertEqual(try FileManager.default.attributesOfItem(atPath: file.path)[.systemFileNumber] as? NSNumber,
                           identity, "a stopped session writes nothing more")
        }
    }

    /// A 1x1 BVFB file at sequence 2 and a view over it in a session whose evidence directory holds it.
    private func withView(_ body: (FBLayerView, URL, HvfEngineSession) throws -> Void) throws {
        let root = LibraryDeletionTestDirectory.temporary.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let path = root.appendingPathComponent("display.fb")
        try Data(count: 68).write(to: path)
        for (offset, value): (UInt64, UInt32) in [(0, 0x42564642), (4, 1), (8, 1), (12, 1), (16, 4), (64, 0xff112233)] {
            try Self.update(value, at: offset, path: path)
        }
        try Self.update(UInt64(2), at: 24, path: path)
        let config = HvfEngineConfig(targetDiskPath: "target", uefiVarsPath: "vars", evidenceDir: root.path,
            watchdogMs: nil, ramMiB: 6144, smpCpus: 4, clipboardSync: false, shareHostDir: nil,
            shareGuestDir: nil, virtioNet: false, virtioGpu3d: false, nvmeBufferedIO: true,
            ctlFilePath: root.appendingPathComponent("agent.ctl").path)
        let session = HvfEngineSession(config: config, repoRoot: root) { _ in false }
        let view = FBLayerView(session: session)
        defer { view.teardown() }
        try body(view, path, session)
    }

    private static func update<Value: FixedWidthInteger>(_ value: Value, at offset: UInt64, path: URL) throws {
        let handle = try FileHandle(forWritingTo: path)
        defer { try? handle.close() }
        try handle.seek(toOffset: offset)
        var little = value.littleEndian
        try withUnsafeBytes(of: &little) { try handle.write(contentsOf: Data($0)) }
    }
}
#endif
