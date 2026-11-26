import Foundation

@MainActor
final class NativeRuntimeAppStartRouter {
    private let ledger: NativeRuntimeStartLedger
    private let retainedModel: () -> LibraryModel?

    init(appInstanceID: String, library: NativeRuntimeLibraryIdentity, retainedModel: @escaping () -> LibraryModel?) {
        ledger = .init(appInstanceID: appInstanceID, library: library)
        self.retainedModel = retainedModel
    }

    /// The request was decoded and checked once by the server (peer, size, library); it is trusted here.
    func handle(_ request: NativeRuntimeStartRequest) throws -> NativeRuntimeStartResponse {
        guard NativeLibraryReader.isCanonicalID(request.vmID) else { throw NativeRuntimeError.invalidMessage }
        return ledger.process(request) { vmID in
            guard let model = retainedModel() else {
                return .refused("Open the VM library in BridgeVM, then try again.")
            }
            return model.requestCLIStart(vmID: vmID)
        }
    }
}
