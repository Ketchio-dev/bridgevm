extension LibraryModel {
    func bindRuntimeSession(_ session: HvfEngineSession, configuration: VMConfig) {
        bindRuntimeWorkAdmission(session, configuration: configuration)
        session.onConfigurationPersisted = { [weak self, weak session] previous, saved in
            guard let self, let session else { return }
            self.adoptSavedRuntimePreferences(session, previous: previous, saved: saved)
        }
    }

    /// Only runtime-view preference keys (clipboard, network, shared folder) changed, so the stopped session
    /// is rebound in place. Replacing it would rebuild the runtime view and silently drop its other unsaved edits.
    private func adoptSavedRuntimePreferences(_ session: HvfEngineSession, previous: VMConfig, saved: VMConfig) {
        var expected = saved.keepingRuntimePreferences(of: previous)
        expected.id = previous.id
        if expected == previous, let context = session.config.libraryContext,
           let launch = HvfEngineConfig.libraryVM(saved, rootURL: context.rootURL,
               e2eHostDiagnosticStopAdmitted: context.e2eHostDiagnosticStopAdmitted),
           hvfRuntimeSessions.rebind(session, from: previous, to: saved) {
            session.config = session.config.adoptingSavedPreferences(launch, previous: previous, saved: saved)
        }
        reload()
        if hvfRuntimeSession(for: saved) !== session {
            operationError = "VM 설정을 저장했지만 VM 등록 정보도 바뀌어 VM 화면을 저장된 설정으로 다시 불러왔습니다. 저장하지 않은 다른 화면 설정은 적용되지 않았으니 확인하세요."
        }
    }
}
