import AppKit
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfFramebufferFocusAdmissionTests: XCTestCase {
    func testResignationCancelsPreeditBeforeImmediateUnmarkCallback() throws {
        try checkUnmark(afterResigningCurrentWindow: true)
    }

    func testOtherWindowResignationLeavesCurrentPreeditCommittable() throws {
        try checkUnmark(afterResigningCurrentWindow: false)
    }

    func testNilWindowStopsThePreviousObservation() {
        let window = NSWindow(contentRect: .zero, styleMask: [], backing: .buffered, defer: true)
        let monitor = HvfInputFocusMonitor()
        var calls = 0
        monitor.watch(window: window) { calls += 1 }
        monitor.watch(window: nil) { calls += 100 }
        NotificationCenter.default.post(name: NSWindow.didResignKeyNotification, object: window)
        XCTAssertEqual(calls, 0)
    }

    func testWindowReplacementAndReentrantStopDoNotCancelUnrelatedTargets() {
        let first = NSWindow(contentRect: .zero, styleMask: [], backing: .buffered, defer: true)
        let second = NSWindow(contentRect: .zero, styleMask: [], backing: .buffered, defer: true)
        let monitor = HvfInputFocusMonitor()
        var calls = 0
        monitor.watch(window: first) { calls += 100 }
        monitor.watch(window: second) { calls += 1; monitor.stop() }
        NotificationCenter.default.post(name: NSWindow.didResignKeyNotification, object: first)
        XCTAssertEqual(calls, 0)
        NotificationCenter.default.post(name: NSWindow.didResignKeyNotification, object: second)
        XCTAssertEqual(calls, 1)
        NotificationCenter.default.post(name: NSWindow.didResignKeyNotification, object: second)
        XCTAssertEqual(calls, 1)
    }

    private func checkUnmark(afterResigningCurrentWindow sameWindow: Bool) throws {
        let fixture = try HvfRuntimeKeyboardDraftFixture()
        defer { fixture.remove() }
        try fixture.withSession { session in
            try fixture.negotiateOrderedInput(session)
            var frame = Data(count: 68)
            for (offset, value): (Int, UInt32) in [(0, 0x42564642), (8, 1), (12, 1), (16, 4), (24, 2)] {
                var little = value.littleEndian
                withUnsafeBytes(of: &little) { frame.replaceSubrange(offset..<(offset + 4), with: $0) }
            }
            try frame.write(to: fixture.evidence.appendingPathComponent("display.fb"))
            let view = FBLayerView(session: session)
            let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 320, height: 200),
                styleMask: [], backing: .buffered, defer: true)
            let other = NSWindow(contentRect: .zero, styleMask: [], backing: .buffered, defer: true)
            window.contentView = view
            defer { view.teardown(); window.contentView = nil }
            view.refreshFrame(at: 0)
            XCTAssertTrue(view.guestInputAvailable)
            view.setMarkedText("한", selectedRange: NSRange(location: 1, length: 0),
                replacementRange: NSRange(location: NSNotFound, length: 0))
            XCTAssertTrue(view.hasMarkedText())
            let target = session.textInputTarget
            let before = try fixture.commands()

            // The notification is synchronously delivered on the main thread.
            // Do not yield: a following AppKit callback must not beat cancellation.
            NotificationCenter.default.post(name: NSWindow.didResignKeyNotification,
                object: sameWindow ? window : other)
            if sameWindow {
                XCTAssertFalse(view.hasMarkedText(), "Known focus loss must invalidate local preedit immediately")
                XCTAssertNotEqual(session.textInputTarget, target)
            } else {
                XCTAssertTrue(view.hasMarkedText())
                XCTAssertEqual(session.textInputTarget, target)
            }
            view.unmarkText()
            let after = try fixture.commands()
            if sameWindow {
                XCTAssertEqual(after, before, "A canceled preedit must not emit a TEXTINPUT command")
            } else {
                XCTAssertEqual(after.count, before.count + 1)
                XCTAssertTrue(after.last?.hasPrefix("TEXTINPUT ") == true)
            }
            XCTAssertNil(try fixture.bytes(fixture.input))
        }
    }
}
