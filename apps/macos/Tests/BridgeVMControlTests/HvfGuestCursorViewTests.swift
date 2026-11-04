#if canImport(AppKit)
import AppKit
import XCTest
@testable import BridgeVMControl

/// The display view with a guest hardware cursor. The view's cursor setter is replaced, so these tests
/// record what would be shown and never change the real macOS cursor.
@MainActor
final class HvfGuestCursorViewTests: XCTestCase {
    private var shown: [NSCursor] = []
    private let center = CGPoint(x: 50, y: 50)

    func testThePointerOnTheImageShowsTheGuestImageAtTheViewsScale() throws {
        try withView { view, _, _ in
            view.updateGuestCursor(pointer: center)
            let cursor = try XCTUnwrap(shown.last)
            XCTAssertEqual(shown.count, 1)
            XCTAssertEqual(cursor.image.size, NSSize(width: 32, height: 32), "64 guest pixels at half scale")
            XCTAssertEqual(cursor.hotSpot, NSPoint(x: 1, y: 2))
            view.updateGuestCursor(pointer: CGPoint(x: 52, y: 51))
            XCTAssertEqual(shown.count, 1, "no new cursor while it stays the guest's")
            XCTAssertTrue(view.guestCursor.spriteClip.isHidden)
            view.updateGuestCursor(pointer: CGPoint(x: 50, y: 10))
            XCTAssertTrue(shown.last === NSCursor.arrow, "the black bar gets the arrow back")
        }
    }

    func testAnInactiveWindowNeverChangesTheCursorAndDrawsTheSpriteInstead() throws {
        try withView { view, _, _ in
            view.windowIsKey = { false }
            view.mouseMoved(with: try Self.event(.mouseMoved))
            view.refreshFrame(at: 1)
            XCTAssertTrue(shown.isEmpty)
            XCTAssertFalse(view.guestCursor.spriteClip.isHidden)
            XCTAssertTrue(view.guestCursor.spriteClip.superlayer === view.layer)
            XCTAssertEqual(view.guestCursor.spriteClip.frame, CGRect(x: 0, y: 25, width: 100, height: 50),
                           "clipped to the VM image")
            XCTAssertEqual(view.guestCursor.sprite.frame, CGRect(x: 50, y: 25, width: 32, height: 32),
                           "top left at the guest position (100, 50), half scale")
        }
    }

    func testEveryWayOutOfTheImageRestoresTheArrowOnce() throws {
        let exits: [(String, (FBLayerView, HvfEngineSession, URL) throws -> Void)] = [
            ("pointer leaves the view", { view, _, _ in view.mouseExited(with: try Self.event(.mouseExited)) }),
            ("window resigns key", { view, _, _ in
                view.windowIsKey = { false }
                view.windowResignedKey()
            }),
            ("display goes away", { view, _, cursorPath in
                let frame = cursorPath.deletingLastPathComponent().appendingPathComponent("display.fb")
                try Data(count: 68).write(to: frame)
                view.refreshFrame(at: 1)
            }),
            ("VM stops", { view, session, _ in
                session.connectionState = .stopped
                view.refreshFrame(at: 1)
            }),
            ("view leaves its window", { view, _, _ in view.viewDidMoveToWindow() }),
            ("view torn down", { view, _, _ in view.teardown() }),
        ]
        for (label, exit) in exits {
            shown = []
            try withView { view, session, cursorPath in
                view.updateGuestCursor(pointer: center)
                XCTAssertEqual(shown.count, 1, label)
                try exit(view, session, cursorPath)
                XCTAssertEqual(shown.count, 2, label)
                XCTAssertTrue(shown.last === NSCursor.arrow, label)
                view.updateGuestCursor(pointer: nil)
                XCTAssertEqual(shown.count, 2, "\(label): the arrow is set once, then left alone")
            }
        }
    }

