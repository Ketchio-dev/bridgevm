import ApplicationServices
import XCTest
@testable import BridgeVMProductE2E

final class T17ChooserOpenActionDeadlineTests: XCTestCase {
    func testEnabledReadFinishingAtOrAfterDeadlineDoesNotPress() {
        for elapsed in [1.0, 2.0] {
            var clock = 0.0, presses = 0
            XCTAssertThrowsError(try T17ChooserOpenAction.perform(identifier: "chooser", timeout: 1,
                enabled: { clock = elapsed; return true },
                press: { presses += 1; return .success }, now: { clock }, pause: {})) { error in
                    XCTAssertEqual((error as? T17Blocker)?.detail,
                        "chooser control readiness timed out: chooser")
                }
            XCTAssertEqual(presses, 0)
        }
    }

    func testEnabledReadFinishingBeforeDeadlinePressesOnce() throws {
        var clock = 0.0, presses = 0
        XCTAssertEqual(try T17ChooserOpenAction.perform(identifier: "chooser", timeout: 1,
            enabled: { clock = 0.9; return true },
            press: { presses += 1; return .success }, now: { clock }, pause: {}), .success)
        XCTAssertEqual(presses, 1)
    }
}
