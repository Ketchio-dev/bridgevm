import AppKit
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfFramebufferScrollOrderingTests: XCTestCase {
    private final class ScrollEvent: NSEvent {
        override var scrollingDeltaY: CGFloat { 1 }
        override var locationInWindow: NSPoint { NSPoint(x: 75, y: 75) }
    }

    func testScrollDoesNotReplayAnOlderCoalescedMoveAfterItsOwnPosition() throws {
        let root = LibraryDeletionTestDirectory.temporary.resolvingSymlinksInPath()
            .appendingPathComponent("scroll-order-" + UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
        defer { try? FileManager.default.removeItem(at: root) }
        var frame = Data(count: 68)
        put(UInt32(0x42564642), at: 0, into: &frame)
        put(UInt32(1), at: 8, into: &frame); put(UInt32(1), at: 12, into: &frame)
        put(UInt32(4), at: 16, into: &frame); put(UInt64(2), at: 24, into: &frame)
        try frame.write(to: root.appendingPathComponent("display.fb"))
        let config = HvfEngineConfig(targetDiskPath: "target", uefiVarsPath: "vars", evidenceDir: root.path,
            watchdogMs: nil, ramMiB: 6144, smpCpus: 4, clipboardSync: false, shareHostDir: nil,
            shareGuestDir: nil, virtioNet: false, virtioGpu3d: false, nvmeBufferedIO: true,
            ctlFilePath: root.appendingPathComponent("agent.ctl").path)
        let session = HvfEngineSession(config: config, repoRoot: root) { _ in false }
        let view = FBLayerView(session: session)
        defer { view.teardown() }
        view.frame = NSRect(x: 0, y: 0, width: 100, height: 100)
        view.refreshFrame(at: 0)
        let move = try XCTUnwrap(NSEvent.mouseEvent(with: .mouseMoved, location: NSPoint(x: 25, y: 25),
            modifierFlags: [], timestamp: 1, windowNumber: 0, context: nil, eventNumber: 0,
            clickCount: 0, pressure: 0))
        view.mouseMoved(with: move)
        view.scrollWheel(with: ScrollEvent())
        view.refreshFrame(at: 1)
        let commands = try String(contentsOf: root.appendingPathComponent("input.ctl"))
            .split(separator: "\n").map(String.init)
        XCTAssertEqual(commands.count, 1, "An older coalesced move must not follow the scroll's new position")
        XCTAssertTrue(commands.first?.hasPrefix("POINTER scroll:1@") == true)
        view.refreshFrame(at: 2)
        XCTAssertEqual(try String(contentsOf: root.appendingPathComponent("input.ctl"))
            .split(separator: "\n").count, 1)
        // A move arriving after the scroll remains eligible for the next frame.
        view.mouseMoved(with: move)
        view.refreshFrame(at: 3)
        let later = try String(contentsOf: root.appendingPathComponent("input.ctl"))
            .split(separator: "\n").map(String.init)
        XCTAssertEqual(later.count, 2)
        XCTAssertTrue(later.last?.hasPrefix("POINTER move:") == true)
    }

    private func put<T: FixedWidthInteger>(_ value: T, at offset: Int, into data: inout Data) {
        var little = value.littleEndian
        withUnsafeBytes(of: &little) { data.replaceSubrange(offset..<(offset + $0.count), with: $0) }
    }
}
