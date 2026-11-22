import Foundation

/// A CLI start is the library's own Start: the GUI start ticket and its session, or the refusal sentence.
@MainActor
enum NativeRuntimeStartAdmission {
    case accepted(HvfGUIStartOperation, HvfEngineSession)
    case refused(String)
}

@MainActor
final class NativeRuntimeStartLedger {
    private struct Record {
        let vmID: String
        let serial: Int
        let ticket: HvfGUIStartOperation?
        weak var session: HvfEngineSession?
        let refusal: String?
        /// Startup proof is history: kept once seen, even after the run stops.
        var proof: NativeRuntimeStartProof?
    }
    private let appInstanceID: String
    private let library: NativeRuntimeLibraryIdentity
    /// Finished records kept for status queries; the oldest are dropped. A start is never refused for history.
    private let history: Int
    private var records: [String: Record] = [:]
    private var serial = 0

    init(appInstanceID: String, library: NativeRuntimeLibraryIdentity, history: Int = 256) {
        self.appInstanceID = appInstanceID; self.library = library; self.history = history
    }

    func process(_ request: NativeRuntimeStartRequest,
                 admit: (_ vmID: String) -> NativeRuntimeStartAdmission) -> NativeRuntimeStartResponse {
        guard request.appInstanceID == appInstanceID else {
            return refused(request, .ownerChanged, "The BridgeVM app restarted during this start; run the command again.")
        }
        if var known = records[request.operationID] {
            guard known.vmID == request.vmID else {
                return refused(request, .refused, "This start operation belongs to another VM; run the command again.")
            }
            let response = reply(request, &known, disposition: request.operation == .startStatus ? .status : .existing)
            records[request.operationID] = known
            return response
        }
        if request.operation == .startStatus {
            return refused(request, .operationUnknown, "The app has no record of this start; run the command again.")
        }
        evictFinished()
        serial += 1
        var record: Record
        switch admit(request.vmID) {
        case let .accepted(ticket, session):
            record = .init(vmID: request.vmID, serial: serial, ticket: ticket, session: session, refusal: nil)
        case let .refused(reason):
            record = .init(vmID: request.vmID, serial: serial, ticket: nil, session: nil, refusal: reason)
        }
        let response = reply(request, &record, disposition: .accepted)
        records[request.operationID] = record
        return response
    }

    /// Refused, started or failed: the CLI's wait has ended, so the record is history.
    private func evictFinished() {
        var finished: [(String, Int)] = []
        for (id, var record) in records {
            let phase = record.ticket.map { Self.observe($0, record: &record, operationID: id).phase }
            if phase == nil || phase == .started || phase == .failed { finished.append((id, record.serial)) }
        }
        for (id, _) in finished.sorted(by: { $0.1 < $1.1 }).prefix(max(0, finished.count - history + 1)) {
            records[id] = nil
        }
    }

    private func reply(_ request: NativeRuntimeStartRequest, _ record: inout Record,
                       disposition: NativeRuntimeStartResponse.Disposition) -> NativeRuntimeStartResponse {
        guard let ticket = record.ticket else {
            return refused(request, .refused, record.refusal ?? "Windows를 시작하지 못했습니다.")
        }
        let observation = Self.observe(ticket, record: &record, operationID: request.operationID)
        return response(request, disposition: disposition, observation: observation, refusal: nil, reason: nil)
    }

    private static func observe(_ ticket: HvfGUIStartOperation, record: inout Record,
                                operationID: String) -> NativeRuntimeStartObservation {
        var target: NativeRuntimeProcessObservation?
        func make(_ phase: NativeRuntimeStartObservation.Phase, failure: String? = nil) -> NativeRuntimeStartObservation {
            .init(operationID: operationID, phase: phase, target: target,
                  proof: phase == .started ? record.proof : nil, failure: failure)
        }
        guard let outcome = ticket.outcome else { return make(.preparing) }
        guard case let .ownedLaunchAccepted(identity) = outcome else {
            return make(.failed, failure: outcome.failureText ?? "Windows가 이미 실행 중입니다.")
        }
        target = .init(token: identity.token.uuidString, processID: identity.processID)
        if record.proof != nil { return make(.started) }
        let session = record.session
        guard let controller = session?.ownedController, controller.identity == identity else {
            // A debug-only legacy launch has no supervisor to confirm startup; it is still running.
            if session?.ownedController == nil, session?.process?.isRunning == true { return make(.awaitingStartup) }
            return make(.failed, failure: "Windows가 시작을 확인하기 전에 종료되었으니 잠시 후 다시 시작하세요.")
        }
        if controller.ledger.ready, let child = controller.ledger.initialHelper, let target {
            record.proof = .init(target: target, manifestSHA256: controller.ledger.manifestSHA256, helperPID: child.pid)
            return make(.started)
        }
        if controller.failure != nil || controller.runnerExit != nil || controller.ledger.complete != nil {
            return make(.failed, failure: session?.guiStartFailureText
                ?? "Windows가 시작을 확인하기 전에 종료되었으니 잠시 후 다시 시작하세요.")
        }
        return make(.awaitingStartup)
    }

    private func refused(_ request: NativeRuntimeStartRequest, _ refusal: NativeRuntimeStartRefusal,
                         _ reason: String) -> NativeRuntimeStartResponse {
        response(request, disposition: .refused, observation: nil, refusal: refusal, reason: reason)
    }
    private func response(_ request: NativeRuntimeStartRequest, disposition: NativeRuntimeStartResponse.Disposition,
                          observation: NativeRuntimeStartObservation?, refusal: NativeRuntimeStartRefusal?,
                          reason: String?) -> NativeRuntimeStartResponse {
        .init(schema: NativeRuntimeStartCodec.responseSchema, scope: NativeRuntimeStartCodec.scope,
            requestID: request.requestID, library: library, vmID: request.vmID, appInstanceID: appInstanceID,
            requestedOperationID: request.operationID, disposition: disposition, observation: observation,
            refusal: refusal, reason: reason)
    }
}
