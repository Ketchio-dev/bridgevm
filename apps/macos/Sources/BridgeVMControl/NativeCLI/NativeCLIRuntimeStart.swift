import Foundation

struct NativeCLIRuntimeStart: Encodable {
    let schema = "bridgevm.app-start.v1"
    let scope = NativeRuntimeStartCodec.scope
    let vmID: String
    let libraryPath: String
    let appInstanceID: String?
    let operationID: String
    let expectedSavedConfigurationDigest: String?
    let started: Bool
    let observation: NativeRuntimeStartObservation?
    let unavailableReason: String?

    static func run(rootURL: URL, id: String) -> Self {
        let deadline = NativeRuntimeTransport.now + 30
        let operationID = UUID().uuidString
        var appInstanceID: String?, digest: String?
        do {
            let library = try NativeRuntimeLibraryHandle.open(rootURL: rootURL, create: false)
            let saved = NativeCLIStatusConfiguration.read(library: library, id: id)
            guard saved.state == .present, let expected = saved.digest else {
                throw NativeRuntimeStartRefusal.configurationChanged
            }
            digest = expected
            let statusRequest = NativeRuntimeRequest(schema: NativeRuntimeCodec.requestSchema, operation: "status",
                requestID: UUID().uuidString, library: library.identity, vmID: id, savedConfiguration: saved)
            let status = try NativeRuntimeClient.query(rootURL: rootURL, request: statusRequest)
            appInstanceID = status.appInstanceID
            let request = NativeRuntimeStartRequest(schema: NativeRuntimeStartCodec.requestSchema,
                operation: .start, requestID: UUID().uuidString, library: library.identity, vmID: id,
                appInstanceID: status.appInstanceID, expectedSavedConfigurationDigest: expected, operationID: operationID)
            let result = NativeCLIStartWaiter.wait(request: request, deadline: deadline) {
                try NativeRuntimeStartClient.query(rootURL: rootURL, request: $0)
            }
            try library.validateCurrentIdentity()
            return Self(vmID: id, libraryPath: library.identity.canonicalPath, appInstanceID: appInstanceID,
                operationID: operationID, expectedSavedConfigurationDigest: expected, started: result.started,
                observation: result.response?.observation, unavailableReason: result.failure)
        } catch {
            return Self(vmID: id, libraryPath: rootURL.path, appInstanceID: appInstanceID,
                operationID: operationID, expectedSavedConfigurationDigest: digest, started: false, observation: nil,
                unavailableReason: (error as? NativeRuntimeStartRefusal)?.rawValue
                    ?? (error as? NativeRuntimeError)?.rawValue ?? "transportFailure")
        }
    }
}
