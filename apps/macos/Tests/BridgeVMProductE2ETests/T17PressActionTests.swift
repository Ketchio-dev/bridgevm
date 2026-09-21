import ApplicationServices
import XCTest
@testable import BridgeVMProductE2E

final class T17PressActionTests: XCTestCase {
    private struct ReadFailure: Error {}

    func testEnabledControlPressesOnceWithoutActivation() throws {
        var presses = 0, activations = 0
        try T17PressAction.perform(identifier: "start", timeout: 1,
            enabled: { true }, press: { presses += 1; return .success },
            activate: { activations += 1; return true }, retry: { .success },
            frontmost: { true }, pause: {})
        XCTAssertEqual(presses, 1)
        XCTAssertEqual(activations, 0)
    }

    func testDisabledControlWaitsUntilEnabledBeforePressing() throws {
        var states = [false, false, true].makeIterator(), presses = 0, pauses = 0
        try T17PressAction.perform(identifier: "start", timeout: 1,
            enabled: { states.next()! }, press: { presses += 1; return .success },
            activate: { true }, retry: { .success }, frontmost: { true },
            now: { Date(timeIntervalSince1970: 0) }, pause: { pauses += 1 })
        XCTAssertEqual(presses, 1)
        XCTAssertEqual(pauses, 2)
    }

    func testDisabledControlFailsWithoutCallingAXPress() {
        var times = [Date(timeIntervalSince1970: 0), Date(timeIntervalSince1970: 1)].makeIterator()
        var presses = 0
        XCTAssertThrowsError(try T17PressAction.perform(identifier: "start", timeout: 0.5,
            enabled: { false }, press: { presses += 1; return .success },
            activate: { true }, retry: { .success }, frontmost: { true },
            now: { times.next()! }, pause: {})) { error in
            XCTAssertEqual(error as? T17Blocker, T17Blocker(code: "ui-element-missing",
                detail: "identified UI element remained disabled: start; timeout_s=0.5"))
        }
        XCTAssertEqual(presses, 0)
    }

    func testMissingEnabledValueFailsClosed() {
        XCTAssertThrowsError(try T17PressAction.perform(identifier: "start", timeout: 1,
            enabled: { nil }, press: { XCTFail("must not press"); return .success },
            activate: { true }, retry: { .success }, frontmost: { true }, pause: {})) { error in
            XCTAssertEqual(error as? T17Blocker, T17Blocker(code: "ui-element-missing",
                detail: "identified UI element has no AXEnabled value: start"))
        }
    }

    func testEnabledReadFailurePropagatesWithoutPressing() {
        XCTAssertThrowsError(try T17PressAction.perform(identifier: "start", timeout: 1,
            enabled: { throw ReadFailure() },
            press: { XCTFail("must not press"); return .success }, activate: { true },
            retry: { .success }, frontmost: { true }, pause: {})) { error in
            XCTAssertTrue(error is ReadFailure)
        }
    }

    func testFailedPressRetriesOnlyAfterSuccessfulActivation() throws {
        var retries = 0
        try T17PressAction.perform(identifier: "start", timeout: 1,
            enabled: { true }, press: { .cannotComplete }, activate: { true },
            retry: { retries += 1; return .success }, frontmost: { true }, pause: {})
        XCTAssertEqual(retries, 1)
    }

    func testFailedActivationRetainsExactAXDiagnostic() {
        XCTAssertThrowsError(try T17PressAction.perform(identifier: "start", timeout: 1,
            enabled: { true }, press: { .cannotComplete }, activate: { false },
            retry: { XCTFail("must not retry"); return .success },
            frontmost: { false }, pause: {})) { error in
            let detail = (error as? T17Blocker)?.detail ?? ""
            XCTAssertTrue(detail.contains("first_ax_error=\(AXError.cannotComplete.rawValue)"))
            XCTAssertTrue(detail.contains("retry_ax_error=not-attempted"))
            XCTAssertTrue(detail.contains("activation_succeeded=false; frontmost=false"))
        }
    }
}
