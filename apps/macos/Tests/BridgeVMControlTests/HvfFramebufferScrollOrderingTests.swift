import AppKit
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfFramebufferScrollOrderingTests: XCTestCase {
    /// A synthetic AppKit scroll event; the view reads only these fields.
    private final class ScrollEvent: NSEvent {
        private let fakeDeltaY: CGFloat, fakePrecise: Bool, fakeMomentum: Bool
        init(deltaY: CGFloat = 1, precise: Bool = false, momentum: Bool = false) {
            fakeDeltaY = deltaY; fakePrecise = precise; fakeMomentum = momentum
            super.init()
        }
        required init?(coder: NSCoder) { nil }
        override var scrollingDeltaY: CGFloat { fakeDeltaY }
        override var scrollingDeltaX: CGFloat { 0 }
        override var hasPreciseScrollingDeltas: Bool { fakePrecise }
        override var phase: NSEvent.Phase { fakePrecise && !fakeMomentum ? .changed : [] }
        override var momentumPhase: NSEvent.Phase { fakeMomentum ? .changed : [] }
        override var locationInWindow: NSPoint { NSPoint(x: 75, y: 75) }
    }

    func testScrollDoesNotReplayAnOlderCoalescedMoveAfterItsOwnPosition() throws {
        try withView(guestPixels: 1) { view, root in
            let move = try XCTUnwrap(NSEvent.mouseEvent(with: .mouseMoved, location: NSPoint(x: 25, y: 25),
                modifierFlags: [], timestamp: 1, windowNumber: 0, context: nil, eventNumber: 0,
                clickCount: 0, pressure: 0))
            view.mouseMoved(with: move)
            view.scrollWheel(with: ScrollEvent())
            view.refreshFrame(at: 1)
            let commands = try Self.commands(root)
            XCTAssertEqual(commands.count, 1, "An older coalesced move must not follow the scroll's new position")
            XCTAssertTrue(commands.first?.hasPrefix("POINTER scroll:1@") == true)
            view.refreshFrame(at: 2)
            XCTAssertEqual(try Self.commands(root).count, 1)
            // A move arriving after the scroll remains eligible for the next frame.
            view.mouseMoved(with: move)
            view.refreshFrame(at: 3)
            let later = try Self.commands(root)
            XCTAssertEqual(later.count, 2)
            XCTAssertTrue(later.last?.hasPrefix("POINTER move:") == true)
        }
    }

    /// A gentle two-finger scroll is dozens of small point deltas plus a momentum tail. On the VMM's
    /// HID wheel it used to become one or more whole Windows notches per event (70 notches here).
    func testATrackpadGestureReachesTheVMMWheelAsProportionalNotches() throws {
        try withView(guestPixels: 200) { view, root in
            // 100-point view, 200-pixel guest: 2 guest pixels per point, so 40 x 1.5 points is 192 units
            // (one notch); 30 momentum deltas of 0.3 points add 29 units, not another notch.
            for _ in 0..<40 { view.scrollWheel(with: ScrollEvent(deltaY: -1.5, precise: true)) }
            for _ in 0..<30 { view.scrollWheel(with: ScrollEvent(deltaY: -0.3, precise: true, momentum: true)) }
            let commands = try Self.commands(root)
            XCTAssertEqual(commands.count, 1, "\(commands.count) notches for 120 points of trackpad travel")
            XCTAssertTrue(commands.first?.hasPrefix("POINTER scroll:-1@") == true, commands.first ?? "none")
        }
    }

    private func withView(guestPixels side: UInt32, _ body: (FBLayerView, URL) throws -> Void) throws {
        let root = LibraryDeletionTestDirectory.temporary.resolvingSymlinksInPath()
            .appendingPathComponent("scroll-order-" + UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
        defer { try? FileManager.default.removeItem(at: root) }
        var frame = Data(count: 64 + Int(side * side) * 4)
        put(UInt32(0x42564642), at: 0, into: &frame)
        put(side, at: 8, into: &frame); put(side, at: 12, into: &frame)
        put(side * 4, at: 16, into: &frame); put(UInt64(2), at: 24, into: &frame)
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
        try body(view, root)
    }

    private static func commands(_ root: URL) throws -> [String] {
        let input = root.appendingPathComponent("input.ctl")
        guard FileManager.default.fileExists(atPath: input.path) else { return [] }
        return try String(contentsOf: input).split(separator: "\n").map(String.init)
    }

    private func put<T: FixedWidthInteger>(_ value: T, at offset: Int, into data: inout Data) {
        var little = value.littleEndian
        withUnsafeBytes(of: &little) { data.replaceSubrange(offset..<(offset + $0.count), with: $0) }
    }
}
