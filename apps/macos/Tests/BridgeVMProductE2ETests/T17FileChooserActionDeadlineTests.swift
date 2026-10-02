import XCTest
@testable import BridgeVMProductE2E

final class T17FileChooserActionDeadlineTests: XCTestCase {
    func testSlowInitialInventoryDoesNotOpenTheChooser() {
        let driver = Driver(); driver.slow = "initial-panel-check"
        fails(driver, at: "open-control")
        XCTAssertEqual(driver.actions, [])
    }

    func testSlowPathWriteDoesNotSendTheReturnKey() {
        let driver = Driver(); driver.slow = "set-location"
        fails(driver, at: "accept-location")
        XCTAssertEqual(driver.actions, ["open", "show-location", "set-location"])
    }

    func testActionCompletingAtDeadlineDoesNotStartAnotherStage() {
        let driver = Driver(); driver.slow = "show-location"
        fails(driver, at: "location-field-ready")
        XCTAssertEqual(driver.actions, ["open", "show-location"])
        XCTAssertEqual(driver.fieldReads, 0)
    }

    func testSlowReadinessReadDoesNotConfirmSelection() {
        let driver = Driver(); driver.slow = "selection-ready"
        fails(driver, at: "selection-ready", before: false)
        XCTAssertFalse(driver.actions.contains("accept-selection"))
    }

    private func fails(_ driver: Driver, at stage: String, before: Bool = true) {
        XCTAssertThrowsError(try T17FileChooser.choose(path: "/private/fixture.iso", timeout: 1,
            driver: driver, now: { driver.time }, pause: { driver.time += 0.1 })) { error in
                let blocker = error as? T17Blocker
                XCTAssertEqual(blocker?.code, "input-selection-failed")
                XCTAssertTrue(blocker?.detail.hasPrefix("stage=\(stage); timed out \(before ? "before chooser stage" : "waiting")") == true)
                XCTAssertTrue(blocker?.detail.hasSuffix("; ax=retained") == true)
            }
    }

    private final class Driver: T17FileChooserDriving {
        var time = 0.0, slow = "", panel = false, fieldReads = 0
        var actions: [String] = []
        var failureContext: String { "ax=retained" }
        private func delay(_ stage: String) { if slow == stage { time = 1 } }
        private func act(_ stage: String) { actions.append(stage); delay(stage) }
        func open() { act("open"); panel = true }
        func panelIsPresent() -> Bool {
            if !panel { delay("initial-panel-check") }
            return panel
        }
        func showLocationField() { act("show-location") }
        func locationFieldIsReady() -> Bool { fieldReads += 1; return true }
        func setLocation(_ path: String) { act("set-location") }
        func acceptLocation() { act("accept-location") }
        func locationFieldIsAbsent() -> Bool { true }
        func selectionIsReady() -> Bool { delay("selection-ready"); return true }
        func acceptSelection() { act("accept-selection"); panel = false }
        func selectedPath() -> String? { "/private/fixture.iso" }
    }
}
