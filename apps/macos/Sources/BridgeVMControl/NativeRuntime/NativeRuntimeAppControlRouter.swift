import Foundation

@MainActor
final class NativeRuntimeAppControlRouter {
    private let ledger: NativeRuntimeStopLedger
    private let validateOwner: () throws -> Void
    private let retainedModel: () -> LibraryModel?

    init(appInstanceID: String, library: NativeRuntimeLibraryIdentity,
         validateOwner: @escaping () throws -> Void, retainedModel: @escaping () -> LibraryModel?) {
        ledger = .init(appInstanceID: appInstanceID, library: library)
        self.validateOwner = validateOwner; self.retainedModel = retainedModel
    }

    func handle(_ request: NativeRuntimeControlRequest, context: NativeRuntimeRequestContext) throws -> NativeRuntimeControlResponse {
        try NativeRuntimeControlCodec.validate(request)
        guard NativeLibraryReader.isCanonicalID(request.vmID) else { throw NativeRuntimeError.invalidMessage }
        try context.validateAdmission()
        try validateOwner()
        try context.validateAdmission()
        return try ledger.process(request) {
            guard let model = retainedModel() else { throw NativeRuntimeStopRefusal.modelUnavailable }
            return try model.requestOwnedRuntimeStop(request)
        }
    }
}
