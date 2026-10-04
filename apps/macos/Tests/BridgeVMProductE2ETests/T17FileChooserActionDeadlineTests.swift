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

    func testSlowSelectionLookupDoesNotConfirmSelection() {
        let driver = Driver(); driver.slow = "selection-lookup"
        fails(driver, at: "accept-selection", before: false)
        XCTAssertFalse(driver.actions.contains("accept-selection"))
    }

    private func fails(_ driver: Driver, at stage: String, before: Bool = true) {
        XCTAssertThrowsError(try T17FileChooser.choose(path: "/private/fixture.iso", timeout: 1,
            driver: driver, now: { driver.time }, pause: { driver.time += 0.1 })) { error in
                let blocker = error as? T17Blocker
                XCTAssertEqual(blocker?.code, "input-selection-failed")
                XCTAssertTrue(blocker?.detail.hasPrefix("stage=\(stage); timed out \(before ? "before chooser stage" : "waiting")") == true)
                XCTAssertTrue(blocker?.detail.contains("; ax=retained") == true)
            }
    }

    private typealias Driver = T17ChooserActionDeadlineDriver
}
