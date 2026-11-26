import XCTest
@testable import BridgeVMProductE2E

final class T17ToggleChangeTests: XCTestCase {
    func testReceiptNamesTheSwitchAndValuesAfterOneUnconfirmedPress() {
        var presses = 0
        XCTAssertThrowsError(try T17ToggleChange.set(false, identifier: "bridgevm.runtime.clipboard",
                                                      value: { true }, press: { presses += 1; return true })) { error in
            XCTAssertEqual(error as? T17Blocker, T17Blocker(code: "ui-element-missing", detail:
                "toggle=bridgevm.runtime.clipboard requested=off before=on after=on: value did not change as requested"))
        }
        XCTAssertEqual(presses, 1)
    }
}
