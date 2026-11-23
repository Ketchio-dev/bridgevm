import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class NativeRuntimeStartLedgerTests: XCTestCase {
    private typealias F = NativeRuntimeStartTestSupport

    private func accepted() -> (NativeRuntimeStartAdmission, HvfGUIStartOperation, HvfEngineSession) {
        let session = HvfEngineSession(config: F.configuration, repoRoot: URL(fileURLWithPath: "/nonexistent")) { _ in false }
        let ticket = HvfGUIStartOperation(configuration: F.configuration)
        return (.accepted(ticket, session), ticket, session)
    }

    func testRetryAndStatusReturnTheSameTicketAndItsRealFailureSentence() throws {
        let ledger = NativeRuntimeStartLedger(appInstanceID: F.app, library: F.library)
        let request = F.request(), (admission, ticket, session) = accepted()
        var admissions = 0
        XCTAssertEqual(ledger.process(request) { _ in admissions += 1; return admission }.observation?.phase, .preparing)
        let retry = ledger.process(F.request(id: request.operationID)) { _ in admissions += 1; return admission }
        XCTAssertEqual(retry.disposition, .existing); XCTAssertEqual(admissions, 1)
        ticket.finish(.failed(.processLaunch, "The runner could not be opened."))
        let status = ledger.process(F.request(.startStatus, id: request.operationID)) { _ in XCTFail(); return admission }
        XCTAssertEqual(status.disposition, .status); XCTAssertEqual(status.observation?.phase, .failed)
        XCTAssertEqual(status.observation?.failure?.hasSuffix("The runner could not be opened."), true)
        XCTAssertEqual(ledger.process(F.request(id: request.operationID, vmID: "other")) { _ in XCTFail(); return admission }.refusal, .refused)
        withExtendedLifetime(session) {}
    }

    func testRefusalCarriesTheStartPathSentence() {
        let ledger = NativeRuntimeStartLedger(appInstanceID: F.app, library: F.library)
        let response = ledger.process(F.request()) { _ in .refused("VM 작업이 진행 중입니다. 완료 후 다시 시도하세요.") }
        XCTAssertEqual(response.refusal, .refused)
        XCTAssertEqual(response.reason, "VM 작업이 진행 중입니다. 완료 후 다시 시도하세요.")
    }

    /// Every CLI start has a fresh operation ID, so a full ledger must not refuse starts after 256.
    func testThreeHundredStartsInOneSessionAreAdmittedAndRecentStatusStillAnswers() throws {
        let ledger = NativeRuntimeStartLedger(appInstanceID: F.app, library: F.library)
        let pending = F.request(vmID: "other-vm"), (pendingAdmission, _, pendingSession) = accepted()
        _ = ledger.process(pending) { _ in pendingAdmission }
        var requests: [NativeRuntimeStartRequest] = []
        for _ in 0..<300 {
            let request = F.request(), (admission, ticket, _) = accepted()
            XCTAssertEqual(ledger.process(request) { _ in admission }.disposition, .accepted)
            ticket.finish(.refused("stopped"))
            requests.append(request)
        }
        let status = { (request: NativeRuntimeStartRequest) in
            ledger.process(F.request(.startStatus, id: request.operationID, vmID: request.vmID)) { _ in
                XCTFail(); return .refused("")
            }
        }
        XCTAssertEqual(status(requests[299]).observation?.failure, "stopped")
        XCTAssertEqual(status(pending).observation?.phase, .preparing, "an unfinished start is never dropped")
        XCTAssertEqual(status(requests[0]).refusal, .operationUnknown)
        withExtendedLifetime(pendingSession) {}
    }

    func testUnknownStatusAndOtherOwnerNeverEnterModel() {
        let ledger = NativeRuntimeStartLedger(appInstanceID: F.app, library: F.library)
        XCTAssertEqual(ledger.process(F.request(.startStatus)) { _ in XCTFail(); return .refused("") }.refusal, .operationUnknown)
        XCTAssertEqual(ledger.process(F.request(app: UUID().uuidString)) { _ in XCTFail(); return .refused("") }.refusal, .ownerChanged)
    }
}
