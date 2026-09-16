import Foundation

enum HvfRuntimeLaunchReadiness {
    static func failure(config: HvfEngineConfig, repoRoot: URL,
                        diagnostic: (String) -> Void) -> HvfRuntimeStartOutcome? {
        let readiness = config.readiness(repoRoot: repoRoot)
        guard !readiness.launchReady else { return nil }
        for blocker in readiness.launchBlockers {
            diagnostic("launch readiness blocked [\(blocker.code)]: \(blocker.summary)")
        }
        return .failed(.readiness, readiness.launchBlockers.map(\.code).joined(separator: ","))
    }
}
