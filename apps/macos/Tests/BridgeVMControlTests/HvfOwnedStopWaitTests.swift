import Foundation
import XCTest
@testable import BridgeVMControl

/// The app's Stop asks Windows to close its apps and waits for it; only an explicit Force Stop forces.
@MainActor
final class HvfOwnedStopWaitTests: XCTestCase {
    private typealias Fixture = HvfOwnedRuntimeControllerTests.Fixture

    func testStopCommandSelection() {
        XCTAssertEqual(HvfGuestStopCommand.shutdown.line, "shutdown.exe /s /t 0", "Stop and quit: no /f")
        XCTAssertEqual(HvfGuestStopCommand.forceShutdown.line, "shutdown.exe /p /f", "Force Stop only")
    }

    func testAppStopWaitsPastTheGraceThenForceStopSendsTheForcedCommand() async throws {
        let f = try Fixture(); defer { f.clean() }
        f.controller.receive(.message(try f.event("ready", sequence: 1)))
        f.controller.guestServiceReady()
        let ticket = try appStop(f)
        try await waitForControl(f, "shutdown.exe /s /t 0\n")
        XCTAssertEqual(f.controller.guestStopWait, .windowsClosingApps)

        f.clock.value = 281; f.controller.tick()
        XCTAssertEqual(ticket.observation.phase, .guestGrace, "The grace ending must not kill Windows")
        XCTAssertEqual(f.controller.guestStopWait, .windowsStillRunning)
        f.clock.value = 400; f.controller.tick()
        XCTAssertNil(ticket.observation.failure, "Waiting for the user is not a missed deadline")
        let target = try XCTUnwrap(f.controller.forceStopTarget)

        XCTAssertEqual(f.controller.requestForceStop(target), .requested)
        try await waitForControl(f, "shutdown.exe /s /t 0\nshutdown.exe /p /f\n")
        XCTAssertEqual(ticket.observation.phase, .cancelling)
        XCTAssertEqual(ticket.observation.deadlineUptime, 442)
        XCTAssertNil(f.controller.forceStopTarget)
        XCTAssertNil(f.controller.guestStopWait)
        f.clock.value = 431; f.controller.tick() // Windows did not turn off: the runner is cancelled
        XCTAssertEqual(ticket.observation.phase, .cancelling)
        XCTAssertEqual(ticket.observation.deadlineUptime, 442, "Cancelling never renews the deadline")
        f.controller.receive(.message(try f.event("complete-not-admitted", sequence: 2, success: true)))
        try await f.exit(); f.controller.receive(.eof)
        XCTAssertTrue(ticket.observation.isComplete)
    }

    func testWindowsTurningOffAfterTheGraceCompletesTheHeldStop() async throws {
        let f = try Fixture(); defer { f.clean() }
        f.controller.receive(.message(try f.event("ready", sequence: 1)))
        let ticket = try appStop(f)
        XCTAssertEqual(f.controller.guestStopWait, .waitingForGuestAgent)
        f.clock.value = 300; f.controller.tick()
        f.controller.receive(.message(try f.event("complete-not-admitted", sequence: 2, success: true)))
        try await f.exit(); f.controller.receive(.eof)
        XCTAssertTrue(ticket.observation.isComplete)
        XCTAssertNil(ticket.observation.failure)
        XCTAssertFalse(f.controller.mayHaveOwnedWork)
    }

    func testForceStopWithoutTheAgentCancelsTheHeldStopAtOnce() throws {
        let f = try Fixture(); defer { f.clean() }
        f.controller.receive(.message(try f.event("ready", sequence: 1)))
        let ticket = try appStop(f)
        f.clock.value = 500; f.controller.tick()
        let target = try XCTUnwrap(f.controller.forceStopTarget)
        XCTAssertEqual(f.controller.requestForceStop(target), .requested)
        XCTAssertEqual(ticket.observation.phase, .cancelling)
        XCTAssertEqual(ticket.observation.deadlineUptime, 512)
        XCTAssertNil(ticket.observation.failure)
        XCTAssertTrue(try Data(contentsOf: URL(fileURLWithPath: f.base.config.ctlFilePath)).isEmpty)
    }

    func testTheSessionStopHoldsAndPublishesWhatWindowsIsDoing() throws {
        let f = try Fixture(); defer { f.clean() }
        let session = HvfEngineSession(config: f.base.config, repoRoot: f.base.root,
            processIsRunning: { _ in false }, vtpmKeyProvider: f.base.keys)
        session.process = f.process
        session.ownedProcessIdentity = f.controller.identity
        session.ownedController = f.controller
        let token = f.controller.identity.token
        f.controller.onChange = { [weak session] in session?.ownedRuntimeChanged(token: token) }
        f.controller.receive(.message(try f.event("ready", sequence: 1)))

        XCTAssertEqual(session.stopOwned(expectedToken: token), .requested(deadline: nil))
        XCTAssertEqual(session.stopWait, .waitingForGuestAgent)
        f.clock.value = 290; f.controller.tick()
        XCTAssertEqual(f.controller.operation?.observation.phase, .guestGrace)
        XCTAssertEqual(session.stopWait, .windowsStillRunning)
        XCTAssertNotNil(session.forceStopTarget)
        XCTAssertTrue(session.blocksAppTermination)
    }

    private func appStop(_ f: Fixture) throws -> HvfOwnedStopOperation {
        guard case let .accepted(ticket) = f.controller.requestStop(
            target: f.controller.identity, operationID: UUID(), afterGrace: .offerForceStop) else {
            throw CocoaError(.executableRuntimeMismatch)
        }
        return ticket
    }

    private func waitForControl(_ f: Fixture, _ expected: String) async throws {
        let path = f.base.config.ctlFilePath
        let deadline = ProcessInfo.processInfo.systemUptime + 2
        while (try? String(contentsOfFile: path, encoding: .utf8)) != expected,
              ProcessInfo.processInfo.systemUptime < deadline {
            try await Task.sleep(nanoseconds: 5_000_000)
        }
        XCTAssertEqual(try String(contentsOfFile: path, encoding: .utf8), expected)
        try await Task.sleep(nanoseconds: 20_000_000) // the delivery callback hops back to the main actor
    }
}
