import Foundation

extension LibraryModel {
    func requestOwnedInstall(_ request: NativeInstallControlRequest,
                             validateOwner: @escaping () throws -> Void) throws -> NativeInstallModelResult {
        try NativeInstallControlCodec.validate(request)
        guard NativeLibraryReader.isCanonicalID(request.vmID) else { throw NativeRuntimeError.invalidMessage }
        try validateOwner()
        let library = try NativeRuntimeLibraryHandle.open(rootURL: rootURL, create: false)
        guard library.identity == request.library else { throw NativeInstallControlRefusal.configurationChanged }
        let saved = try validatedInstallConfiguration(request, library: library)
        let store = windowsInstallSessions.cliOperations
        switch request.operation {
        case .install:
            guard let id = request.operationID.flatMap(UUID.init(uuidString:)) else {
                throw NativeRuntimeError.invalidMessage
            }
            switch store.reserve(vmID: saved.slug, digest: request.expectedSavedConfigurationDigest,
                                 operationID: id) {
            case let .accepted(operation):
                guard libraryWorkRefusal(slug: saved.slug, excludingInstall: operation,
                    owns: { _, current in current == saved }) == nil else {
                    operation.fail("이 VM의 다른 작업이 진행 중입니다.")
                    throw NativeInstallControlRefusal.busy
                }
                beginOwnedInstallPreparation(request, saved: saved, library: library,
                    operation: operation, validateOwner: validateOwner)
                return .init(disposition: .accepted, operation: operation)
            case let .existing(operation): return .init(disposition: .existing, operation: operation)
            case let .refused(refusal): throw refusal
            }
        case .installStatus:
            return try existingInstallResult(store, request: request, disposition: .status)
        case .installCancel:
            let result = try existingInstallResult(store, request: request, disposition: .cancelAccepted)
            guard result.operation.cancel() else { throw NativeInstallControlRefusal.cancellationUnavailable }
            return result
        }
    }

    private func existingInstallResult(_ store: NativeInstallOperationStore,
        request: NativeInstallControlRequest, disposition: NativeInstallControlResponse.Disposition
    ) throws -> NativeInstallModelResult {
        switch store.operation(vmID: request.vmID, digest: request.expectedSavedConfigurationDigest) {
        case let .success(operation): return .init(disposition: disposition, operation: operation)
        case let .failure(refusal): throw refusal
        }
    }
}
