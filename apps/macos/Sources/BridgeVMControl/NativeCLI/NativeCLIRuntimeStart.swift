import Foundation

struct NativeCLIRuntimeStart: Encodable {
    let schema = "bridgevm.app-start.v2"
    let scope = NativeRuntimeStartCodec.scope
    let vmID: String
    let libraryPath: String
    let appInstanceID: String?
    let operationID: String
    let started: Bool
    let observation: NativeRuntimeStartObservation?
    let unavailableReason: String?

    /// Sends only the VM ID; the app starts that VM's current saved configuration.
    static func run(rootURL: URL, id: String) -> Self {
        let deadline = NativeRuntimeTransport.now + 30
        let operationID = UUID().uuidString
        var appInstanceID: String?
        do {
            let library = try NativeRuntimeLibraryHandle.open(rootURL: rootURL, create: false)
            let statusRequest = NativeRuntimeRequest(schema: NativeRuntimeCodec.requestSchema, operation: "status",
                requestID: UUID().uuidString, library: library.identity, vmID: id,
                savedConfiguration: NativeCLIStatusConfiguration.read(library: library, id: id))
            let status = try NativeRuntimeClient.query(rootURL: rootURL, request: statusRequest)
            appInstanceID = status.appInstanceID
            let request = NativeRuntimeStartRequest(schema: NativeRuntimeStartCodec.requestSchema,
                operation: .start, requestID: UUID().uuidString, library: library.identity, vmID: id,
                appInstanceID: status.appInstanceID, operationID: operationID)
            let result = NativeCLIStartWaiter.wait(request: request, deadline: deadline) {
                try NativeRuntimeStartClient.query(rootURL: rootURL, request: $0)
            }
            return Self(vmID: id, libraryPath: library.identity.canonicalPath, appInstanceID: appInstanceID,
                operationID: operationID, started: result.started,
                observation: result.response?.observation, unavailableReason: result.failure)
        } catch {
            return Self(vmID: id, libraryPath: rootURL.path, appInstanceID: appInstanceID,
                operationID: operationID, started: false, observation: nil,
                unavailableReason: (error as? NativeRuntimeError)?.rawValue ?? "transportFailure")
        }
    }
}
