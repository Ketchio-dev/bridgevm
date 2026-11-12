import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class SupervisionGracefulStopRegressionTests: XCTestCase {
    private typealias Fixture = HvfOwnedRuntimeControllerTests.Fixture

    private func stopAfterLoss(_ f: Fixture) async throws -> HvfOwnedStopOperation {
        f.controller.receive(.message(try f.event("ready", sequence: 1)))
        f.controller.receive(.failed(.channelClosed))
        let ticket = try f.stop()
        try await Task.sleep(nanoseconds: 50_000_000)
        return ticket
    }

    func testOrdinaryStopClosesOwnerChannelWithoutSignallingRunner() async throws {
        let f = try Fixture(); defer { f.clean() }
        _ = try await stopAfterLoss(f)
        XCTAssertTrue(f.process.isRunning, "Ordinary Stop must allow the runner's owner-EOF shutdown grace")
        XCTAssertThrowsError(try f.channel.sendStop(.init(schemaVersion: 1, kind: "stop",
            runToken: f.controller.identity.token.uuidString.lowercased(), sequence: 1,
            operationID: UUID().uuidString.lowercased()))) { error in
            XCTAssertEqual(error as? HvfOwnedRuntimeProtocolError, .channelClosed)
        }
        f.clock.value += 300; f.controller.tick()
        XCTAssertTrue(f.process.isRunning, "The app must not turn its observation deadline into SIGTERM")
        XCTAssertNotNil(f.controller.forceStopTarget)
    }

    func testRunnerNaturalExitEndsForceWaitButDoesNotConfirmCleanup() async throws {
        let f = try Fixture(); defer { f.clean() }
        let ticket = try await stopAfterLoss(f)
        try await f.exit()
        XCTAssertEqual(f.process.terminationReason, .exit)
        XCTAssertEqual(f.process.terminationStatus, 7, "The synthetic runner must exit naturally, not from Stop's SIGTERM")
        XCTAssertNil(f.controller.forceStopTarget, "Runner exit ends the escalation wait")
        XCTAssertNotNil(ticket.observation.runnerExit)
        XCTAssertFalse(ticket.observation.isComplete)
        XCTAssertTrue(f.controller.mayHaveOwnedWork, "No cleanup receipt means ownership remains unconfirmed")
    }

    func testForceStopDuringOwnerEOFWaitTerminatesRetainedRunner() async throws {
        let f = try Fixture(); defer { f.clean() }
        let ticket = try await stopAfterLoss(f)
        let target = try XCTUnwrap(f.controller.forceStopTarget, "Force Stop remains offered during owner-EOF wait")
        XCTAssertEqual(f.controller.requestForceStop(target), .requested)
        let limit = ProcessInfo.processInfo.systemUptime + 2
        while f.process.isRunning, ProcessInfo.processInfo.systemUptime < limit {
            try await Task.sleep(nanoseconds: 5_000_000)
        }
        XCTAssertFalse(f.process.isRunning)
        XCTAssertEqual(f.process.terminationReason, .uncaughtSignal)
        XCTAssertEqual(f.process.terminationStatus, 15)
        f.controller.tick()
        XCTAssertFalse(ticket.observation.isComplete)
    }
}
