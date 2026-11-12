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
        bindInstallAdmissions(session, slug: slug)
        return session
    }

    /// Resolve CLI ownership at check time: detail-view adoption may rebind this
    /// session, and operation.start installs its session before checking admission.
    func bindInstallAdmissions(_ session: HvfWindowsInstallSession, slug: String) {
        let admission: @MainActor (Bool, Bool) -> String? = { [weak self, weak session] continuing, report in
            guard let self, let session else {
                return "VM 라이브러리를 사용할 수 없습니다. 라이브러리 화면에서 다시 시도하세요."
            }
            let candidate = self.windowsInstallSessions.cliOperations.records[slug]?.operation
            let operation = candidate?.session === session ? candidate : nil
            let refusal = self.libraryWorkRefusal(slug: slug, excludingInstall: operation,
                excludingWindowsInstall: continuing ? session : nil,
                owns: { owner, current in
                    current.engineKind == .hvfEngine && current.installPending == true
                        && owner.windowsInstallSessions.owns(session, for: current)
                })
            if report, let refusal { self.operationError = refusal }
            return refusal
        }
        session.workAdmission = { admission(false, $0) }
        session.continuationAdmission = { admission(true, false) }
        session.onCompleted = { [weak self] in self?.reload() }
    }
}
