import Foundation

extension LibraryModel {
    func shouldShowWindowsInstall(for config: VMConfig) -> Bool {
        if windowsInstallSessions.isActive(slug: config.slug) { return true }
        if hvfRuntimeSessions.isActive(slug: config.slug) { return false }
        return config.engineKind == .hvfEngine && config.installPending == true
    }

    func hvfRuntimeSession(for config: VMConfig) -> HvfEngineSession? {
        let latest = latestConfiguration(for: config)
        guard let session = hvfRuntimeSessions.session(for: latest, libraryRoot: rootURL, e2eHostDiagnosticStopAdmitted: e2eHostDiagnosticStopAdmitted) else { return nil }
        bindRuntimeSession(session, configuration: latest)
        return session
    }

    func hvfRuntimeDetailSession(for config: VMConfig) -> HvfEngineSession? {
        guard hvfRuntimeSessions.isActive(slug: config.slug)
            || (config.engineKind == .hvfEngine && config.installPending != true) else { return nil }
        return hvfRuntimeSession(for: config)
    }

    func experimentalHvfRuntimeSession() -> HvfEngineSession {
        hvfRuntimeSessions.experimentalSession()
    }
}

extension LibraryModel {
    func bindControlModel(_ model: ControlModel) -> ControlModel {
        model.workAdmission = boundWorkAdmission(slug: model.config.slug) { [weak model] owner, current in
            model.map { owner.ownsControlModel($0, for: current) } ?? false
        }
        model.onConfigurationPersisted = { [weak self] in self?.reload() }
        return model
    }
}
