import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class NativeRuntimeStartLedgerTests: XCTestCase {
    private typealias F = NativeRuntimeStartTestSupport

    func testReservationPrecedesPublicationAndReentryNeverLaunchesTwice() throws {
        let ledger = NativeRuntimeStartLedger(appInstanceID: F.app, library: F.library)
        let request = F.request(), ticket = F.ticket(request)
        var admissions = 0
        let response = try ledger.process(request) { reserve in
            admissions += 1
            XCTAssertEqual(try ledger.process(request) { _ in XCTFail(); return .refused(.busy) }.refusal, .operationUnavailable)
            let other = F.request()
            XCTAssertEqual(try ledger.process(other) { _ in XCTFail(); return .refused(.busy) }.refusal, .busy)
            try reserve(ticket)
            for operation in [NativeRuntimeStartRequest.Operation.start, .startStatus] {
                let nested = try ledger.process(F.request(operation, id: request.operationID)) { _ in XCTFail(); return .refused(.busy) }
                XCTAssertEqual(nested.observation?.phase, .preparing)
                XCTAssertEqual(nested.observation?.operationID, request.operationID)
            }
            return .accepted(ticket)
        }
        XCTAssertEqual(response.disposition, .accepted); XCTAssertEqual(admissions, 1)
    }

    func testExactRetryRetainsTicketAfterModelDisappearsAndBindsConfiguration() throws {
        let ledger = NativeRuntimeStartLedger(appInstanceID: F.app, library: F.library)
        let request = F.request(), ticket = F.ticket(request)
        _ = try ledger.process(request) { reserve in try reserve(ticket); return .accepted(ticket) }
        for altered in [F.request(id: request.operationID, vmID: "other"), F.request(id: request.operationID, digest: String(repeating: "b", count: 64))] {
            XCTAssertEqual(try ledger.process(altered) { _ in XCTFail(); return .refused(.busy) }.refusal, .operationConflict)
        }
        ticket.checkDeadline(now: 40)
        let pending = try ledger.process(F.request(.startStatus, id: request.operationID)) { _ in XCTFail(); return .refused(.busy) }
        XCTAssertEqual(pending.observation?.phase, .unconfirmed)
        ticket.finishWorker(target: nil)
        let result = try ledger.process(F.request(id: request.operationID)) { _ in XCTFail(); return .refused(.busy) }
        XCTAssertEqual(result.disposition, .existing); XCTAssertEqual(result.observation?.phase, .failed)
        XCTAssertEqual(result.observation?.failure, .deadlineExceeded)
        XCTAssertEqual(result.observation?.acceptedUptime, 10); XCTAssertEqual(result.observation?.deadlineUptime, 40)
    }

    func testHistoryNeverEvictsAtCapacityIncludingRefusedOutcomes() throws {
        let ledger = NativeRuntimeStartLedger(appInstanceID: F.app, library: F.library, capacity: 1)
        let request = F.request()
        XCTAssertEqual(try ledger.process(request) { _ in throw NativeRuntimeStartRefusal.configurationChanged }.refusal, .configurationChanged)
        XCTAssertEqual(try ledger.process(F.request(id: request.operationID)) { _ in XCTFail(); return .refused(.busy) }.refusal, .configurationChanged)
        XCTAssertEqual(try ledger.process(F.request()) { _ in XCTFail(); return .refused(.busy) }.refusal, .ledgerFull)
        XCTAssertEqual(try ledger.process(F.request(.startStatus, id: request.operationID)) { _ in XCTFail(); return .refused(.busy) }.refusal, .configurationChanged)
    }

    func testUnknownStatusAndOtherOwnerNeverEnterModel() throws {
        let ledger = NativeRuntimeStartLedger(appInstanceID: F.app, library: F.library, capacity: 1)
        XCTAssertEqual(try ledger.process(F.request(.startStatus)) { _ in XCTFail(); return .refused(.busy) }.refusal, .operationUnknown)
        XCTAssertEqual(try ledger.process(F.request(app: UUID().uuidString)) { _ in XCTFail(); return .refused(.busy) }.refusal, .ownerChanged)
        let request = F.request(), ticket = F.ticket(request)
        XCTAssertEqual(try ledger.process(request) { reserve in try reserve(ticket); return .existing(ticket) }.disposition, .existing)
    }

    func testWrongTicketCannotPassRegistrationOrPermitRetryEffects() throws {
        let ledger = NativeRuntimeStartLedger(appInstanceID: F.app, library: F.library)
        let request = F.request(), wrong = F.ticket(F.request())
        var effects = 0
        XCTAssertThrowsError(try ledger.process(request) { reserve in try reserve(wrong); effects += 1; return .accepted(wrong) })
        XCTAssertEqual(effects, 0)
        XCTAssertEqual(try ledger.process(request) { _ in effects += 1; return .refused(.busy) }.refusal, .admissionRefused)
        XCTAssertEqual(effects, 0)
    }

    func testResultMustBeTheIdenticalPreviouslyRegisteredTicket() throws {
        let ledger = NativeRuntimeStartLedger(appInstanceID: F.app, library: F.library)
        let request = F.request(), ticket = F.ticket(request), impostor = F.ticket(request)
        XCTAssertThrowsError(try ledger.process(request) { reserve in try reserve(ticket); return .accepted(impostor) })
        XCTAssertEqual(try ledger.process(F.request(.startStatus, id: request.operationID)) { _ in XCTFail(); return .refused(.busy) }.refusal, .admissionRefused)
    }
}
