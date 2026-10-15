#if canImport(AppKit)
import AppKit
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfFramebufferCompositionBoundaryTests: XCTestCase {
    func testSessionReplacementDiscardsOldCompositionAndSameSessionPreservesIt() throws {
        let fixture = try Fixture()
        defer { fixture.finish() }
        let view = try XCTUnwrap(fixture.view)
        mark(view)
        view.updateSession(fixture.session)
        XCTAssertTrue(view.hasMarkedText())
        let replacement = fixture.makeSession()
        view.updateSession(replacement)
        XCTAssertFalse(view.hasMarkedText())
        view.unmarkText()
        XCTAssertFalse(fixture.hasGuestCommands)
    }

    func testWindowResignationAndTeardownDiscardWithoutGuestDelivery() async throws {
        let fixture = try Fixture()
        defer { fixture.finish() }
        let window = NSWindow(contentRect: fixture.view.bounds, styleMask: [], backing: .buffered, defer: true)
        window.contentView = fixture.view
        defer { window.contentView = nil }
        fixture.view.refreshFrame(at: 1)
        mark(fixture.view)
        NotificationCenter.default.post(name: NSWindow.didResignKeyNotification, object: window)
        for _ in 0..<10 { await Task.yield() }
        XCTAssertFalse(fixture.view.hasMarkedText())
        fixture.view.unmarkText()
        XCTAssertFalse(fixture.hasGuestCommands)
        mark(fixture.view)
        fixture.view.teardown()
        fixture.view.unmarkText()
        XCTAssertFalse(fixture.view.hasMarkedText())
        XCTAssertFalse(fixture.hasGuestCommands)
    }

    func testPointerTargetChangesCancelPreeditBeforeSendingPointer() throws {
        let fixture = try Fixture()
        defer { fixture.finish() }
        let view = try XCTUnwrap(fixture.view)
        for type: NSEvent.EventType in [.leftMouseDown, .rightMouseDown] {
            mark(view)
            let event = try XCTUnwrap(NSEvent.mouseEvent(with: type, location: NSPoint(x: 100, y: 50),
                modifierFlags: [], timestamp: 0, windowNumber: 0, context: nil, eventNumber: 0,
                clickCount: 1, pressure: 1))
            if type == .leftMouseDown { view.mouseDown(with: event) } else { view.rightMouseDown(with: event) }
            XCTAssertFalse(view.hasMarkedText())
            view.unmarkText()
        }
        let commands = try String(contentsOf: fixture.root.appendingPathComponent("input.ctl"))
        XCTAssertTrue(commands.contains("POINTER "))
        XCTAssertFalse(commands.contains("KEY "))
        XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.session.config.ctlFilePath))
    }

    func testMarkedOverlayProvidesCandidateAnchorAndDoesNotCapturePointer() throws {
        let fixture = try Fixture()
        defer { fixture.finish() }
        let window = NSWindow(contentRect: fixture.view.bounds, styleMask: [], backing: .buffered, defer: true)
        window.contentView = fixture.view
        defer { window.contentView = nil }
        fixture.view.refreshFrame(at: 1)
        mark(fixture.view)
        let overlay = try XCTUnwrap(fixture.view.subviews.first as? NSTextField)
        XCTAssertEqual(overlay.stringValue, "한")
        XCTAssertFalse(overlay.isHidden)
        XCTAssertNil(overlay.hitTest(.zero))
        var actual = NSRange(location: NSNotFound, length: 0)
        let anchor = fixture.view.firstRect(forCharacterRange: NSRange(location: 0, length: 1), actualRange: &actual)
        XCTAssertGreaterThan(anchor.height, 0)
        XCTAssertEqual(actual, NSRange(location: 0, length: 1))
        let caret = fixture.view.firstRect(forCharacterRange: NSRange(location: 1, length: 0), actualRange: &actual)
        XCTAssertEqual(actual, NSRange(location: 1, length: 0))
        XCTAssertEqual(caret.width, 0)
        XCTAssertGreaterThan(caret.minX, anchor.minX)
        fixture.view.cancelTextComposition()
        XCTAssertTrue(overlay.isHidden)
    }

    func testSameSessionGuestResetRejectsCommitBeforeViewUpdate() throws {
        let fixture = try Fixture()
        defer { fixture.finish() }
        mark(fixture.view)
        let previous = fixture.session.textInputTarget
        try Data((HvfGuestResetRecord.processRecreation + "\n").utf8).write(to: fixture.root.appendingPathComponent("run.log"))
        fixture.session.poll()
        XCTAssertNotEqual(fixture.session.textInputTarget, previous)
        fixture.view.insertText("한", replacementRange: NSRange(location: NSNotFound, length: 0))
        XCTAssertFalse(fixture.view.hasMarkedText())
        XCTAssertFalse(fixture.hasGuestCommands)
    }

    func testSameSessionBootAndConfigChangeRejectPendingCommit() throws {
        let fixture = try Fixture()
        defer { fixture.finish() }
        mark(fixture.view)
        fixture.session.beginOwnedInputBoot()
        fixture.view.unmarkText()
        XCTAssertFalse(fixture.view.hasMarkedText())
        XCTAssertFalse(fixture.hasGuestCommands)
        mark(fixture.view)
        fixture.session.config.targetDiskPath = "different-disk"
        fixture.view.insertText("한", replacementRange: NSRange(location: NSNotFound, length: 0))
        XCTAssertFalse(fixture.view.hasMarkedText())
        XCTAssertFalse(fixture.hasGuestCommands)
    }

    func testDisplayTickClearsPreeditAfterInputTargetCancellation() throws {
        let fixture = try Fixture()
        defer { fixture.finish() }
        mark(fixture.view)
        fixture.session.cancelOrderedInputTarget()
        fixture.view.refreshFrame(at: 1)
        XCTAssertFalse(fixture.view.hasMarkedText())
        let overlay = try XCTUnwrap(fixture.view.subviews.first as? NSTextField)
        XCTAssertTrue(overlay.isHidden)
        XCTAssertFalse(fixture.hasGuestCommands)
    }

    private func mark(_ view: FBLayerView) {
        view.setMarkedText("한", selectedRange: NSRange(location: 1, length: 0),
            replacementRange: NSRange(location: NSNotFound, length: 0))
        XCTAssertTrue(view.hasMarkedText())
    }

    @MainActor private final class Fixture {
        let root = LibraryDeletionTestDirectory.temporary.appendingPathComponent("ime-boundary-" + UUID().uuidString)
        var session: HvfEngineSession!
        var view: FBLayerView!
        var hasGuestCommands: Bool {
            ["input.ctl", "agent.ctl"].contains { FileManager.default.fileExists(atPath: root.appendingPathComponent($0).path) }
        }
        init() throws {
            try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
            var frame = Data(count: 68)
            for (offset, value): (Int, UInt32) in [(0, 0x42564642), (8, 1), (12, 1), (16, 4), (24, 2)] {
                var little = value.littleEndian
                withUnsafeBytes(of: &little) { frame.replaceSubrange(offset..<(offset + 4), with: $0) }
            }
            try frame.write(to: root.appendingPathComponent("display.fb"))
            session = makeSession()
            view = FBLayerView(session: session)
            view.frame = NSRect(x: 0, y: 0, width: 320, height: 200)
            view.refreshFrame(at: 0)
        }
        func makeSession() -> HvfEngineSession {
            let config = HvfEngineConfig(targetDiskPath: "disk", uefiVarsPath: "vars", evidenceDir: root.path,
                watchdogMs: nil, ramMiB: 6144, smpCpus: 4, clipboardSync: false, shareHostDir: nil,
                shareGuestDir: nil, virtioNet: false, virtioGpu3d: false, nvmeBufferedIO: false,
                ctlFilePath: root.appendingPathComponent("agent.ctl").path)
            return HvfEngineSession(config: config, repoRoot: root) { _ in false }
        }
        func finish() { view.teardown(); try? FileManager.default.removeItem(at: root) }
    }
}
#endif
