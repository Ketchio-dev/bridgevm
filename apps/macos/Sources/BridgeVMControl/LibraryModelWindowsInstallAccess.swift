import Foundation

extension HvfWindowsInstallSessionStore {
    func isActive(slug: String) -> Bool { record(for: slug)?.session.isRunning == true }

    func session(for config: VMConfig, libraryRoot: URL, repoRoot: URL) -> HvfWindowsInstallSession {
        // The accepted plan stays authoritative until its work ends, even when
        // registration or request metadata changes while another view is open.
        if let entry = record(for: config.slug), entry.session.isRunning { return entry.session }
        let input = HvfWindowsInstallPlanInput.load(for: config, libraryRoot: libraryRoot, repoRoot: repoRoot)
        return session(for: config, request: input.request, makePlan: input.makePlan)
    }
}

extension LibraryModel {
    func windowsInstallSession(for config: VMConfig,
        repoRoot: URL = HvfEngineSession.defaultRepoRoot()) -> HvfWindowsInstallSession {
        let latest = latestConfiguration(for: config)
        let session = windowsInstallSessions.session(for: latest, libraryRoot: rootURL,
            repoRoot: repoRoot)
        return bindWindowsInstallSession(session, slug: latest.slug)
    }

    func bindWindowsInstallSession(_ session: HvfWindowsInstallSession, slug: String) -> HvfWindowsInstallSession {
        session.workAdmission = boundWorkAdmission(slug: slug) { [weak session] owner, current in
            guard let session else { return false }
            return current.engineKind == .hvfEngine && current.installPending == true
                && owner.windowsInstallSessions.owns(session, for: current)
        }
        session.onCompleted = { [weak self] in self?.reload() }
        return session
    }
}
