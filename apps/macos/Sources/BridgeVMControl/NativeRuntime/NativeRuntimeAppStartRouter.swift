import Foundation

@MainActor
final class NativeRuntimeAppStartRouter {
    private let ledger: NativeRuntimeStartLedger
    private let validateOwner: () throws -> Void
    private let retainedModel: () -> LibraryModel?

    init(appInstanceID: String, library: NativeRuntimeLibraryIdentity,
         validateOwner: @escaping () throws -> Void, retainedModel: @escaping () -> LibraryModel?) {
        ledger = .init(appInstanceID: appInstanceID, library: library)
        self.validateOwner = validateOwner; self.retainedModel = retainedModel
    }

    func handle(_ request: NativeRuntimeStartRequest, context: NativeRuntimeRequestContext) throws -> NativeRuntimeStartResponse {
        try NativeRuntimeStartCodec.validate(request)
        guard NativeLibraryReader.isCanonicalID(request.vmID) else { throw NativeRuntimeError.invalidMessage }
        try context.validateAdmission()
        try validateOwner()
        try context.validateAdmission()
        return try ledger.process(request) { onReserved in
            guard let model = retainedModel() else { throw NativeRuntimeStartRefusal.modelUnavailable }
            return try model.requestOwnedRuntimeStart(request, validateOwner: validateOwner) { ticket in
                try context.validateAdmission()
                try self.validateOwner()
                try context.validateAdmission()
                try onReserved(ticket)
            }
        }
    }
}
