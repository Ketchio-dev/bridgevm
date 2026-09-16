import Foundation

@MainActor
final class NativeRuntimeStopLedger {
    private struct Record {
        let vmID: String
        let target: NativeRuntimeStopTarget
        let ticket: HvfOwnedStopOperation?
    }
    private let appInstanceID: String
    private let library: NativeRuntimeLibraryIdentity
    private let capacity: Int
    private var records: [String: Record] = [:]

    init(appInstanceID: String, library: NativeRuntimeLibraryIdentity, capacity: Int = 256) {
        self.appInstanceID = appInstanceID; self.library = library; self.capacity = capacity
    }

    func process(_ request: NativeRuntimeControlRequest,
                 admit: () throws -> HvfOwnedStopAdmission) throws -> NativeRuntimeControlResponse {
        try NativeRuntimeControlCodec.validate(request)
        guard request.library == library else { throw NativeRuntimeError.libraryChanged }
        guard request.target.appInstanceID == appInstanceID else { return reply(request, refusal: .ownerChanged) }
        if let known = records[request.operationID] {
            guard known.vmID == request.vmID, known.target == request.target else {
                return reply(request, refusal: .operationConflict)
            }
            return try reply(request, record: known, disposition: request.operation == .stopStatus ? .status : .existing)
        }
        if request.operation == .stopStatus { return reply(request, refusal: .operationUnknown) }
        if let known = records.values.first(where: { $0.vmID == request.vmID && $0.target == request.target }) {
            return try reply(request, record: known, disposition: .existing)
        }
        guard records.count < capacity else { return reply(request, refusal: .ledgerFull) }
        records[request.operationID] = Record(vmID: request.vmID, target: request.target, ticket: nil)
        do {
            let admission = try admit()
            let ticket: HvfOwnedStopOperation
            let disposition: NativeRuntimeControlResponse.Disposition
            switch admission {
            case .accepted(let value): ticket = value; disposition = .accepted
            case .existing(let value): ticket = value; disposition = .existing
            case .refused(let reason):
                records.removeValue(forKey: request.operationID)
                let refusal: NativeRuntimeStopRefusal = reason == .operationConflict ? .operationConflict
                    : reason == .supervisionUnavailable ? .unsupportedRuntime : .notOwned
                return reply(request, refusal: refusal)
            }
            guard ticket.target.token.uuidString == request.target.runToken,
                  ticket.target.processID == request.target.processID,
                  disposition == .existing || ticket.operationID.uuidString == request.operationID else {
                throw NativeRuntimeError.invalidMessage
            }
            if let known = records[ticket.operationID.uuidString], let retained = known.ticket {
                guard known.vmID == request.vmID, known.target == request.target, retained === ticket else {
                    throw NativeRuntimeStopRefusal.operationConflict
                }
            }
            let record = Record(vmID: request.vmID, target: request.target, ticket: ticket)
            records.removeValue(forKey: request.operationID)
            records[ticket.operationID.uuidString] = record
            return try reply(request, record: record, disposition: disposition)
        } catch {
            if records[request.operationID]?.ticket == nil { records.removeValue(forKey: request.operationID) }
            if let refusal = error as? NativeRuntimeStopRefusal { return reply(request, refusal: refusal) }
            throw error
        }
    }

    private func reply(_ request: NativeRuntimeControlRequest, record: Record,
                       disposition: NativeRuntimeControlResponse.Disposition) throws -> NativeRuntimeControlResponse {
        guard let ticket = record.ticket else { return reply(request, refusal: .operationUnavailable) }
        return response(request, disposition: disposition, observation: try .init(ticket.observation), refusal: nil)
    }

    private func reply(_ request: NativeRuntimeControlRequest, refusal: NativeRuntimeStopRefusal) -> NativeRuntimeControlResponse {
        response(request, disposition: .refused, observation: nil, refusal: refusal)
    }

    private func response(_ request: NativeRuntimeControlRequest, disposition: NativeRuntimeControlResponse.Disposition,
                          observation: NativeRuntimeStopObservation?, refusal: NativeRuntimeStopRefusal?) -> NativeRuntimeControlResponse {
        .init(schema: NativeRuntimeControlCodec.responseSchema, scope: NativeRuntimeControlCodec.scope,
              requestID: request.requestID, library: library, vmID: request.vmID, appInstanceID: appInstanceID,
              requestedOperationID: request.operationID, target: request.target,
              disposition: disposition, observation: observation, refusal: refusal)
    }
}
