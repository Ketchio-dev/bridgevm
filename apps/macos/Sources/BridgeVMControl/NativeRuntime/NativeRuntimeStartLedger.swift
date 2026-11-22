import Foundation

@MainActor
final class NativeRuntimeStartLedger {
    private struct Record {
        let vmID: String
        let digest: String
        var ticket: HvfOwnedStartOperation?
        var refusal: NativeRuntimeStartRefusal?
        var admitting: Bool
        let serial: Int
        /// Refused, started, failed or unconfirmed: the CLI's wait has ended, so the record is history.
        @MainActor var finished: Bool {
            !admitting && (refusal != nil || ticket.map { ![.preparing, .awaitingStartup].contains($0.observation.phase) } ?? true)
        }
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
                 admit: (_ onReserved: @MainActor (HvfOwnedStartOperation) throws -> Void) throws -> HvfOwnedStartAdmission
    ) throws -> NativeRuntimeStartResponse {
        try NativeRuntimeStartCodec.validate(request)
        guard request.library == library else { throw NativeRuntimeError.libraryChanged }
        guard request.appInstanceID == appInstanceID else { return reply(request, refusal: .ownerChanged) }
        if let known = records[request.operationID] {
            guard known.vmID == request.vmID, known.digest == request.expectedSavedConfigurationDigest else {
                return reply(request, refusal: .operationConflict)
            }
            return try reply(request, record: known, disposition: request.operation == .startStatus ? .status : .existing)
        }
        if request.operation == .startStatus { return reply(request, refusal: .operationUnknown) }
        let finished = records.filter { $0.value.finished }.sorted { $0.value.serial < $1.value.serial }
        for (id, _) in finished.prefix(max(0, finished.count - history + 1)) { records[id] = nil }
        let reentrant = records.values.contains { $0.vmID == request.vmID && $0.admitting }
        serial += 1
        records[request.operationID] = .init(vmID: request.vmID, digest: request.expectedSavedConfigurationDigest,
            ticket: nil, refusal: reentrant ? .busy : nil, admitting: !reentrant, serial: serial)
        if reentrant { return reply(request, refusal: .busy) }
        do {
            let admission = try admit { ticket in try self.reserve(ticket, for: request) }
            switch admission {
            case .accepted(let ticket), .existing(let ticket):
                guard let retained = records[request.operationID]?.ticket, retained === ticket else {
                    throw NativeRuntimeError.invalidMessage
                }
                records[request.operationID]?.admitting = false
                return try reply(request, record: records[request.operationID]!,
                                 disposition: admission.isExisting ? .existing : .accepted)
            case .refused(let reason):
                guard let refusal = NativeRuntimeStartRefusal(rawValue: reason.rawValue) else {
                    throw NativeRuntimeError.invalidMessage
                }
                records[request.operationID]?.refusal = refusal
                records[request.operationID]?.admitting = false
                return reply(request, refusal: refusal)
            }
        } catch {
            let refusal = (error as? NativeRuntimeStartRefusal) ?? .admissionRefused
            records[request.operationID]?.refusal = refusal
            records[request.operationID]?.admitting = false
            if error is NativeRuntimeStartRefusal { return reply(request, refusal: refusal) }
            throw error
        }
    }

    private func reserve(_ ticket: HvfOwnedStartOperation, for request: NativeRuntimeStartRequest) throws {
        guard ticket.operationID.uuidString == request.operationID,
              ticket.expectedSavedConfigurationDigest == request.expectedSavedConfigurationDigest,
              records[request.operationID]?.admitting == true else { throw NativeRuntimeError.invalidMessage }
        if let known = records[request.operationID]?.ticket, known !== ticket { throw NativeRuntimeError.invalidMessage }
        _ = try NativeRuntimeStartObservation(ticket.observation)
        records[request.operationID]?.ticket = ticket
    }

    private func reply(_ request: NativeRuntimeStartRequest, record: Record,
                       disposition: NativeRuntimeStartResponse.Disposition) throws -> NativeRuntimeStartResponse {
        if let refusal = record.refusal { return reply(request, refusal: refusal) }
        guard let ticket = record.ticket else { return reply(request, refusal: .operationUnavailable) }
        return response(request, disposition: disposition, observation: try .init(ticket.observation), refusal: nil)
    }
    private func reply(_ request: NativeRuntimeStartRequest, refusal: NativeRuntimeStartRefusal) -> NativeRuntimeStartResponse {
        response(request, disposition: .refused, observation: nil, refusal: refusal)
    }
    private func response(_ request: NativeRuntimeStartRequest, disposition: NativeRuntimeStartResponse.Disposition,
                          observation: NativeRuntimeStartObservation?, refusal: NativeRuntimeStartRefusal?) -> NativeRuntimeStartResponse {
        .init(schema: NativeRuntimeStartCodec.responseSchema, scope: NativeRuntimeStartCodec.scope,
            requestID: request.requestID, library: library, vmID: request.vmID, appInstanceID: appInstanceID,
            expectedSavedConfigurationDigest: request.expectedSavedConfigurationDigest,
            requestedOperationID: request.operationID, disposition: disposition, observation: observation, refusal: refusal)
    }
}

private extension HvfOwnedStartAdmission {
    var isExisting: Bool { if case .existing = self { return true }; return false }
}
