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

    func testALateForceStopStillReachesTheCLIAsAValidObservation() async throws {
        let f = try Fixture(); defer { f.clean() }
        f.controller.receive(.message(try f.event("ready", sequence: 1)))
        f.controller.guestServiceReady()
        let ticket = try appStop(f)
        try await waitForControl(f, "shutdown.exe /s /t 0\n")
        f.clock.value = 600; f.controller.tick() // a CLI stop joins this ticket (NativeRuntimeStopLedger)
        XCTAssertEqual(f.controller.requestForceStop(try XCTUnwrap(f.controller.forceStopTarget)), .requested)
        XCTAssertEqual(ticket.observation.deadlineUptime, 642)
        let target = NativeRuntimeStopTarget(appInstanceID: UUID().uuidString,
            runToken: f.controller.identity.token.uuidString, processID: f.controller.identity.processID,
            acceptedConfigurationDigest: String(repeating: "a", count: 64))
        XCTAssertNoThrow(try NativeRuntimeControlCodec.validate(NativeRuntimeStopObservation(ticket.observation), target: target),
                         "The escalated deadline is 542 s after admission")
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
        let session = bind(f)
        let token = f.controller.identity.token
        f.controller.receive(.message(try f.event("ready", sequence: 1)))

        XCTAssertEqual(session.stopOwned(expectedToken: token), .requested(deadline: nil))
        XCTAssertEqual(session.stopWait, .waitingForGuestAgent)
        f.clock.value = 290; f.controller.tick()
        XCTAssertEqual(f.controller.operation?.observation.phase, .guestGrace)
        XCTAssertEqual(session.stopWait, .windowsStillRunning)
        XCTAssertNotNil(session.forceStopTarget)
        XCTAssertTrue(session.blocksAppTermination)
    }

    func testGuestInputKeepsWorkingWhileTheStopWaitsForWindows() throws {
        let f = try Fixture(); defer { f.clean() }
        let evidence = URL(fileURLWithPath: f.base.config.evidenceDir)
        try FileManager.default.createDirectory(at: evidence, withIntermediateDirectories: true)
        let log = evidence.appendingPathComponent("run.log")
        try "BVAGENT READY host=test t=1\nBVAGENT SERVICE start t=2\n".write(to: log, atomically: true, encoding: .utf8)
        let session = bind(f)
        // Synthetic owned-boot boundary and capability reply, not a real Windows receipt.
        session.beginOwnedInputBoot()
        session.poll(); session.poll() // the READY batch resets the stream; the next poll asks for capabilities
        let request = try XCTUnwrap(try commands(f).last)
        let id = request.split(separator: " ").last ?? "missing"
        try append("BVAGENT CMD \(request) exit=0\nBVINPUT_CAPS \(id) 3 TEXTINPUT KEYINPUT POINTERINPUT 65536\n"
            + "BVAGENT END \(request)\n", to: log)
        session.poll(); session.poll()
        f.controller.receive(.message(try f.event("ready", sequence: 1)))

        XCTAssertEqual(session.stopOwned(expectedToken: f.controller.identity.token), .requested(deadline: nil))
        session.poll()
        XCTAssertTrue(session.sendKey("enter"), "Windows still runs: the user may have to save or answer it")
        XCTAssertEqual(try commands(f).filter { $0.hasPrefix("KEYINPUT ") }.count, 1)
        f.clock.value = 290; f.controller.tick(); session.poll()
        XCTAssertEqual(f.controller.guestStopWait, .windowsStillRunning)
        XCTAssertTrue(session.sendKey("enter"), "The held Stop keeps input after its grace")

        let target = try XCTUnwrap(session.forceStopTarget)
        XCTAssertEqual(session.requestForceStop(target), .requested)
        session.poll()
        XCTAssertFalse(session.sendKey("enter"), "Force Stop closes Windows' apps without asking")
    }

    func testAStopNeverOpensANewInputNegotiation() throws {
        let f = try Fixture(); defer { f.clean() }
        let evidence = URL(fileURLWithPath: f.base.config.evidenceDir)
        try FileManager.default.createDirectory(at: evidence, withIntermediateDirectories: true)
        try "BVAGENT READY host=test t=1\nBVAGENT SERVICE start t=2\n".write(
            to: evidence.appendingPathComponent("run.log"), atomically: true, encoding: .utf8)
        let session = bind(f)
        session.beginOwnedInputBoot()
        session.poll() // the READY batch: the stream has not asked for capabilities yet
        f.controller.receive(.message(try f.event("ready", sequence: 1)))

        XCTAssertEqual(session.stopOwned(expectedToken: f.controller.identity.token), .requested(deadline: nil))
        session.poll(); session.poll()
        XCTAssertEqual(try commands(f).filter { $0.hasPrefix("INPUTCAPS") }, [],
                       "The Stop may have found the control file replaced: it writes nothing new there")
        XCTAssertTrue(session.sendKey("enter"), "Input goes through the legacy device path")
    }

    private func bind(_ f: Fixture) -> HvfEngineSession {
        let session = HvfEngineSession(config: f.base.config, repoRoot: f.base.root,
            processIsRunning: { _ in false }, vtpmKeyProvider: f.base.keys)
        session.process = f.process
        session.ownedProcessIdentity = f.controller.identity
        session.ownedController = f.controller
        let token = f.controller.identity.token
        f.controller.onChange = { [weak session] in session?.ownedRuntimeChanged(token: token) }
        return session
    }

    private func commands(_ f: Fixture) throws -> [String] {
        try String(contentsOfFile: f.base.config.ctlFilePath, encoding: .utf8).split(separator: "\n").map(String.init)
    }

    private func append(_ value: String, to log: URL) throws {
        let handle = try FileHandle(forWritingTo: log)
        defer { try? handle.close() }
        try handle.seekToEnd()
        try handle.write(contentsOf: Data(value.utf8))
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
