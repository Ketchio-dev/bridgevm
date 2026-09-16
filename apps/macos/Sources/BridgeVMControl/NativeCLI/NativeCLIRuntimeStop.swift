import Foundation

struct NativeCLIRuntimeStop: Encodable {
    let schema = "bridgevm.app-stop.v1"
    let scope = NativeRuntimeControlCodec.scope
    let vmID: String
    let libraryPath: String
    let target: NativeRuntimeStopTarget?
    let operationID: String?
    let complete: Bool
    let observation: NativeRuntimeStopObservation?
    let unavailableReason: String?

    static func run(rootURL: URL, id: String) -> Self {
        let deadline = NativeRuntimeTransport.now + 210
        var target: NativeRuntimeStopTarget?, operationID: String?
        do {
            let library = try NativeRuntimeLibraryHandle.open(rootURL: rootURL, create: false)
            let saved = NativeCLIStatusConfiguration.read(library: library, id: id)
            let request = NativeRuntimeRequest(schema: NativeRuntimeCodec.requestSchema, operation: "status",
                requestID: UUID().uuidString, library: library.identity, vmID: id, savedConfiguration: saved)
            let status = try NativeRuntimeClient.query(rootURL: rootURL, request: request)
            let accepted = try selectTarget(status)
            let requestedOperation = UUID().uuidString
            target = accepted; operationID = requestedOperation
            let stop = NativeRuntimeControlRequest(schema: NativeRuntimeControlCodec.requestSchema,
                operation: .stop, requestID: UUID().uuidString, library: library.identity, vmID: id,
                operationID: requestedOperation, target: accepted)
            let result = NativeCLIStopWaiter.wait(request: stop, deadline: deadline) {
                try NativeRuntimeControlClient.query(rootURL: rootURL, request: $0)
            }
            try library.validateCurrentIdentity()
            return Self(vmID: id, libraryPath: library.identity.canonicalPath, target: accepted,
                operationID: result.operationID, complete: result.complete,
                observation: result.response?.observation, unavailableReason: result.failure)
        } catch {
            return Self(vmID: id, libraryPath: rootURL.path, target: target, operationID: operationID,
                complete: false, observation: nil,
                unavailableReason: (error as? NativeRuntimeStopRefusal)?.rawValue
                    ?? (error as? NativeRuntimeError)?.rawValue ?? "transportFailure")
        }
    }

    static func selectTarget(_ status: NativeRuntimeResponse) throws -> NativeRuntimeStopTarget {
        let active = status.sessions.filter { $0.ownership == .owned }
        if active.isEmpty, status.sessions.contains(where: { $0.ownership == .attachedObservation }) {
            throw NativeRuntimeStopRefusal.unsupportedRuntime
        }
        let candidates = active.isEmpty ? status.sessions.filter { $0.ownership == .ownedExitObserved } : active
        guard !candidates.isEmpty else { throw NativeRuntimeStopRefusal.targetUnavailable }
        guard candidates.count == 1, let session = candidates.first else { throw NativeRuntimeStopRefusal.ambiguousTarget }
        guard let process = session.ownedProcess ?? session.lastOwnedExit?.process,
              let digest = session.acceptedConfigurationDigest else { throw NativeRuntimeStopRefusal.configurationChanged }
        return .init(appInstanceID: status.appInstanceID, runToken: process.token,
                     processID: process.processID, acceptedConfigurationDigest: digest)
    }
}
