import XCTest
@testable import BridgeVMProductE2E

final class T17FileChooserDeadlineTests: XCTestCase {
    func testLatePanelReadinessDoesNotSendShortcut() {
        let driver = Driver(); driver.panelReadTime = 1
        XCTAssertThrowsError(try choose(driver))
        XCTAssertFalse(driver.shortcutSent)
    }

    func testLateExactSelectionIsNotSuccess() {
        for time in [1.0, 2.0] {
            let driver = Driver(); driver.selectionReadTime = time
            XCTAssertThrowsError(try choose(driver))
            XCTAssertTrue(driver.selectionAccepted)
        }
    }

    func testExactSelectionBeforeDeadlineSucceeds() throws {
        let driver = Driver(); driver.selectionReadTime = 0.9
        try choose(driver)
        XCTAssertTrue(driver.selectionAccepted)
    }

    private func choose(_ driver: Driver) throws {
        try T17FileChooser.choose(path: "/private/fixture.iso", timeout: 1, driver: driver,
                                  now: { driver.time }, pause: { driver.time += 0.1 })
    }

    private final class Driver: T17FileChooserDriving {
        var time = 0.0, panelReadTime = 0.0, selectionReadTime = 0.0
        var panel = false, shortcutSent = false, selectionAccepted = false
        func open() { panel = true }
        func panelIsPresent() -> Bool {
            if panel { time = panelReadTime }
            return panel
        }
        func showLocationField() { shortcutSent = true }
        func locationFieldIsReady() -> Bool { true }
        func setLocation(_ path: String) {}
        func acceptLocation() {}
        func locationFieldIsAbsent() -> Bool { true }
        func selectionIsReady() -> Bool { true }
        func acceptSelection() { selectionAccepted = true; panel = false }
        func selectedPath() -> String? {
            time = selectionReadTime
            return "/private/fixture.iso"
        }
    }
}
