import Foundation
import XCTest
@testable import BridgeVMControl

/// Restart state belongs to one guest boot.
@MainActor
final class HvfRestartBootGenerationTests: XCTestCase {
    /// Windows can finish restarting while the "has not restarted" confirmation is open; confirming
    /// must not reset the new boot. The confirmation belongs to the boot it was asked for.
    func testWindowsRestartingWhileTheConfirmationIsOpenDoesNotResetTheNewBoot() throws {
        let fixture = try HvfRuntimeKeyboardDraftFixture()
        defer { fixture.remove() }
        try fixture.withSession { session in
            try fixture.negotiateOrderedInput(session)
            XCTAssertEqual(session.requestRestart(), .requested(.windowsAsked))
            XCTAssertEqual(session.requestRestart(), .needsResetConfirmation(.windowsHasNotRestarted))

            try fixture.appendLog(HvfGuestResetRecord.processRecreation + "\n")
            session.poll()
            guard case .refused = session.requestReset() else {
                return XCTFail("the confirmation was for the boot that just ended")
            }
            XCTAssertNil(try fixture.bytes(fixture.input), "the new boot is not reset")
            try fixture.appendLog("BVAGENT SERVICE start t=9\n")
            session.poll()
            XCTAssertEqual(session.requestRestart(), .requested(.windowsAsked), "Restart works for the new boot")
        }
    }

    /// A Reset confirmed after a reset record but before the next helper opens input.ctl is never read
    /// (the helper starts at the end), which would leave Restart and Reset doing nothing for the rest of
    /// the run. An unacknowledged Reset is dropped, so the user can press again.
    func testAResetNoHelperTookDoesNotBlockRestartForTheRestOfTheRun() throws {
        let fixture = try HvfRuntimeKeyboardDraftFixture()
        defer { fixture.remove() }
        try fixture.withSession { session in
            var now: TimeInterval = 100
            session.ownedStartNow = { now }
            session.beginOwnedInputBoot()
            session.connectionState = .booting
            XCTAssertEqual(session.requestRestart(), .needsResetConfirmation(.agentUnavailable))
            XCTAssertEqual(session.requestReset(), .requested(.reset))

            now += 10 // no `BVPOWER host reset`: no helper read the line
            session.poll()
            XCTAssertNil(session.restartRequest)
            XCTAssertEqual(session.requestRestart(), .needsResetConfirmation(.agentUnavailable),
                           "Restart is available again")
            XCTAssertEqual(session.requestReset(), .requested(.reset))
            XCTAssertEqual(try fixture.bytes(fixture.input), Data("RESET\nRESET\n".utf8))
        }
    }
}
