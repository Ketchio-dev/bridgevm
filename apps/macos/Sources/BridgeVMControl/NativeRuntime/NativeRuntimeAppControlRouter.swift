import Foundation

@MainActor
final class NativeRuntimeAppControlRouter {
    private let ledger: NativeRuntimeStopLedger
    private let retainedModel: () -> LibraryModel?

    init(appInstanceID: String, library: NativeRuntimeLibraryIdentity, retainedModel: @escaping () -> LibraryModel?) {
        ledger = .init(appInstanceID: appInstanceID, library: library)
        self.retainedModel = retainedModel
    }

    /// The request was decoded and checked once by the server (peer, size, library); it is trusted here.
    func handle(_ request: NativeRuntimeControlRequest) throws -> NativeRuntimeControlResponse {
        guard NativeLibraryReader.isCanonicalID(request.vmID) else { throw NativeRuntimeError.invalidMessage }
        return try ledger.process(request, retainsTarget: { retainedModel()?.retainsOwnedStopTarget(request) ?? true }) {
            guard let model = retainedModel() else { throw NativeRuntimeStopRefused.modelUnavailable }
            return try model.requestOwnedRuntimeStop(request)
        }
    }
}
