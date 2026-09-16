extension NativeCLIRuntimeStart {
    var text: String {
        var lines = ["Native VM start: \(vmID)", "Observation scope: \(scope)",
                     "Runtime startup: \(started ? "confirmed" : "not confirmed")", "Operation: \(operationID)"]
        if let observation { lines.append("Start phase: \(observation.phase.rawValue)") }
        if let target = observation?.target { lines.append("Owned run: \(target.token), PID \(target.processID)") }
        if let unavailableReason { lines.append("Start result unavailable: \(unavailableReason)") }
        lines.append("Startup confirms the supervisor and initial helper; guest boot and display readiness remain unproven.")
        return lines.joined(separator: "\n") + "\n"
    }
}
