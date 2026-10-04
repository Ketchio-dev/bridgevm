import XCTest
@testable import BridgeVMProductE2E

final class T17ChooserNativeReadClippingTests: XCTestCase {
    func testNestedClippingNeverChangesTheRecordedAXCode() {
        for length in 0...300 {
            var time = 0.0
            let budget = T17ChooserNativeBudget(deadline: 1, now: { time })
            XCTAssertThrowsError(try budget.read(.buttonLookup) {
                try budget.read(.snapshotAttempt) {
                    try budget.read(.relationship) {
                        time = 1
                        throw T17FileChooser.failure(String(repeating: "x", count: length) + "; ax_error=-25204")
                    }
                }
            }) { error in
                let detail = (error as? T17Blocker)?.detail ?? ""
                XCTAssertTrue(detail.contains("operation=selection-button-lookup"))
                XCTAssertTrue(detail.hasSuffix("; ax_error=-25204"))
                XCTAssertTrue(detail.count <= 400)
            }
        }
    }
}
