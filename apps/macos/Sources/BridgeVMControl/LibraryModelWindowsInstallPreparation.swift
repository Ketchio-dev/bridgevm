import Foundation

extension LibraryModel {
    func windowsInstallPreparation(for config: VMConfig) -> HvfWindowsInstallPreparation {
        windowsInstallSessions.preparation.lookup(slug: config.slug, owner: self)
    }

    func currentWindowsInstallPreparationInput(slug: String, repoRoot: URL) -> HvfWindowsInstallPlanInput? {
        guard let config = vms.first(where: { $0.slug == slug }),
              config.engineKind == .hvfEngine, config.installPending == true else { return nil }
        return .load(for: config, libraryRoot: rootURL, repoRoot: repoRoot)
    }

    func cachedWindowsInstallSession(
        for input: HvfWindowsInstallPlanInput
    ) -> HvfWindowsInstallSession? {
        guard let entry = windowsInstallSessions.record(for: input.config.slug),
              entry.sourceConfig == input.config, entry.request == input.request else { return nil }
        return entry.session
    }

    func windowsInstallPreparationResult(
        _ plan: HvfWindowsInstallPlan, input: HvfWindowsInstallPlanInput,
        replacing expected: HvfWindowsInstallSession?
    ) -> HvfWindowsInstallPreparation.State {
        let slug = input.config.slug
        if let entry = windowsInstallSessions.record(for: slug), entry.session.isRunning {
            return .ready(bindWindowsInstallSession(entry.session, slug: slug))
        }
        guard currentWindowsInstallPreparationInput(slug: slug, repoRoot: input.repoRoot) == input,
              plan.repoRoot == input.repoRoot, plan.libraryRoot == input.libraryRoot,
              plan.bundlePath == input.config.bundlePath, plan.slug == slug, plan.request == input.request else {
            return .failed(HvfWindowsInstallPreparation.staleMessage)
        }
        if let session = cachedWindowsInstallSession(for: input) {
            return .ready(bindWindowsInstallSession(session, slug: slug))
        }
        guard windowsInstallSessions.record(for: slug)?.session === expected else {
            return .failed(HvfWindowsInstallPreparation.staleMessage)
        }
        let session = windowsInstallSessions.session(for: input.config, request: input.request,
            makePlan: { plan })
        return .ready(bindWindowsInstallSession(session, slug: slug))
    }
}
