import ApplicationServices
import XCTest
@testable import BridgeVMProductE2E

final class T17PressActionDeadlineTests: XCTestCase {
    func testLateEnabledReadDoesNotPressOrActivate() {
        for elapsed in [1.0, 2.0] {
            var clock = Date(timeIntervalSince1970: 0), presses = 0, activations = 0
            XCTAssertThrowsError(try T17PressAction.perform(identifier: "start", timeout: 1,
                enabled: { clock = Date(timeIntervalSince1970: elapsed); return true },
                press: { presses += 1; return .success }, activate: { activations += 1; return true },
                retry: { XCTFail("late read retried"); return .success }, frontmost: { true },
                now: { clock }, pause: {})) { error in
                    XCTAssertEqual(error as? T17Blocker, T17Blocker(code: "ui-element-missing",
                        detail: "identified UI element readiness timed out: start; timeout_s=1.0"))
                }
            XCTAssertEqual(presses, 0); XCTAssertEqual(activations, 0)
        }
    }

    func testEnabledReadBeforeDeadlineStillPressesOnce() throws {
        var clock = Date(timeIntervalSince1970: 0), presses = 0
        try T17PressAction.perform(identifier: "start", timeout: 1,
            enabled: { clock = Date(timeIntervalSince1970: 0.9); return true },
            press: { presses += 1; return .success }, activate: { XCTFail(); return true },
            retry: { XCTFail(); return .success }, frontmost: { true }, now: { clock }, pause: {})
        XCTAssertEqual(presses, 1)
    }

    func testRetryPauseReachingDeadlineDoesNotSendAnotherPress() {
        for elapsed in [1.0, 2.0] {
            var clock = Date(timeIntervalSince1970: 0), presses = 0, retries = 0
            XCTAssertThrowsError(try T17PressAction.perform(identifier: "start", timeout: 1,
                enabled: { true }, press: { presses += 1; return .cannotComplete }, activate: { true },
                retry: { retries += 1; return .success }, frontmost: { true }, now: { clock },
                pause: { clock = Date(timeIntervalSince1970: elapsed) })) { error in
                    self.assertUnretriedFailure(error)
                }
            XCTAssertEqual(presses, 1); XCTAssertEqual(retries, 0)
        }
    }

    func testActivationReachingDeadlineDoesNotSendAnotherPress() {
        var clock = Date(timeIntervalSince1970: 0), retries = 0
        XCTAssertThrowsError(try T17PressAction.perform(identifier: "start", timeout: 1,
            enabled: { true }, press: { .cannotComplete },
            activate: { clock = Date(timeIntervalSince1970: 1); return true },
            retry: { retries += 1; return .success }, frontmost: { true }, now: { clock }, pause: {})) {
                self.assertUnretriedFailure($0)
            }
        XCTAssertEqual(retries, 0)
    }

    private func assertUnretriedFailure(_ error: Error) {
        XCTAssertEqual((error as? T17Blocker)?.code, "ui-element-missing")
        XCTAssertEqual((error as? T17Blocker)?.detail,
            "AXPress failed: start; first_ax_error=\(AXError.cannotComplete.rawValue); retry_ax_error=not-attempted; activation_succeeded=true; frontmost=true; attempts=1")
    }
}
