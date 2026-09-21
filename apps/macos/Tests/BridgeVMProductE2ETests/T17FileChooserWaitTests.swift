import ApplicationServices
import XCTest
@testable import BridgeVMProductE2E

final class T17FileChooserWaitTests: XCTestCase {
    private func failure(_ status: AXError) -> T17Blocker {
        T17FileChooser.failure(
            "file chooser AXIdentifier read failed; ax_error=\(status.rawValue)")
    }

    func testTransientReadCanRecoverWithinTheUnchangedDeadline() throws {
        var clock: TimeInterval = 0
        var attempts = 0
        try T17FileChooserWait.until(stage: "chooser dismissal", deadline: 1,
            failureContext: { "; ax=complete" }, now: { clock },
            pause: { clock += 0.1 }) {
                attempts += 1
                if attempts < 3 { throw self.failure(.failure) }
                return true
            }
        XCTAssertEqual(attempts, 3)
        XCTAssertEqual(clock, 0.2)
    }

    func testExhaustionRetainsLastTransientAndCompleteContext() {
        var clock: TimeInterval = 0
        XCTAssertThrowsError(try T17FileChooserWait.until(
            stage: "chooser dismissal", deadline: 0.2,
            failureContext: { "; ax=complete" }, now: { clock },
            pause: { clock += 0.1 }, ready: { throw self.failure(.failure) })) { error in
                let detail = (error as? T17Blocker)?.detail ?? ""
                XCTAssertTrue(detail.contains("last_transient=file chooser AXIdentifier read failed; ax_error=-25200"))
                XCTAssertTrue(detail.hasSuffix("; ax=complete"))
            }
    }

    func testNonTransientFailureStillFailsImmediately() {
        var pauses = 0
        let observed = failure(.illegalArgument)
        XCTAssertThrowsError(try T17FileChooserWait.until(stage: "chooser dismissal", deadline: 1,
            failureContext: { "; unused" }, now: { 0 }, pause: { pauses += 1 },
            ready: { throw observed })) { error in
                XCTAssertEqual((error as? T17Blocker)?.detail, observed.detail)
            }
        XCTAssertEqual(pauses, 0)
    }
}
