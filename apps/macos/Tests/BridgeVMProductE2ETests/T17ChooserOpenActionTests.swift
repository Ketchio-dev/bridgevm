import ApplicationServices
import XCTest
@testable import BridgeVMProductE2E

final class T17ChooserOpenActionTests: XCTestCase {
    private struct ReadFailure: Error {}

    func testTargetRoleIsExactButton() {
        XCTAssertEqual(T17ChooserOpenAction.targetRole, kAXButtonRole as String)
    }

    func testSuccessAndCannotCompleteAreOnlyProvisionalResults() throws {
        for result in [AXError.success, .cannotComplete] {
            XCTAssertEqual(try T17ChooserOpenAction.perform(identifier: "chooser", timeout: 1,
                enabled: { true }, press: { result }, pause: {}), result)
        }
    }

    func testCannotCompleteWithoutExactPanelStillFailsChooser() {
        let driver = ProvisionalDriver()
        XCTAssertThrowsError(try T17FileChooser.choose(path: "/private/share", timeout: 0.1,
            driver: driver, now: { driver.clock }, pause: { driver.clock += 0.1 }))
        XCTAssertEqual(driver.trigger, .cannotComplete)
        XCTAssertFalse(driver.touchedPath)
    }

    func testDisabledControlWaitsThenPressesOnce() throws {
        var states = [false, false, true].makeIterator(), presses = 0, pauses = 0
        let result = try T17ChooserOpenAction.perform(identifier: "chooser", timeout: 1,
            enabled: { states.next()! }, press: { presses += 1; return .cannotComplete },
            now: { Date(timeIntervalSince1970: 0) }, pause: { pauses += 1 })
        XCTAssertEqual(result, .cannotComplete); XCTAssertEqual(presses, 1); XCTAssertEqual(pauses, 2)
    }

    func testMissingOrFailedEnabledReadFailsWithoutPressing() {
        var presses = 0
        XCTAssertThrowsError(try T17ChooserOpenAction.perform(identifier: "chooser", timeout: 1,
            enabled: { nil }, press: { presses += 1; return .success }, pause: {}))
        XCTAssertThrowsError(try T17ChooserOpenAction.perform(identifier: "chooser", timeout: 1,
            enabled: { throw ReadFailure() }, press: { presses += 1; return .success }, pause: {})) {
                XCTAssertTrue($0 is ReadFailure)
            }
        XCTAssertEqual(presses, 0)
    }

    func testDisabledControlTimesOutWithoutPressing() {
        var times = [Date(timeIntervalSince1970: 0), Date(timeIntervalSince1970: 1)].makeIterator(), presses = 0
        XCTAssertThrowsError(try T17ChooserOpenAction.perform(identifier: "chooser", timeout: 0.5,
            enabled: { false }, press: { presses += 1; return .success }, now: { times.next()! }, pause: {})) {
                XCTAssertTrue(($0 as? T17Blocker)?.detail.contains("remained disabled") == true)
            }
        XCTAssertEqual(presses, 0)
    }

    func testEveryOtherAXErrorFailsImmediately() {
        for result in [AXError.failure, .illegalArgument, .invalidUIElement, .actionUnsupported] {
            var presses = 0
            XCTAssertThrowsError(try T17ChooserOpenAction.perform(identifier: "chooser", timeout: 1,
                enabled: { true }, press: { presses += 1; return result }, pause: {})) {
                    let detail = ($0 as? T17Blocker)?.detail ?? ""
                    XCTAssertTrue(detail.contains("ax_error=\(result.rawValue)"))
                }
            XCTAssertEqual(presses, 1)
        }
    }

    private final class ProvisionalDriver: T17FileChooserDriving {
        var clock = 0.0, touchedPath = false
        var trigger: AXError?
        var failureContext: String { "open_ax_result=\(trigger?.rawValue ?? 0)" }
        func open() throws {
            trigger = try T17ChooserOpenAction.perform(identifier: "chooser", timeout: 1,
                enabled: { true }, press: { .cannotComplete }, pause: {})
        }
        func panelIsPresent() -> Bool { false }
        func showLocationField() { XCTFail() }
        func locationFieldIsReady() -> Bool { XCTFail(); return false }
        func setLocation(_ path: String) { touchedPath = true; XCTFail() }
        func acceptLocation() { XCTFail() }
        func locationFieldIsAbsent() -> Bool { XCTFail(); return false }
        func selectionIsReady() -> Bool { XCTFail(); return false }
        func acceptSelection() { XCTFail() }
        func selectedPath() -> String? { XCTFail(); return nil }
    }
}
