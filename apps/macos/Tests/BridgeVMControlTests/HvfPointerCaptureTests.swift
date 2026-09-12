#if canImport(AppKit)
import AppKit
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfPointerCaptureTests: XCTestCase {
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