    func testAHiddenSpriteGivesTheArrowAndANewShapeComesBack() throws {
        try withView { view, _, cursorPath in
            let pointer = center
            view.hostPointer = { pointer }
            view.refreshFrame(at: 1)
            XCTAssertEqual(shown.count, 1)
            try HvfGuestCursorFixture(serial: 1, visible: false, sequence: 4).write(to: cursorPath)
            view.refreshFrame(at: 2)
            XCTAssertTrue(shown.last === NSCursor.arrow, "Windows draws the pointer itself: the macOS arrow")
            try HvfGuestCursorFixture(serial: 2, visible: true, sequence: 6, pixel: 0x11).write(to: cursorPath)
            view.refreshFrame(at: 3)
            XCTAssertEqual(shown.count, 3)
            XCTAssertFalse(shown.last === NSCursor.arrow)
        }
    }

    func testWithoutCursorCommandsNothingChanges() throws {
        try withView(cursor: nil) { view, _, _ in
            let pointer = center
            view.hostPointer = { pointer }
            view.mouseMoved(with: try Self.event(.mouseMoved))
            view.refreshFrame(at: 1)
            view.mouseExited(with: try Self.event(.mouseExited))
            XCTAssertTrue(shown.isEmpty)
            XCTAssertTrue(view.guestCursor.spriteClip.isHidden)
        }
    }

    /// A 200x100 guest frame in a 100x100 view (the image is 100x50 at y 25, half scale) of a running VM,
    /// with display.cursor showing a 64x64 sprite with its top left at (100, 50), hot spot (2, 4).
    private func withView(cursor: HvfGuestCursorFixture? = HvfGuestCursorFixture(),
                          _ body: (FBLayerView, HvfEngineSession, URL) throws -> Void) throws {
        let root = LibraryDeletionTestDirectory.temporary.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        var frame = Data(count: 64 + 200 * 100 * 4)
        for (offset, value): (Int, UInt32) in [(0, 0x4256_4642), (4, 1), (8, 200), (12, 100), (16, 800)] {
            withUnsafeBytes(of: value.littleEndian) { frame.replaceSubrange(offset..<(offset + 4), with: $0) }
        }
        withUnsafeBytes(of: UInt64(2).littleEndian) { frame.replaceSubrange(24..<32, with: $0) }
        try frame.write(to: root.appendingPathComponent("display.fb"))
        let cursorPath = root.appendingPathComponent("display.cursor")
        try cursor?.write(to: cursorPath)
        let config = HvfEngineConfig(targetDiskPath: "target", uefiVarsPath: "vars", evidenceDir: root.path,
            watchdogMs: nil, ramMiB: 6144, smpCpus: 4, clipboardSync: false, shareHostDir: nil,
            shareGuestDir: nil, virtioNet: false, virtioGpu3d: false, nvmeBufferedIO: true,
            ctlFilePath: root.appendingPathComponent("agent.ctl").path)
        let session = HvfEngineSession(config: config, repoRoot: root) { _ in false }
        session.connectionState = .connected(host: "guest")
        let view = FBLayerView(session: session)
        view.frame = NSRect(x: 0, y: 0, width: 100, height: 100)
        view.windowIsKey = { true }
        view.hostPointer = { nil }
        view.guestCursor.setCursor = { [weak self] in self?.shown.append($0) }
        view.guestCursor.currentCursor = { [weak self] in self?.shown.last ?? NSCursor.arrow }
        defer { view.teardown() }
        view.refreshFrame(at: 0)
        try body(view, session, cursorPath)
        withExtendedLifetime(session) {}
    }

    private static func event(_ type: NSEvent.EventType) throws -> NSEvent {
        if type == .mouseExited || type == .mouseEntered {
            return try XCTUnwrap(NSEvent.enterExitEvent(with: type, location: CGPoint(x: 50, y: 50),
                modifierFlags: [], timestamp: 1, windowNumber: 0, context: nil, eventNumber: 0,
                trackingNumber: 0, userData: nil))
        }
        return try XCTUnwrap(NSEvent.mouseEvent(with: type, location: CGPoint(x: 50, y: 50),
            modifierFlags: [], timestamp: 1, windowNumber: 0, context: nil,
            eventNumber: 0, clickCount: 0, pressure: 0))
    }
}
#endif
