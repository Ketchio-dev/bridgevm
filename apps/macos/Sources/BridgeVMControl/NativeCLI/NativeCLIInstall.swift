import Foundation

enum NativeCLIInstall {
    static func run(rootURL: URL, id: String,
                    operation: NativeInstallControlRequest.Operation) -> NativeCLIInstallResult {
        let operationID = operation == .install ? UUID().uuidString : nil
        var appInstanceID: String?, libraryPath = rootURL.path
        do {
            let library = try NativeRuntimeLibraryHandle.open(rootURL: rootURL, create: false)
            libraryPath = library.identity.canonicalPath
            let statusRequest = NativeRuntimeRequest(schema: NativeRuntimeCodec.requestSchema, operation: "status",
                requestID: UUID().uuidString, library: library.identity, vmID: id,
                savedConfiguration: NativeCLIStatusConfiguration.read(library: library, id: id))
            let status = try NativeRuntimeClient.query(rootURL: rootURL, request: statusRequest)
            appInstanceID = status.appInstanceID
            let request = NativeInstallControlRequest(schema: NativeInstallControlCodec.requestSchema,
                operation: operation, requestID: UUID().uuidString, library: library.identity, vmID: id,
                appInstanceID: status.appInstanceID, operationID: operationID)
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

struct NativeCLIInstallResult: Encodable {
    let schema = "bridgevm.app-install-command.v2"
    let scope = NativeInstallControlCodec.scope
    let command: String
    let vmID: String
    let libraryPath: String
    let appInstanceID: String?
    let requestedOperationID: String?
    let disposition: NativeInstallControlResponse.Disposition?
    let observation: NativeInstallObservation?
    let unavailableReason: String?
    let complete: Bool

    var text: String {
        var lines = ["Native Windows install: \(vmID)", "Command: \(command)",
                     "Observation scope: \(scope)", "Control exchange: \(complete ? "complete" : "unavailable")"]
        if let requestedOperationID { lines.append("Requested operation: \(requestedOperationID)") }
        if let disposition { lines.append("Disposition: \(disposition.rawValue)") }
        if let observation {
            lines.append("Install operation: \(observation.operationID)")
            lines.append("Install phase: \(observation.phase.rawValue)")
            lines.append("Cancellation available: \(observation.canCancel ? "yes" : "no")")
            if let failure = observation.failure { lines.append("Install failure: \(failure)") }
            if !observation.logTail.isEmpty { lines.append("Recent log:\n" + observation.logTail.joined(separator: "\n")) }
            lines.append(observation.cliNextAction(vmID: vmID))
        }
        if let unavailableReason {
            lines.append("Install control unavailable: "
                + NativeCLIReason.sentence(unavailableReason, vmID: vmID, action: "installing"))
        }
        lines.append("This reports app-owned host installation state; it does not prove guest boot or display readiness.")
        return lines.joined(separator: "\n") + "\n" }
}

extension NativeInstallObservation {
    func cliNextAction(vmID: String) -> String {
        switch phase {
        case .done:
            return "Next action: check readiness for exact VM ID \(vmID), then start only if ready."
        case .failed where failure == HvfWindowsInstallPlan.missingDriverFiles:
            return "Next action: retrying can’t help: " + NativeCLICreateWindowsGuidance.setup
        case .failed, .cancelled:
            return "Next action: explicitly retry installation for exact VM ID \(vmID)."
        case .preparingPlan, .validating, .preparingSource, .installing, .finalizing,
             .recovering, .cancelling:
            return "Next action: inspect installation status for exact VM ID \(vmID)."
        }
    }
}
