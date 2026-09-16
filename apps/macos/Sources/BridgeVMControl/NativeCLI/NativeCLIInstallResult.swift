import Foundation

struct NativeCLIInstallResult: Encodable {
    let schema = "bridgevm.app-install-command.v1"
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
        if let unavailableReason { lines.append("Install control unavailable: \(unavailableReason)") }
        lines.append("This reports app-owned host installation state; it does not prove guest boot or display readiness.")
        return lines.joined(separator: "\n") + "\n" }
}
