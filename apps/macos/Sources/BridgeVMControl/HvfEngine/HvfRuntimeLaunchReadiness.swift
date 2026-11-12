import Foundation

enum HvfRuntimeLaunchReadiness {
    /// The user reads each blocker's summary; its code stays in the diagnostics (the run's event log).
    static func failure(config: HvfEngineConfig, repoRoot: URL,
                        diagnostic: (String) -> Void) -> HvfRuntimeStartOutcome? {
        let readiness = config.readiness(repoRoot: repoRoot)
        guard !readiness.launchReady else { return nil }
        return failure(blockers: readiness.launchBlockers, diagnostic: diagnostic)
    }

    static func failure(blockers: [HvfWindowsReadinessIssue],
                        diagnostic: (String) -> Void) -> HvfRuntimeStartOutcome {
        for blocker in blockers {
            diagnostic("launch readiness blocked [\(blocker.code)]: \(blocker.summary)")
        }
        return .failed(.readiness(blockers.map(\.code)), blockers.map(\.summary).joined(separator: " "))
    }
}
