import Darwin
@testable import BridgeVMControl

/// Complements a live kqueue witness when cancellation wins before registration.
/// Call only with the exact retained runner's validated completion ledger.
enum HvfOwnedRunnerReapedEvidence {
    static func confirmed(_ summary: HvfOwnedRuntimeRoleSummary, role: String,
                          witness: HvfOwnedRunnerExitWitness?) -> Bool {
        guard role == "helper" || role == "swtpm" else { return false }
        if summary.spawnedCount == 0 {
            return summary.reapedCount == 0 && summary.last == nil && witness == nil
        }
        guard summary.spawnedCount == 1, summary.reapedCount == 1,
              let child = summary.last, child.role == role,
              (try? HvfOwnedRuntimeEventValidation.validateChild(child, reaped: true)) != nil else { return false }
        if let witness { return witness.observe() }
        // Signal zero observes existence only. A live/reused PID or EPERM refuses.
        // ESRCH corroborates the supervisor's exact reaped PID without attaching
        // to, or terminating, any process after its identity could have expired.
        return Darwin.kill(child.pid, 0) == -1 && errno == ESRCH
    }
}
