#if canImport(AppKit)
import AppKit
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfPointerCaptureTests: XCTestCase {
    func testReleasingOneButtonKeepsTheOthersCaptured() {
        let capture = HvfPointerCapture(center: NotificationCenter())
        var calls: [Int] = []
        capture.arm(window: nil, button: 0) { calls.append(0) }
        capture.arm(window: nil, button: 1) { calls.append(1) }
        capture.arm(window: nil, button: 2) { calls.append(2) }
        capture.disarm(button: 0)
        capture.cancel()
        XCTAssertEqual(calls, [1, 2])
    }
    func testCancellationEmitsLatestReleaseExactlyOnce() {
        let capture = HvfPointerCapture(center: NotificationCenter())
        var calls: [Int] = []
        capture.arm(window: nil) { calls.append(1) }
        capture.updateRelease { calls.append(2) }
        capture.cancel(); capture.cancel()
        XCTAssertEqual(calls, [2])
    }

    func testNormalDisarmDoesNotEmitOrRearmOnMove() {
        let capture = HvfPointerCapture(center: NotificationCenter())
        var calls = 0
        capture.arm(window: nil) { calls += 1 }
        capture.disarm()
        capture.updateRelease { calls += 1 }
        capture.cancel()
        XCTAssertEqual(calls, 0)
    }

    func testOnlyCapturedWindowResignationCancels() {
        let center = NotificationCenter()
        let capture = HvfPointerCapture(center: center)
        let first = NSObject(), second = NSObject()
        var calls: [Int] = []
        capture.arm(window: first) { calls.append(1) }
        capture.arm(window: second) { calls.append(2) }
        center.post(name: NSWindow.didResignKeyNotification, object: first)
        XCTAssertTrue(calls.isEmpty)
        center.post(name: NSWindow.didResignKeyNotification, object: second)
        center.post(name: NSWindow.didResignKeyNotification, object: second)
        XCTAssertEqual(calls, [2])
    }

    func testReleaseCanRearmWithoutBeingDiscarded() {
        let capture = HvfPointerCapture(center: NotificationCenter())
        var calls: [Int] = []
        capture.arm(window: nil) {
            calls.append(1)
            capture.arm(window: nil) { calls.append(2) }
        }
        capture.cancel(); capture.cancel(); capture.cancel()
        XCTAssertEqual(calls, [1, 2])
    }
}
#endif
