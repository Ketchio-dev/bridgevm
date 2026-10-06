import XCTest
@testable import BridgeVMProductE2E

private final class OwnedReadError: Error {}
final class T17ChooserNativeReadFailureTests: XCTestCase {
    func testLateThrowingEnabledReadKeepsOperationAndKnownErrorWithoutPress() {
        var time = 0.0, presses = 0
        let original = T17FileChooser.failure("file chooser AXAttributeNames read failed; ax_error=-25204")
        XCTAssertThrowsError(try T17ChooserSelectionAction.perform(
            budget: T17ChooserNativeBudget(deadline: 1, now: { time }), lookup: { 7 },
            enabled: { _ in time = 1; throw original }, press: { _ in presses += 1 })) { error in
                let detail = (error as? T17Blocker)?.detail ?? ""
                XCTAssertTrue(detail.contains("operation=selection-enabled; boundary=returned"))
                XCTAssertTrue(detail.contains(original.detail))
            }
        XCTAssertEqual(presses, 0)
    }
    func testLateThrowingLookupCannotReadEnabledOrPress() {
        var time = 0.0, enabled = 0, presses = 0
        XCTAssertThrowsError(try T17ChooserSelectionAction.perform(
            budget: T17ChooserNativeBudget(deadline: 1, now: { time }), lookup: { () throws -> Int? in
                time = 1; throw T17FileChooser.failure("AX lookup read failed; ax_error=-25204")
            }, enabled: { _ in enabled += 1; return true }, press: { _ in presses += 1 })) { error in
                XCTAssertTrue((error as? T17Blocker)?.detail.contains("operation=selection-button-lookup") == true)
            }
        XCTAssertEqual(enabled, 0); XCTAssertEqual(presses, 0)
    }
    func testUnderBudgetErrorObjectIdentityIsUnchanged() {
        let original = OwnedReadError()
        XCTAssertThrowsError(try T17ChooserNativeBudget(deadline: 1, now: { 0.5 }).read(.enabled) {
            throw original
        }) { error in XCTAssertTrue((error as? OwnedReadError) === original) }
    }
    func testUnderBudgetKnownBlockerCodeAndTextAreUnchanged() {
        let original = T17FileChooser.failure("file chooser AXAttributeNames read failed; ax_error=-25204")
        XCTAssertThrowsError(try T17ChooserNativeBudget(deadline: 1, now: { 0.5 }).read(.enabled) {
            throw original
        }) { error in
            XCTAssertEqual((error as? T17Blocker)?.code, original.code)
            XCTAssertEqual((error as? T17Blocker)?.detail, original.detail)
        }
    }
    func testLateThrowingRelationshipCannotPauseOrRetrySnapshot() {
        var time = 0.0, roots = 0, pauses = 0
        let budget = T17ChooserNativeBudget(deadline: 1, now: { time })
        XCTAssertThrowsError(try budget.snapshot(root: { roots += 1; return 7 }, nodes: { _ -> [Int] in
            try budget.read(.relationship) {
                time = 1; throw T17FileChooser.failure("AXChildren read failed; ax_error=-25204")
            }
        }, project: { $0.count }, pause: { pauses += 1 })) { error in
            let detail = (error as? T17Blocker)?.detail ?? ""
            XCTAssertTrue(detail.contains("operation=snapshot-attempt"))
            XCTAssertTrue(detail.contains("operation=ax-relationship"))
            XCTAssertTrue(detail.contains("ax_error=-25204"))
        }
        XCTAssertEqual(roots, 1); XCTAssertEqual(pauses, 0)
    }
    func testNestedLongKnownFailureKeepsCodeWithBoundedOriginalContext() {
        var time = 0.0
        let budget = T17ChooserNativeBudget(deadline: 1, now: { time })
        XCTAssertThrowsError(try budget.read(.buttonLookup) {
            try budget.read(.snapshotAttempt) {
                try budget.read(.relationship) {
                    time = 1; throw T17FileChooser.failure(String(repeating: "x", count: 1_200) + "; ax_error=-25204")
                }
            }
        }) { error in
            let detail = (error as? T17Blocker)?.detail ?? ""
            XCTAssertTrue(detail.contains("operation=selection-button-lookup"))
            XCTAssertTrue(detail.contains("ax_error=-25204"))
            XCTAssertTrue(detail.count <= 400)
        }
    }
}
