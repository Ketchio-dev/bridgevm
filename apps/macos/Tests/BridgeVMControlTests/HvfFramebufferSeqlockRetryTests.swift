#if canImport(AppKit)
import AppKit
import XCTest
@testable import BridgeVMControl

/// A display tick that finds the BVFB writer mid-frame, or sees the sequence
/// move during its copy, retries in the same tick: at most 2 times, within 0.5 ms.
@MainActor
final class HvfFramebufferSeqlockRetryTests: XCTestCase {
    func testOddSequenceThatCompletesWithinTheTickIsTakenThisTick() throws {
        try withView(sequence: 3) { view, path, made in
            var pauses = 0
            view.frameReader.clock = { 0 }
            view.frameReader.pause = { writing, _ in
                pauses += 1
                XCTAssertTrue(writing(), "the tick waits while the writer holds the frame")
                // The writer finishes its frame while the tick waits.
                do {
                    try Self.update(UInt32(0xffaabbcc), at: 64, path: path)
                    try Self.update(UInt64(4), at: 24, path: path)
                } catch {
                    XCTFail("the writer could not finish its frame: \(error)")
                }
            }
            view.refreshFrame(at: 0)
            XCTAssertEqual(pauses, 1)
            XCTAssertEqual(made(), 1, "the frame is taken in the tick that found the writer mid-frame")
            let contents = try XCTUnwrap(view.layer?.contents)
            XCTAssertEqual(CFGetTypeID(contents as CFTypeRef), CGImage.typeID)
            let image = contents as! CGImage
            XCTAssertEqual(try XCTUnwrap(image.dataProvider?.data) as Data, Data([0xcc, 0xbb, 0xaa, 0xff]))
            XCTAssertEqual(view.accessibilityValue() as? String, "frame 1x1")
            view.refreshFrame(at: 1)
            XCTAssertEqual([pauses, made()], [1, 1], "the same sequence is not taken twice")
        }
    }

    func testMismatchRetriesWithinBudgetThenGivesUp() throws {
        // Every copy sees the writer publish again: two retries, then the tick gives up.
        try withView(sequence: 2) { view, _, made in
            let script: [UInt64] = [2, 4, 4, 6, 6, 8, 8, 8]
            var loads = 0, pauses = 0
            view.frameReader.clock = { 0 }
            view.frameReader.loadSequence = { _ in
                loads += 1
                return script[min(loads, script.count) - 1]
            }
            view.frameReader.pause = { _, _ in pauses += 1 }
            view.refreshFrame(at: 0)
            XCTAssertEqual([loads, pauses, made()], [6, 2, 0])
            XCTAssertNil(view.layer?.contents)
            XCTAssertEqual(view.accessibilityValue() as? String, "no-frame")
            view.refreshFrame(at: 1)
            XCTAssertEqual([loads, pauses, made()], [8, 2, 1], "the next tick takes the settled frame")
            XCTAssertEqual(view.accessibilityValue() as? String, "frame 1x1")
        }
        // Once 0.5 ms have passed since the tick's first read, it does not retry.
        try withView(sequence: 2) { view, _, made in
            let script: [UInt64] = [2, 4], times: [UInt64] = [0, 500_000]
            var loads = 0, reads = 0, pauses = 0
            view.frameReader.clock = {
                reads += 1
                return times[min(reads, times.count) - 1]
            }
            view.frameReader.loadSequence = { _ in
                loads += 1
                return script[min(loads, script.count) - 1]
            }
            view.frameReader.pause = { _, _ in pauses += 1 }
            view.refreshFrame(at: 0)
            XCTAssertEqual([loads, pauses, made()], [2, 0, 0])
            XCTAssertNil(view.layer?.contents)
        }
    }

    /// A 1x1 BVFB file at `sequence` and a view over it whose image factory counts frames made.
    private func withView(sequence: UInt64,
                          _ body: (FBLayerView, URL, () -> Int) throws -> Void) throws {
        var made = 0
        let defaults = HvfFramebufferImageFactory()
        var factory = defaults
        factory.provider = { buffer, count in
            made += 1
            return defaults.provider(buffer, count)
        }
        let root = LibraryDeletionTestDirectory.temporary.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let path = root.appendingPathComponent("display.fb")
        try Data(count: 68).write(to: path)
        for (offset, value): (UInt64, UInt32) in [(0, 0x42564642), (4, 1), (8, 1), (12, 1), (16, 4), (64, 0xff112233)] {
            try Self.update(value, at: offset, path: path)
        }
        try Self.update(sequence, at: 24, path: path)
        let config = HvfEngineConfig(targetDiskPath: "target", uefiVarsPath: "vars", evidenceDir: root.path,
            watchdogMs: nil, ramMiB: 6144, smpCpus: 4, clipboardSync: false, shareHostDir: nil,
            shareGuestDir: nil, virtioNet: false, virtioGpu3d: false, nvmeBufferedIO: true,
            ctlFilePath: root.appendingPathComponent("agent.ctl").path)
        let session = HvfEngineSession(config: config, repoRoot: root) { _ in false }
        let view = FBLayerView(session: session, imageFactory: factory)
        defer { view.teardown() }
        try body(view, path) { made }
        withExtendedLifetime(session) {}
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
