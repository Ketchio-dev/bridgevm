import XCTest
@testable import BridgeVMProductE2E

final class T17FileChooserTests: XCTestCase {
    private let path = "/private/test media/installer.iso"

    private func run(_ driver: Driver, path: String? = nil) throws {
        var clock: TimeInterval = 0
        try T17FileChooser.choose(path: path ?? self.path, timeout: 1, driver: driver,
                                  now: { clock }, pause: { clock += 0.1 })
    }

    func testDelayedPanelMustReachEveryStageAndExactPath() throws {
        let driver = Driver()
        driver.panelDelay = 3
        try run(driver)
        XCTAssertEqual(driver.actions, ["open", "show-location", "set-location", "accept-location", "accept-selection"])
        XCTAssertEqual(driver.path, path)
    }

    func testExistingPanelRefusesToOpenAnother() {
        let driver = Driver(); driver.panel = true
        XCTAssertThrowsError(try run(driver))
        XCTAssertTrue(driver.actions.isEmpty)
    }

    func testMissingLocationFieldDoesNotTypeOrContinue() {
        let driver = Driver(); driver.fieldReady = false
        XCTAssertThrowsError(try run(driver))
        XCTAssertEqual(driver.actions, ["open", "show-location"])
    }

    func testGoToSheetMustCloseBeforeOpenIsPressed() {
        let driver = Driver(); driver.locationCloses = false
        XCTAssertThrowsError(try run(driver))
        XCTAssertFalse(driver.actions.contains("accept-selection"))
    }

    func testDisabledOpenIsNeverPressed() {
        let driver = Driver(); driver.openEnabled = false
        XCTAssertThrowsError(try run(driver))
        XCTAssertFalse(driver.actions.contains("accept-selection"))
    }

    func testDismissalWithoutExactSelectionIsNotSuccess() {
        for selected in ["", "installer.iso", "/different/installer.iso"] {
            let driver = Driver(); driver.returnedPath = selected
            XCTAssertThrowsError(try run(driver))
        }
    }

    func testExactSelectionWithOpenPanelIsNotSuccess() {
        let driver = Driver(); driver.panelCloses = false
        XCTAssertThrowsError(try run(driver))
    }

    func testUnreadableWindowInventoryIsNotAbsence() {
        let driver = Driver(); driver.inventoryFails = true
        XCTAssertThrowsError(try run(driver))
        XCTAssertTrue(driver.actions.isEmpty)
    }

    func testRejectedPathStopsBeforeConfirmation() {
        let driver = Driver(); driver.pathRejected = true
        XCTAssertThrowsError(try run(driver))
        XCTAssertFalse(driver.actions.contains("accept-location"))
        XCTAssertFalse(driver.actions.contains("accept-selection"))
    }

    func testRelativePathIsRejectedBeforeAnyAction() {
        let driver = Driver()
        XCTAssertThrowsError(try run(driver, path: "installer.iso"))
        XCTAssertTrue(driver.actions.isEmpty)
    }

    private typealias Driver = T17FileChooserDriver
}
