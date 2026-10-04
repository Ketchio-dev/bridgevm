import ApplicationServices
import XCTest
@testable import BridgeVMProductE2E

final class T17ChooserSelectionActionTests: XCTestCase {
    func testLateButtonLookupCannotReadEnabledOrPress() {
        var time = 0.0, enabledReads = 0, presses = 0
        let budget = T17ChooserNativeBudget(deadline: 1, now: { time })
        XCTAssertThrowsError(try T17ChooserSelectionAction.perform(budget: budget,
            lookup: { time = 1; return 7 }, enabled: { _ in enabledReads += 1; return true },
            press: { _ in presses += 1 })) { error in
                XCTAssertTrue((error as? T17Blocker)?.detail.contains("operation=selection-button-lookup") == true)
            }
        XCTAssertEqual(enabledReads, 0); XCTAssertEqual(presses, 0)
    }
    func testLateEnabledReadCannotPress() {
        var time = 0.0, presses = 0
        let budget = T17ChooserNativeBudget(deadline: 1, now: { time })
        XCTAssertThrowsError(try T17ChooserSelectionAction.perform(budget: budget,
            lookup: { 7 }, enabled: { _ in time = 1; return true }, press: { _ in presses += 1 })) { error in
                XCTAssertTrue((error as? T17Blocker)?.detail.contains("operation=selection-enabled") == true)
            }
        XCTAssertEqual(presses, 0)
    }
    func testExactButtonIsPressedOnceBeforeDeadline() throws {
        let budget = T17ChooserNativeBudget(deadline: 1, now: { 0.5 })
        var buttons: [Int] = []
        try T17ChooserSelectionAction.perform(budget: budget, lookup: { 7 }, enabled: { _ in true },
            press: { buttons.append($0) })
        XCTAssertEqual(buttons, [7])
    }
    func testMissingOrDisabledButtonReturnsNotReadyWithoutPress() throws {
        for button in [nil, 7] as [Int?] {
            XCTAssertFalse(try T17ChooserSelectionAction.perform(
                budget: T17ChooserNativeBudget(deadline: 1, now: { 0 }), lookup: { button },
                enabled: { _ in false }, press: { _ in XCTFail("disabled selection pressed") }))
        }
    }
    func testLateAdmittedPressRetainsRawResultAndDoesNotReplay() {
        for result in [AXError.success, .cannotComplete] {
            var time = 0.0, presses = 0
            let budget = T17ChooserNativeBudget(deadline: 1, now: { time })
            XCTAssertThrowsError(try T17ChooserSelectionAction.perform(budget: budget,
                lookup: { 7 }, enabled: { _ in true }, press: { _ in
                    _ = try budget.input(.selectionPress) { presses += 1; time = 1.5; return result }
                })) { error in
                    let detail = (error as? T17Blocker)?.detail ?? ""
                    XCTAssertTrue(detail.contains("operation=selection-press; boundary=returned"))
                    XCTAssertTrue(detail.contains("ax_error=\(result.rawValue)"))
                }
            XCTAssertEqual(presses, 1)
        }
    }
}
