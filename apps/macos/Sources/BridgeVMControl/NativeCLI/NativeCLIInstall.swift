import Foundation

enum NativeCLIInstall {
    static func run(rootURL: URL, id: String,
                    operation: NativeInstallControlRequest.Operation) -> NativeCLIInstallResult {
        let operationID = operation == .install ? UUID().uuidString : nil
        var appInstanceID: String?, libraryPath = rootURL.path
        do {
            let library = try NativeRuntimeLibraryHandle.open(rootURL: rootURL, create: false)
            libraryPath = library.identity.canonicalPath
            let saved = NativeCLIStatusConfiguration.read(library: library, id: id)
            guard saved.state == .present, let digest = saved.digest else {
                throw NativeInstallControlRefusal.configurationChanged
            }
            let statusRequest = NativeRuntimeRequest(schema: NativeRuntimeCodec.requestSchema, operation: "status",
                requestID: UUID().uuidString, library: library.identity, vmID: id, savedConfiguration: saved)
            let status = try NativeRuntimeClient.query(rootURL: rootURL, request: statusRequest)
            appInstanceID = status.appInstanceID
            let request = NativeInstallControlRequest(schema: NativeInstallControlCodec.requestSchema,
                operation: operation, requestID: UUID().uuidString, library: library.identity, vmID: id,
                appInstanceID: status.appInstanceID, expectedSavedConfigurationDigest: digest,
                operationID: operationID)
            let response = try NativeInstallControlClient.query(rootURL: rootURL, request: request)
            try library.validateCurrentIdentity()
            let complete = response.refusal == nil && (operation != .install
                || ![.failed, .cancelled].contains(response.observation?.phase))
            return .init(command: operation.rawValue, vmID: id, libraryPath: libraryPath,
                appInstanceID: appInstanceID, requestedOperationID: operationID,
                disposition: response.disposition, observation: response.observation,
                unavailableReason: response.refusal?.rawValue, complete: complete)
        } catch {
            return .init(command: operation.rawValue, vmID: id, libraryPath: libraryPath,
                appInstanceID: appInstanceID, requestedOperationID: operationID, disposition: nil,
                observation: nil, unavailableReason: (error as? NativeInstallControlRefusal)?.rawValue
                    ?? (error as? NativeRuntimeError)?.rawValue ?? "transportFailure", complete: false)
        }
    }
}
