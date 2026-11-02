import Combine
import Foundation
import XCTest
@testable import BridgeVMControl

/// Force stop is an escalation of one owned stop, never a replacement run or clean-shutdown proof.
@MainActor
final class HvfForceStopTests: XCTestCase {
    private typealias Fixture = HvfOwnedRuntimeControllerTests.Fixture

    func testExactWaitingOperationEscalatesOnceWithoutReleasingOwnership() throws {
        let f = try Fixture(); defer { f.clean() }
        f.controller.receive(.message(try f.event("ready", sequence: 1)))
        let ticket = try f.stop()
        let target = try XCTUnwrap(f.controller.forceStopTarget)
        let accepted = ticket.observation.acceptedUptime
        f.clock.value = 105

        XCTAssertEqual(f.controller.requestForceStop(target), .requested)
        XCTAssertTrue(f.controller.operation === ticket)
        XCTAssertEqual(ticket.observation.operationID, target.operationID)
        XCTAssertEqual(ticket.observation.target, target.identity)
        XCTAssertEqual(ticket.observation.acceptedUptime, accepted)
        XCTAssertEqual(ticket.observation.phase, .cancelling)
        XCTAssertEqual(ticket.observation.deadlineUptime, 117)
        XCTAssertTrue(f.controller.mayHaveOwnedWork)
        XCTAssertFalse(ticket.observation.isComplete)
        XCTAssertNil(ticket.observation.supervisorComplete)
        XCTAssertNil(ticket.observation.runnerExit)
        XCTAssertNil(f.controller.forceStopTarget)

        f.clock.value = 106
        XCTAssertEqual(f.controller.requestForceStop(target), .refused(.notWaiting))
        XCTAssertTrue(f.controller.operation === ticket)
        XCTAssertEqual(ticket.observation.deadlineUptime, 117, "A repeated confirmation must not renew the deadline")
    }

    func testWrongTargetOrOperationAndNonwaitingPhasesAreRefused() async throws {
        let f = try Fixture(); defer { f.clean() }
        f.controller.receive(.message(try f.event("ready", sequence: 1)))
        let runningTarget = HvfForceStopTarget(identity: f.controller.identity, operationID: UUID())
        XCTAssertNil(f.controller.forceStopTarget)
        XCTAssertEqual(f.controller.requestForceStop(runningTarget), .refused(.operationChanged))
        XCTAssertNil(f.controller.operation, "Force stop must not create an operation from running")
        let ticket = try f.stop()
        let target = try XCTUnwrap(f.controller.forceStopTarget)
        let invalid: [(HvfForceStopTarget, HvfForceStopResult)] = [
            (.init(identity: .init(token: UUID(), processID: target.identity.processID), operationID: target.operationID), .refused(.targetChanged)),
            (.init(identity: .init(token: target.identity.token, processID: -1), operationID: target.operationID), .refused(.targetChanged)),
            (.init(identity: target.identity, operationID: UUID()), .refused(.operationChanged)),
        ]
        for (stale, refusal) in invalid {
            XCTAssertEqual(f.controller.requestForceStop(stale), refusal)
            XCTAssertEqual(ticket.observation.phase, .guestGrace)
            XCTAssertEqual(ticket.observation.deadlineUptime, 292)
            XCTAssertEqual(f.controller.forceStopTarget, target)
        }

        f.controller.receive(.message(try f.event("complete-not-admitted", sequence: 2, success: true)))
        XCTAssertEqual(ticket.observation.phase, .awaitingRunnerExit)
        XCTAssertNil(f.controller.forceStopTarget)
        XCTAssertEqual(f.controller.requestForceStop(target), .refused(.notWaiting))
        try await f.exit()
        f.controller.receive(.eof)
        XCTAssertEqual(ticket.observation.phase, .completed)
        XCTAssertEqual(f.controller.requestForceStop(target), .refused(.notWaiting))
    }

    func testAvailabilityPublishesWhenAlreadyStoppingThenClearsOnEscalation() throws {
        let f = try Fixture(); defer { f.clean() }
        let session = bind(f)
        session.connectionState = .stopping
        var targets: [HvfForceStopTarget?] = []
        var stateChanges = 0
        let targetObserver = session.$forceStopTarget.dropFirst().sink { targets.append($0) }
        let stateObserver = session.$connectionState.dropFirst().sink { _ in stateChanges += 1 }
        defer { targetObserver.cancel(); stateObserver.cancel() }
        f.controller.receive(.message(try f.event("ready", sequence: 1)))
        XCTAssertNil(session.forceStopTarget)

        let ticket = try f.stop()
        let target = try XCTUnwrap(session.forceStopTarget)
        XCTAssertEqual(target.operationID, ticket.operationID)
        XCTAssertEqual(targets, [target])
        XCTAssertEqual(stateChanges, 0, "The phase publication must not depend on a new connection state")
        XCTAssertEqual(session.requestForceStop(target), .requested)
        XCTAssertNil(session.forceStopTarget)
        XCTAssertEqual(targets, [target, nil])
        XCTAssertEqual(session.connectionState, .stopping)
        XCTAssertEqual(stateChanges, 0)
        XCTAssertTrue(f.controller.mayHaveOwnedWork)
    }

    func testCapturedConfirmationCannotRetargetAReplacementRun() throws {
        let old = try Fixture(); defer { old.clean() }
        let replacement = try Fixture(); defer { replacement.clean() }
        let session = bind(old)
        session.connectionState = .stopping
        old.controller.receive(.message(try old.event("ready", sequence: 1)))
        let oldTicket = try old.stop()
        let captured = try XCTUnwrap(session.forceStopTarget)

        attach(replacement, to: session)
        replacement.controller.receive(.message(try replacement.event("ready", sequence: 1)))
        let replacementTicket = try replacement.stop()
        let current = try XCTUnwrap(session.forceStopTarget)
        XCTAssertNotEqual(captured, current)
        XCTAssertEqual(session.requestForceStop(captured), .refused(.targetChanged))
        XCTAssertEqual(session.forceStopTarget, current)
        XCTAssertEqual(replacementTicket.observation.phase, .guestGrace)
        XCTAssertEqual(replacementTicket.observation.deadlineUptime, 292)
        XCTAssertEqual(oldTicket.observation.phase, .guestGrace)
    }

    private func bind(_ fixture: Fixture) -> HvfEngineSession {
        let session = HvfEngineSession(config: fixture.base.config, repoRoot: fixture.base.root,
            processIsRunning: { _ in false }, vtpmKeyProvider: fixture.base.keys)
        attach(fixture, to: session)
        return session
    }

    private func attach(_ fixture: Fixture, to session: HvfEngineSession) {
        session.process = fixture.process
        session.ownedProcessIdentity = fixture.controller.identity
        session.ownedController = fixture.controller
        let token = fixture.controller.identity.token
        fixture.controller.onChange = { [weak session] in session?.ownedRuntimeChanged(token: token) }
        session.refreshForceStopTarget()
    }
}
