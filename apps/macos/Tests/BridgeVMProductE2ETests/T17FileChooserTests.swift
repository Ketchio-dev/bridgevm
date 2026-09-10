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

    private final class Driver: T17FileChooserDriving {
        var actions: [String] = []
        var panel = false, fieldReady = true, locationCloses = true, openEnabled = true
        var panelCloses = true, inventoryFails = false, pathRejected = false
        var panelDelay = 0
        var path: String?
        var returnedPath: String?
        func open() { actions.append("open"); panel = true }
        func panelIsPresent() throws -> Bool {
            if inventoryFails { throw T17FileChooser.failure("unreadable inventory") }
            if panel && panelDelay > 0 { panelDelay -= 1; return false }
            return panel
        }
        func showLocationField() { actions.append("show-location") }
        func locationFieldIsReady() -> Bool { fieldReady }
        func setLocation(_ value: String) throws {
            actions.append("set-location")
            if pathRejected { throw T17FileChooser.failure("rejected path") }
            path = value
        }
        func acceptLocation() { actions.append("accept-location") }
        func locationFieldIsAbsent() -> Bool { locationCloses }
        func selectionIsReady() -> Bool { openEnabled }
        func acceptSelection() { actions.append("accept-selection"); if panelCloses { panel = false } }
        func selectedPath() -> String? { returnedPath ?? path }
    }
}
