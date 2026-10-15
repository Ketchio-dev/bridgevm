#if canImport(AppKit)
import AppKit
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfInputFocusMonitorTests: XCTestCase {
    func testStoppedMonitorDoesNotReceiveLaterNotification() {
        let window = NSWindow(contentRect: .zero, styleMask: [], backing: .buffered, defer: true)
        let monitor = HvfInputFocusMonitor()
        var calls = 0
        monitor.watch(window: window) { calls += 1 }
        monitor.stop()
        NotificationCenter.default.post(name: NSWindow.didResignKeyNotification, object: window)
        XCTAssertEqual(calls, 0)
    }

    func testReplacementReceivesOnlyItsOwnNotifications() {
        let window = NSWindow(contentRect: .zero, styleMask: [], backing: .buffered, defer: true)
        let monitor = HvfInputFocusMonitor()
        var old = 0, current = 0
        monitor.watch(window: window) { old += 1 }
        NotificationCenter.default.post(name: NSWindow.didResignKeyNotification, object: window)
        monitor.watch(window: window) { current += 1 }
        NotificationCenter.default.post(name: NSWindow.didResignKeyNotification, object: window)
        XCTAssertEqual(old, 1, "The old target lost focus before the monitor was rebound")
        XCTAssertEqual(current, 1)
        monitor.stop()
    }
}
#endif
