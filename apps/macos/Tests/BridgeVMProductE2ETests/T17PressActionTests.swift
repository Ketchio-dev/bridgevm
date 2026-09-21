import ApplicationServices
import XCTest
@testable import BridgeVMProductE2E

final class T17PressActionTests: XCTestCase {
    private struct ReadFailure: Error {}
    func testTargetRoleIsExactButton() { XCTAssertEqual(T17PressAction.targetRole, kAXButtonRole as String) }

    func testEnabledControlPressesOnceWithoutActivation() throws {
        var presses = 0, activations = 0
        try T17PressAction.perform(identifier: "start", timeout: 1, enabled: { true },
            press: { presses += 1; return .success }, activate: { activations += 1; return true },
            retry: { .success }, frontmost: { true }, pause: {})
        XCTAssertEqual(presses, 1); XCTAssertEqual(activations, 0)
    }

    func testDisabledControlWaitsUntilEnabledBeforePressing() throws {
        var states = [false, false, true].makeIterator(), presses = 0, pauses = 0
        try T17PressAction.perform(identifier: "start", timeout: 1, enabled: { states.next()! },
            press: { presses += 1; return .success }, activate: { true }, retry: { .success },
            frontmost: { true }, now: { Date(timeIntervalSince1970: 0) }, pause: { pauses += 1 })
        XCTAssertEqual(presses, 1); XCTAssertEqual(pauses, 2)
    }

    func testDisabledControlTimesOutWithoutPressing() {
        var times = [Date(timeIntervalSince1970: 0), Date(timeIntervalSince1970: 1)].makeIterator(), presses = 0
        XCTAssertThrowsError(try T17PressAction.perform(identifier: "start", timeout: 0.5,
            enabled: { false }, press: { presses += 1; return .success }, activate: { true },
            retry: { .success }, frontmost: { true }, now: { times.next()! }, pause: {})) { error in
            XCTAssertTrue((error as? T17Blocker)?.detail.contains("remained disabled") == true)
        }
        XCTAssertEqual(presses, 0)
    }

    func testMissingOrFailedEnabledReadFailsClosed() {
        XCTAssertThrowsError(try T17PressAction.perform(identifier: "start", timeout: 1, enabled: { nil },
            press: { XCTFail(); return .success }, activate: { true }, retry: { .success }, frontmost: { true }, pause: {}))
        XCTAssertThrowsError(try T17PressAction.perform(identifier: "start", timeout: 1,
            enabled: { throw ReadFailure() }, press: { XCTFail(); return .success }, activate: { true },
            retry: { .success }, frontmost: { true }, pause: {})) { XCTAssertTrue($0 is ReadFailure) }
    }

    func testCannotCompleteRetriesAfterPauseUntilSuccess() throws {
        var results = [AXError.cannotComplete, .success].makeIterator(), retries = 0, pauses = 0
        try T17PressAction.perform(identifier: "start", timeout: 1, enabled: { true },
            press: { .cannotComplete }, activate: { true },
            retry: { retries += 1; return results.next()! }, frontmost: { true },
            now: { Date(timeIntervalSince1970: 0) }, pause: { pauses += 1 })
        XCTAssertEqual(retries, 2); XCTAssertEqual(pauses, 2)
    }

    func testCannotCompleteExhaustionStopsAtDeadline() {
        var times = [Date(timeIntervalSince1970: 0), Date(timeIntervalSince1970: 1)].makeIterator(), retries = 0
        XCTAssertThrowsError(try T17PressAction.perform(identifier: "start", timeout: 0.5, enabled: { true }, press: { .cannotComplete }, activate: { true }, retry: { retries += 1; return .cannotComplete }, frontmost: { true }, now: { times.next()! }, pause: {}))
        XCTAssertEqual(retries, 1)
    }

    func testNonTransientErrorFailsWithoutActivationOrRetry() {
        var activations = 0, retries = 0
        XCTAssertThrowsError(try T17PressAction.perform(identifier: "start", timeout: 1,
            enabled: { true }, press: { .invalidUIElement }, activate: { activations += 1; return true },
            retry: { retries += 1; return .success }, frontmost: { true }, pause: {})) { error in
            let detail = (error as? T17Blocker)?.detail ?? ""
            XCTAssertTrue(detail.contains("activation_succeeded=not-attempted")); XCTAssertTrue(detail.contains("attempts=1"))
        }
        XCTAssertEqual(activations, 0); XCTAssertEqual(retries, 0)
    }

    func testFailedActivationRetainsExactAXDiagnostic() {
        XCTAssertThrowsError(try T17PressAction.perform(identifier: "start", timeout: 1,
            enabled: { true }, press: { .cannotComplete }, activate: { false },
            retry: { XCTFail(); return .success }, frontmost: { false }, pause: {})) { error in
            let detail = (error as? T17Blocker)?.detail ?? ""
            XCTAssertTrue(detail.contains("first_ax_error=\(AXError.cannotComplete.rawValue)"))
            XCTAssertTrue(detail.contains("retry_ax_error=not-attempted")); XCTAssertTrue(detail.contains("activation_succeeded=false; frontmost=false"))
        }
    }
}
