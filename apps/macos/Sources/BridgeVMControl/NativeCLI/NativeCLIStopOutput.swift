extension NativeCLIRuntimeStop {
    var text: String {
        var lines = ["Native VM stop: \(vmID)", "Observation scope: \(scope)",
                     "Owned runtime cleanup: \(complete ? "confirmed" : "not confirmed")"]
        if let operationID { lines.append("Operation: \(operationID)") }
        if let target { lines.append("Owned run: \(target.runToken), PID \(target.processID)") }
        if let observation { lines.append("Stop phase: \(observation.phase.rawValue)") }
        if let unavailableReason { lines.append("Stop result unavailable: \(unavailableReason)") }
        lines.append("Completion requires supervisor cleanup and retained runner exit; normal guest shutdown is unproven.")
        return lines.joined(separator: "\n") + "\n"
    }
}
