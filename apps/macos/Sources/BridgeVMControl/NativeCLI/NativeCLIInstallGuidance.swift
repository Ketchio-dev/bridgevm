import Foundation

extension NativeInstallObservation {
    func cliNextAction(vmID: String) -> String {
        switch phase {
        case .done:
            return "Next action: check readiness for exact VM ID \(vmID), then start only if ready."
        case .failed, .cancelled:
            return "Next action: explicitly retry installation for exact VM ID \(vmID)."
        case .preparingPlan, .validating, .preparingSource, .installing, .finalizing,
             .recovering, .cancelling:
            return "Next action: inspect installation status for exact VM ID \(vmID)."
        }
    }
}
