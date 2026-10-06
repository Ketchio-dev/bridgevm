import ApplicationServices
import XCTest
@testable import BridgeVMProductE2E

final class T17FileChooserSelectionAttemptTests: XCTestCase {
    func testProductionChooseUsesOneFreshGraphWhenEnabled() {
        let driver = T17SelectionAttemptDriver()
        driver.relationshipCost = 4.5
        do { try driver.choose() }
        catch { XCTFail("One enabled selection graph fits the shared deadline: \(error)") }
        XCTAssertEqual(driver.roots, 1)
        XCTAssertEqual(driver.relationshipReads, [0, 1, 2])
        XCTAssertEqual(driver.pressed, [2])
        XCTAssertEqual(driver.selectedReads, 1)
        XCTAssertEqual(driver.clock, 13.5)
    }

    func testDisabledPollReacquiresReplacementBeforePress() throws {
        let driver = T17SelectionAttemptDriver(); driver.enabled = false
        driver.afterPause = {
            driver.edges = [0: [3], 3: [4], 4: []]
            driver.roles[3] = "AXDialog"; driver.roles[4] = "AXButton"
            driver.identifiers[3] = "open-panel"; driver.identifiers[4] = "OKButton"
            driver.enabled = true
        }
        try driver.choose()
        XCTAssertEqual(driver.roots, 2)
        XCTAssertEqual(driver.relationshipReads, [0, 1, 2, 0, 3, 4])
        XCTAssertEqual(driver.enabledReads, [2, 4]); XCTAssertEqual(driver.pressed, [4])
    }

    func testAbsentPollReacquiresCurrentOwnerWithoutEnabledRead() throws {
        let driver = T17SelectionAttemptDriver(); driver.edges = [0: []]
        driver.afterPause = { driver.edges = [0: [1], 1: [2], 2: []] }
        try driver.choose()
        XCTAssertEqual(driver.roots, 2)
        XCTAssertEqual(driver.relationshipReads, [0, 0, 1, 2])
        XCTAssertEqual(driver.enabledReads, [2]); XCTAssertEqual(driver.pressed, [2])
    }

    func testLateLookupPreventsEnabledAndPressInCombinedStage() {
        let driver = T17SelectionAttemptDriver(); driver.relationshipCost = 7
        XCTAssertThrowsError(try driver.choose()) { error in
            let detail = (error as? T17Blocker)?.detail ?? ""
            XCTAssertTrue(detail.hasPrefix("stage=accept-selection;"))
            XCTAssertTrue(detail.contains("operation=ax-relationship; boundary=returned"))
            XCTAssertFalse(detail.contains("selection-ready"))
        }
        XCTAssertEqual(driver.roots, 1); XCTAssertTrue(driver.enabledReads.isEmpty)
        XCTAssertTrue(driver.pressed.isEmpty); XCTAssertEqual(driver.selectedReads, 0)
    }

    func testLateOrLateThrowingEnabledReadCannotPress() {
        for throwsRead in [false, true] {
            let driver = T17SelectionAttemptDriver()
            driver.enabledRead = { _ in
                driver.clock = 20
                if throwsRead { throw T17FileChooser.failure("file chooser AXAttributeNames read failed; ax_error=-25204") }
                return true
            }
            XCTAssertThrowsError(try driver.choose()) { error in
                XCTAssertTrue((error as? T17Blocker)?.detail.contains("operation=selection-enabled; boundary=returned") == true)
            }
            XCTAssertEqual(driver.roots, 1); XCTAssertTrue(driver.pressed.isEmpty)
        }
    }

    func testTransientEnabledFailureRetriesFreshGraphBeforeAnyPress() throws {
        let driver = T17SelectionAttemptDriver()
        driver.enabledRead = { _ in
            if driver.roots == 1 { throw T17FileChooser.failure("file chooser AXAttributeNames read failed; ax_error=-25204") }
            return true
        }
        try driver.choose()
        XCTAssertEqual(driver.roots, 2)
        XCTAssertEqual(driver.relationshipReads, [0, 1, 2, 0, 1, 2])
        XCTAssertEqual(driver.pressed, [2])
    }

    func testPressFailureCannotBecomeTransientReadRetry() {
        let driver = T17SelectionAttemptDriver()
        driver.pressError = T17FileChooser.failure("uncertain press read failed; ax_error=-25204")
        XCTAssertThrowsError(try driver.choose()) { error in
            let detail = (error as? T17Blocker)?.detail ?? ""
            XCTAssertTrue(detail.contains("selection_press_replay_refused=true"))
            XCTAssertTrue(detail.contains("ax_error=-25204"))
        }
        XCTAssertEqual(driver.roots, 1); XCTAssertEqual(driver.pressed, [2])
        XCTAssertEqual(driver.selectedReads, 0)
    }

    func testLateAdmittedPressRetainsActualResultWithoutReplay() {
        for result in [AXError.success, .cannotComplete] {
            let driver = T17SelectionAttemptDriver(); driver.pressResult = result; driver.pressReturnTime = 20
            XCTAssertThrowsError(try driver.choose()) { error in
                let detail = (error as? T17Blocker)?.detail ?? ""
                XCTAssertTrue(detail.contains("operation=selection-press; boundary=returned"))
                XCTAssertTrue(detail.contains("ax_error=\(result.rawValue)"))
            }
            XCTAssertEqual(driver.roots, 1); XCTAssertEqual(driver.pressed, [2])
            XCTAssertEqual(driver.selectedReads, 0)
        }
    }
}
