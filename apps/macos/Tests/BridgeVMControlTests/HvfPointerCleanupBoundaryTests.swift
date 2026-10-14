import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfPointerCleanupBoundaryTests: XCTestCase {
    func testChangedBindingOrLifecycleCancelsCleanupBeforeAnyWrite() {
        for change in ["binding", "service", "BVAGENT READY host=new", "BVAGENT re-READY host=new",
                       "BVAGENT SERVICE start t=2", "PSCI SYSTEM_RESET: reboot 1/8"] {
            var fixture = HvfPointerFailureFixture()
            fixture.failAfterConfirmedPress()
            let before = fixture.commands
            let update = fixture.poll(lines: change.hasPrefix("BV") || change.hasPrefix("PSCI") ? [change] : [],
                ready: change != "service", binding: change == "binding" ? ["other"] : nil)
            if change == "binding" { XCTAssertNil(update) }
            else { XCTAssertEqual(update, .cancelled(.sessionChanged, discarded: 0), change) }
            XCTAssertEqual(fixture.router.count, 0, change)
            XCTAssertEqual(fixture.commands, before, change)
            fixture.assertFailedClosed()
        }
    }

    func testRestartInCleanupReceiptBatchCancelsEvenAnEarlierValidReceipt() {
        var fixture = HvfPointerFailureFixture()
        fixture.failAfterConfirmedPress()
        _ = fixture.poll()
        let cleanup = fixture.commands.last!
        _ = fixture.poll(lines: fixture.receipt(cleanup) + ["BVAGENT re-READY host=new"])
        XCTAssertEqual(fixture.router.failure, .agentRestarted)
        XCTAssertEqual(fixture.router.count, 0)
        XCTAssertEqual(fixture.commands.count, 3)
        fixture.assertFailedClosed()
    }

    func testGuestResetRearmsOnlyNewOwnedGenerationWithoutOldCleanup() {
        var fixture = HvfPointerFailureFixture()
        fixture.failAfterConfirmedPress()
        let before = fixture.commands
        _ = fixture.router.pollBatch(binding: fixture.binding, serviceReady: false,
            lines: ["PSCI SYSTEM_RESET: reboot 1/8"], now: fixture.now) { _ in
                XCTFail("Old cleanup crossed the reset"); return true
            }
        XCTAssertFalse(fixture.router.failed)
        XCTAssertEqual(fixture.router.count, 0)
        XCTAssertEqual(fixture.router.state, .disconnected)
        XCTAssertNil(fixture.poll(ready: false))
        XCTAssertEqual(fixture.commands, before)
        _ = fixture.poll()
        XCTAssertTrue(fixture.commands.last?.hasPrefix("INPUTCAPS ") == true)
    }
}
