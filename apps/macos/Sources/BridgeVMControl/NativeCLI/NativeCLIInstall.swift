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

    /// Text names the result in plain words; operation IDs and the scope stay in --json.
    var text: String {
        var lines = ["Windows installation: \(vmID)"]
        switch disposition {
        case .accepted?: lines.append("Installation started.")
        case .existing?: lines.append("An installation is already running.")
        case .cancelAccepted?: lines.append("Cancel requested.")
        case .status?, .refused?, nil: break
        }
        if let observation {
            lines.append("Status: \(observation.phase.cliStatus)")
            if let failure = observation.failure { lines.append(failure) }
            if !observation.logTail.isEmpty { lines.append("Recent log:\n" + observation.logTail.joined(separator: "\n")) }
            lines.append(observation.cliNextAction(vmID: vmID))
            if observation.canCancel { lines.append("To cancel, run `bridgevm app install-cancel \(vmID)`.") }
        }
        if let unavailableReason {
            lines.append(NativeCLIReason.sentence(unavailableReason, vmID: vmID, action: "installing"))
        }
        return lines.joined(separator: "\n") + "\n" }
}

extension NativeInstallObservation {
    func cliNextAction(vmID: String) -> String {
        switch phase {
        case .done:
            return "Next: run `bridgevm app readiness \(vmID)`, and start the VM only if it’s ready."
        case .failed where failure == HvfWindowsInstallPlan.missingDriverFiles:
            return "Trying again won’t help. " + NativeCLICreateWindowsGuidance.setup
        case .failed, .cancelled:
            return "To try again, run `bridgevm app install \(vmID)`."
        case .preparingPlan, .validating, .preparingSource, .installing, .finalizing,
             .recovering, .cancelling:
            return "To follow progress, run `bridgevm app install-status \(vmID)`."
        }
    }
}

extension NativeInstallObservation.Phase {
    /// The app's installation step names, with the CLI's “canceled” spelling.
    var cliStatus: String {
        switch self {
        case .preparingPlan: return "Preparing the installation"
        case .validating: return "Checking installation files"
        case .preparingSource: return "Preparing installation media"
        case .installing: return "Installing Windows"
        case .finalizing: return "Saving the VM"
        case .recovering: return "Recovering the installation"
        case .cancelling: return "Canceling"
        case .done: return "Complete"
        case .failed: return "Failed"
        case .cancelled: return "Canceled"
        }
    }
}
