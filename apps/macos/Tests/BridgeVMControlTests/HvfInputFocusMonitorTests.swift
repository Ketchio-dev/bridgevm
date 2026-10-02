#if canImport(AppKit)
import AppKit
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfInputFocusMonitorTests: XCTestCase {
    func testStoppedMonitorDiscardsAlreadyQueuedNotification() async {
        let window = NSWindow(contentRect: .zero, styleMask: [], backing: .buffered, defer: true)
        let monitor = HvfInputFocusMonitor()
        var calls = 0
        monitor.watch(window: window) { calls += 1 }
        NotificationCenter.default.post(name: NSWindow.didResignKeyNotification, object: window)
        monitor.stop()
        for _ in 0..<10 { await Task.yield() }
        XCTAssertEqual(calls, 0)
    }

    func testReplacementReceivesOnlyItsOwnNotifications() async {
        let window = NSWindow(contentRect: .zero, styleMask: [], backing: .buffered, defer: true)
        let monitor = HvfInputFocusMonitor()
        var old = 0, current = 0
        monitor.watch(window: window) { old += 1 }
        NotificationCenter.default.post(name: NSWindow.didResignKeyNotification, object: window)
        monitor.watch(window: window) { current += 1 }
        NotificationCenter.default.post(name: NSWindow.didResignKeyNotification, object: window)
        for _ in 0..<10 { await Task.yield() }
        XCTAssertEqual(old, 0)
        XCTAssertEqual(current, 1)
        monitor.stop()
    }
}
#endif
