import Foundation

enum HvfRuntimeLaunchReadiness {
    /// The user reads each blocker's summary; its code stays in the diagnostics (the run's event log).
    static func failure(config: HvfEngineConfig, repoRoot: URL,
                        diagnostic: (String) -> Void) -> HvfRuntimeStartOutcome? {
        let readiness = config.readiness(repoRoot: repoRoot)
        guard !readiness.launchReady else { return nil }
        for blocker in readiness.launchBlockers {
            diagnostic("launch readiness blocked [\(blocker.code)]: \(blocker.summary)")
        }
        return .failed(.readiness, readiness.launchBlockers.map(\.summary).joined(separator: " "))
    }
}
