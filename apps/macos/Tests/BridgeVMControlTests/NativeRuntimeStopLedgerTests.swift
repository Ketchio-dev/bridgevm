import Foundation
import XCTest
@testable import BridgeVMControl

final class NativeRuntimeStopLedgerTests: XCTestCase {
    private typealias Fixture = NativeRuntimeControlTestSupport

    @MainActor private func ticket(_ request: NativeRuntimeControlRequest) -> HvfOwnedStopOperation {
        .init(operationID: UUID(uuidString: request.operationID)!,
              target: .init(token: UUID(uuidString: request.target.runToken)!, processID: request.target.processID),
              now: 10, deadline: 202, phase: .guestGrace)
    }

    @MainActor func testReplayJoinAndTargetConflictNeverReadmit() throws {
        let ledger = NativeRuntimeStopLedger(appInstanceID: Fixture.app, library: Fixture.library)
        let request = Fixture.request(), operation = ticket(request)
        var admissions = 0
        let first = try ledger.process(request) { admissions += 1; return .accepted(operation) }
        XCTAssertEqual(first.disposition, .accepted)
        for repeated in [Fixture.request(id: request.operationID), Fixture.request()] {
            let result = try ledger.process(repeated) { admissions += 1; return .accepted(self.ticket(repeated)) }
            XCTAssertEqual(result.disposition, .existing)
            XCTAssertEqual(result.observation?.operationID, request.operationID)
        }
        let changed = Fixture.request(id: request.operationID, vmID: "other-vm")
        XCTAssertEqual(try ledger.process(changed) { XCTFail("conflict mutated"); return .accepted(operation) }.refusal, .operationConflict)
        XCTAssertEqual(admissions, 1)
    }

    @MainActor func testReservationPrecedesReentryAndSurvivesModelRemoval() throws {
        let ledger = NativeRuntimeStopLedger(appInstanceID: Fixture.app, library: Fixture.library)
        let request = Fixture.request(), operation = ticket(request)
        _ = try ledger.process(request) {
            let nested = try ledger.process(Fixture.request(id: request.operationID)) {
                XCTFail("reentrant admission"); return .accepted(operation)
            }
            XCTAssertEqual(nested.refusal, .operationUnavailable)
            return .accepted(operation)
        }
        operation.update(phase: .unconfirmed, complete: nil, exit: nil, failure: .cleanupUnconfirmed)
        let status = Fixture.request(.stopStatus, id: request.operationID)
        let retained = try ledger.process(status) { XCTFail("queried removed model"); return .refused(.notOwned) }
        XCTAssertEqual(retained.disposition, .status)
        XCTAssertEqual(retained.observation?.phase, .unconfirmed)
        XCTAssertEqual(retained.observation?.failure, .cleanupUnconfirmed)
    }

    /// Every run has its own target. A full ledger used to refuse every stop after 256 runs.
    @MainActor func testThreeHundredStopsInOneSessionAreAdmittedAndRecentStatusStillAnswers() throws {
        let ledger = NativeRuntimeStopLedger(appInstanceID: Fixture.app, library: Fixture.library)
        let pending = Fixture.request(vmID: "other-vm"), pendingTicket = ticket(pending)
        _ = try ledger.process(pending) { .accepted(pendingTicket) }
        var requests: [NativeRuntimeControlRequest] = []
        for _ in 0..<300 {
            let request = Fixture.request(target: .init(appInstanceID: Fixture.app, runToken: UUID().uuidString,
                processID: 123, acceptedConfigurationDigest: Fixture.target.acceptedConfigurationDigest))
            let operation = ticket(request)
            XCTAssertEqual(try ledger.process(request) { .accepted(operation) }.disposition, .accepted)
            operation.update(phase: .completed, complete: nil, exit: nil, failure: nil)
            requests.append(request)
        }
        let status = { (request: NativeRuntimeControlRequest) in
            try ledger.process(Fixture.request(.stopStatus, id: request.operationID, target: request.target, vmID: request.vmID)) {
                XCTFail("status admitted"); return .refused(.notOwned)
            }
        }
        XCTAssertEqual(try status(requests[299]).observation?.phase, .completed)
        XCTAssertEqual(try status(pending).observation?.phase, .guestGrace, "an unfinished stop is never dropped")
        XCTAssertEqual(try status(requests[0]).refusal, .operationUnknown)
    }

    @MainActor func testUnknownStatusOwnerChangeAndRefusalDoNotReserveOperations() throws {
        let ledger = NativeRuntimeStopLedger(appInstanceID: Fixture.app, library: Fixture.library)
        let request = Fixture.request()
        XCTAssertEqual(try ledger.process(Fixture.request(.stopStatus)) { XCTFail(); return .refused(.notOwned) }.refusal, .operationUnknown)
        let changed = Fixture.request(target: .init(appInstanceID: UUID().uuidString, runToken: Fixture.token,
            processID: 123, acceptedConfigurationDigest: Fixture.target.acceptedConfigurationDigest))
        XCTAssertEqual(try ledger.process(changed) { XCTFail(); return .refused(.notOwned) }.refusal, .ownerChanged)
        XCTAssertEqual(try ledger.process(request) { .refused(.supervisionUnavailable) }.refusal, .unsupportedRuntime)
        XCTAssertEqual(try ledger.process(request) { .accepted(self.ticket(request)) }.disposition, .accepted)
    }
}
