import Foundation
struct NativeCLIRuntimeStatus: Encodable {
    let schema = NativeRuntimeCodec.responseSchema
    let scope = NativeRuntimeCodec.scope
    let vmID: String
    let libraryPath: String
    let savedConfiguration: NativeRuntimeSavedConfiguration
    let complete: Bool
    let runtimeState: String
    let unavailableReason: String?
    let appInstanceID: String?
    let observedAt: Double?
    let sessions: [NativeRuntimeSessionObservation]

    static func snapshot(rootURL: URL, id: String) -> Self {
        var saved = NativeRuntimeSavedConfiguration(state: .unreadable, digest: nil)
        do {
            let library = try NativeRuntimeLibraryHandle.open(rootURL: rootURL, create: false)
            saved = NativeCLIStatusConfiguration.read(library: library, id: id)
            let request = NativeRuntimeRequest(schema: NativeRuntimeCodec.requestSchema, operation: "status",
                requestID: UUID().uuidString, library: library.identity, vmID: id, savedConfiguration: saved)
            let result = try NativeRuntimeClient.query(rootURL: rootURL, request: request)
            try library.validateCurrentIdentity()
            try NativeRuntimeCodec.validate(result, for: request)
            return Self(vmID: id, libraryPath: library.identity.canonicalPath, savedConfiguration: saved,
                complete: true, runtimeState: result.sessions.contains { $0.ownership != .notObserved } ? "observed" : "unobserved",
                unavailableReason: nil, appInstanceID: result.appInstanceID, observedAt: result.observedAt, sessions: result.sessions)
        } catch {
            return Self(vmID: id, libraryPath: rootURL.path, savedConfiguration: saved,
                complete: false, runtimeState: "unobserved",
                unavailableReason: (error as? NativeRuntimeError)?.rawValue ?? "transportFailure",
                appInstanceID: nil, observedAt: nil, sessions: [])
        }
    }

    var text: String {
        var lines = ["Native VM runtime: \(vmID)", "Observation scope: \(scope)",
                     "Runtime state: \(runtimeState)", "Saved configuration: \(savedConfiguration.state.rawValue)"]
        if let unavailableReason { lines.append("Observation unavailable: \(unavailableReason)") }
        if complete && sessions.isEmpty { lines.append("The running app has no retained session for this ID.") }
        for session in sessions {
            lines.append("Session: \(session.ownership.rawValue), connection: \(session.connectionState ?? "unobserved")")
            lines.append("  Accepted configuration: \(session.configurationMatch.rawValue)")
            lines.append("  Graphics mode: \(session.graphicsMode?.rawValue ?? "not-reported")")
            if let process = session.ownedProcess { lines.append("  Owned child PID: \(process.processID)") }
            if let exit = session.lastOwnedExit {
                lines.append("  Last owned child exit: PID \(exit.process.processID), \(exit.reason), status \(exit.status)")
            }
        }
        lines.append("App-retained observations only; no guest-health or system-wide VM state is inferred.")
        return lines.joined(separator: "\n") + "\n"
    }
}
