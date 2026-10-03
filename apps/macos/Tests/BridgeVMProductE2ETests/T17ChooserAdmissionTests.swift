import ApplicationServices
import XCTest
@testable import BridgeVMProductE2E

final class T17ChooserAdmissionTests: XCTestCase {
    func testLookupExhaustingTheBudgetNeverBuildsOrOpensTheDriver() {
        for end in [20.0, 21.0] {
            var clock = 0.0, drivers = 0
            XCTAssertThrowsError(try T17ChooserAdmission.choose(path: "/fixture/share", timeout: 20,
                now: { clock }, pause: {}, lookup: { allowance in
                    XCTAssertEqual(allowance, 20); clock = end; return 1
                }, driver: { _, _ in drivers += 1; return Driver() })) { error in
                    XCTAssertTrue((error as? T17Blocker)?.detail.hasPrefix("stage=chooser-admission;") == true)
                }
            XCTAssertEqual(drivers, 0)
        }
    }

    func testLookupAndInteractionUseTheSameAbsoluteDeadline() throws {
        let driver = Driver(); driver.clock = 100
        try T17ChooserAdmission.choose(path: "/fixture/share", timeout: 20,
            now: { driver.clock }, pause: {}, lookup: { allowance in
                XCTAssertEqual(allowance, 20); driver.clock = 119; return 7
            }, driver: { target, deadline in
                XCTAssertEqual(target, 7); XCTAssertEqual(deadline, 120)
                driver.openDeadline = deadline; return driver
            })
        XCTAssertEqual(driver.actions, ["open", "location", "write", "accept-location", "accept-selection"])
        XCTAssertEqual(driver.selectedReads, 1)
    }

    func testSlowEnabledReadDoesNotPressTheControl() {
        let driver = Driver(); driver.enabledDelay = 20
        fails(driver, at: "open-control")
        XCTAssertEqual(driver.presses, 0)
        XCTAssertEqual(driver.panelReads, 1)
    }

    func testSlowPressReturnsARefusalBeforePanelReadiness() {
        let driver = Driver(); driver.pressDelay = 20
        fails(driver, at: "open-control")
        XCTAssertEqual(driver.presses, 1)
        XCTAssertEqual(driver.panelReads, 1)
        XCTAssertFalse(driver.actions.contains("location"))
    }

    func testAcceptSelectionOvershootNeverStartsConfirmation() {
        let driver = Driver(); driver.acceptDelay = 20
        fails(driver, at: "selection-confirmation")
        XCTAssertTrue(driver.actions.contains("accept-selection"))
        XCTAssertEqual(driver.panelReads, 2)
        XCTAssertEqual(driver.selectedReads, 0)
    }

    func testProvisionalCannotCompleteStillNeedsExactSelection() throws {
        let driver = Driver(); driver.openResult = .cannotComplete
        try choose(driver)
        XCTAssertEqual(driver.selectedReads, 1)
        XCTAssertEqual(driver.presses, 1)
    }

    func testConsumedPreludeLeavesOnlyItsRemainingOpenAllowance() {
        let driver = Driver(); driver.enabledDelay = 1
        XCTAssertThrowsError(try T17ChooserAdmission.choose(path: "/fixture/share", timeout: 20,
            now: { driver.clock }, pause: {}, lookup: { _ in driver.clock = 19; return 1 },
            driver: { _, deadline in driver.openDeadline = deadline; return driver }))
        XCTAssertEqual(driver.presses, 0)
    }

    func testInvalidArgumentsDoNotLookupOrOpenAnything() {
        for (path, timeout) in [("relative", 20.0), ("/nul\0", 20), ("/fixture", 0), ("/fixture", .infinity)] {
            XCTAssertThrowsError(try T17ChooserAdmission.choose(path: path, timeout: timeout,
                now: { 0 }, pause: {}, lookup: { _ in XCTFail(); return 1 },
                driver: { _, _ in XCTFail(); return Driver() }))
        }
    }

    private func choose(_ driver: Driver) throws {
        try T17ChooserAdmission.choose(path: "/fixture/share", timeout: 20,
            now: { driver.clock }, pause: { driver.clock += 0.1 }, lookup: { _ in 1 },
            driver: { _, deadline in driver.openDeadline = deadline; return driver })
    }

    private func fails(_ driver: Driver, at stage: String) {
        XCTAssertThrowsError(try choose(driver)) { error in
            XCTAssertEqual((error as? T17Blocker)?.code, "input-selection-failed")
            XCTAssertTrue((error as? T17Blocker)?.detail.hasPrefix("stage=\(stage);") == true)
        }
    }

    private final class Driver: T17FileChooserDriving {
        var clock = 0.0, openDeadline = 0.0, enabledDelay = 0.0, pressDelay = 0.0, acceptDelay = 0.0
        var panel = false, presses = 0, panelReads = 0, selectedReads = 0
        var openResult = AXError.success
        var actions: [String] = []
        func open() throws {
            actions.append("open")
            _ = try T17ChooserOpenAction.perform(identifier: "fixture", deadline: openDeadline,
                enabled: { self.clock += self.enabledDelay; return true },
                press: { self.presses += 1; self.clock += self.pressDelay; self.panel = true; return self.openResult },
                now: { self.clock }, pause: {})
        }
        func panelIsPresent() -> Bool { panelReads += 1; return panel }
        func showLocationField() { actions.append("location") }
        func locationFieldIsReady() -> Bool { true }
        func setLocation(_ path: String) { actions.append("write") }
        func acceptLocation() { actions.append("accept-location") }
        func locationFieldIsAbsent() -> Bool { true }
        func selectionIsReady() -> Bool { true }
        func acceptSelection() { actions.append("accept-selection"); clock += acceptDelay; panel = false }
        func selectedPath() -> String? { selectedReads += 1; return "/fixture/share" }
    }
}
