import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfGuestRestartTests: XCTestCase {
    /// With the guest agent, Restart asks Windows once and never resets the VM.
    func testRestartWithTheAgentAsksWindowsAndWritesNoReset() throws {
        let fixture = try HvfRuntimeKeyboardDraftFixture()
        defer { fixture.remove() }
        try fixture.withSession { session in
            try fixture.negotiateOrderedInput(session)
            let negotiated = try fixture.commands()

            XCTAssertEqual(session.requestRestart(), .requested(.windowsAsked))
            XCTAssertEqual(try fixture.commands(), negotiated + ["shutdown.exe /r /t 0"])
            XCTAssertEqual(session.restartRequest, .windowsAsked)

            // Windows has not restarted yet (an app may hold it): a second press offers Reset.
            XCTAssertEqual(session.requestRestart(), .needsResetConfirmation(.windowsHasNotRestarted))
            XCTAssertEqual(session.requestWindowsRestart(), .requested(.windowsAsked), "asking again")
            XCTAssertEqual(try fixture.commands(), negotiated + ["shutdown.exe /r /t 0", "shutdown.exe /r /t 0"])
            XCTAssertNil(try fixture.bytes(fixture.input), "no RESET")

            // The guest's reset record is the restart.
            try fixture.appendLog(HvfGuestResetRecord.processRecreation + "\n")
            session.poll()
            XCTAssertNil(session.restartRequest)
        }
    }

    /// At the login screen or during boot there is no agent: Restart resets only after the user confirms.
    func testRestartWithoutTheAgentResetsOnlyAfterConfirmation() throws {
        let fixture = try HvfRuntimeKeyboardDraftFixture()
        defer { fixture.remove() }
        try fixture.withSession { session in
            session.beginOwnedInputBoot()
            session.connectionState = .booting

            XCTAssertEqual(session.requestRestart(), .needsResetConfirmation(.agentUnavailable))
            XCTAssertNil(try fixture.bytes(fixture.input), "nothing before the user confirms")

            XCTAssertEqual(session.requestReset(), .requested(.reset))
            XCTAssertEqual(session.requestReset(), .requested(.reset))
            XCTAssertEqual(session.requestRestart(), .requested(.reset))
            XCTAssertEqual(try fixture.bytes(fixture.input), Data("RESET\n".utf8),
                           "one line: a second one could reset the next boot too")
            XCTAssertEqual(try fixture.commands(), [])
        }
    }

    func testAStoppedVMIsNotRestarted() throws {
        let fixture = try HvfRuntimeKeyboardDraftFixture()
        defer { fixture.remove() }
        try fixture.withSession { session in
            session.beginOwnedInputBoot()
            guard case .refused = session.requestRestart() else { return XCTFail("restart of a stopped VM") }
            guard case .refused = session.requestReset() else { return XCTFail("reset of a stopped VM") }
            session.connectionState = .stopping
            guard case .refused = session.requestReset() else { return XCTFail("reset during a Stop") }
            XCTAssertNil(try fixture.bytes(fixture.input))
            XCTAssertEqual(try fixture.commands(), [])
        }
    }
}
