import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfOwnedStartProofTests: XCTestCase {
    private func ticket(_ f: HvfOwnedRuntimeControllerTests.Fixture) -> HvfOwnedStartOperation {
        let ticket = HvfOwnedStartOperation(operationID: UUID(), configuration: f.base.config,
            expectedSavedConfigurationDigest: HvfOwnedStartTestSupport.digest, now: 100)
        ticket.finishWorker(target: f.controller.identity)
        return ticket
    }
    func testReadyIsNotStartProofButMatchingFirstHelperIs() throws {
        let f = try HvfOwnedRuntimeControllerTests.Fixture(); defer { f.clean() }
        let ticket = ticket(f)
        f.controller.receive(.message(try f.event("ready", sequence: 1)))
        ticket.observe(f.controller, now: 101)
        XCTAssertEqual(ticket.observation.phase, .awaitingStartup); XCTAssertNil(ticket.observation.proof)
        f.controller.receive(.message(try f.event("helper-started", sequence: 2)))
        ticket.observe(f.controller, now: 102)
        XCTAssertEqual(ticket.observation.phase, .started)
        XCTAssertEqual(ticket.observation.proof?.target, f.controller.identity)
        XCTAssertEqual(ticket.observation.proof?.helperGeneration, 0)
        XCTAssertEqual(ticket.observation.proof?.helperPID, 1236)
        XCTAssertEqual(ticket.observation.proof?.manifestSHA256, String(repeating: "a", count: 64))
        XCTAssertEqual(ticket.observation.expectedSavedConfigurationDigest, HvfOwnedStartTestSupport.digest)
    }
    func testFirstHelperProofAfterDeadlineCannotBecomeLateSuccess() throws {
        let f = try HvfOwnedRuntimeControllerTests.Fixture(); defer { f.clean() }
        let ticket = ticket(f)
        f.controller.receive(.message(try f.event("ready", sequence: 1)))
        f.controller.receive(.message(try f.event("helper-started", sequence: 2)))
        ticket.observe(f.controller, now: 131) // No earlier deadline tick.
        XCTAssertEqual(ticket.observation.phase, .unconfirmed)
        XCTAssertEqual(ticket.observation.failure, .deadlineExceeded); XCTAssertNil(ticket.proof)
        ticket.observe(f.controller, now: 132); XCTAssertNil(ticket.proof)
    }
    func testProtocolFailureAndEarlyRunnerExitNeverProduceSuccess() async throws {
        for invalidProtocol in [true, false] {
            let f = try HvfOwnedRuntimeControllerTests.Fixture(); defer { f.clean() }
            let ticket = ticket(f)
            if invalidProtocol { f.controller.receive(.message(try f.event("helper-started", sequence: 1))) }
            else { try await f.exit() }
            ticket.observe(f.controller, now: 101)
            XCTAssertNil(ticket.proof)
            XCTAssertTrue(ticket.failure == .protocolInvalid || ticket.failure == .runnerExited)
            XCTAssertEqual(ticket.observation.phase, .unconfirmed)
        }
    }
    func testStartedIsHistoricalProofAndIsNotRewrittenByLaterExit() async throws {
        let f = try HvfOwnedRuntimeControllerTests.Fixture(); defer { f.clean() }
        let ticket = ticket(f)
        f.controller.receive(.message(try f.event("ready", sequence: 1)))
        f.controller.receive(.message(try f.event("helper-started", sequence: 2)))
        ticket.observe(f.controller, now: 101)
        let proof = ticket.proof
        try await f.exit(); ticket.observe(f.controller, now: 200)
        XCTAssertEqual(ticket.observation.phase, .started); XCTAssertEqual(ticket.proof, proof)
        XCTAssertNil(ticket.failure)
    }
}
