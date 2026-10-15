extension LibraryModel {
    func bindRuntimeSession(_ session: HvfEngineSession, configuration: VMConfig) {
        bindRuntimeWorkAdmission(session, configuration: configuration)
        session.onConfigurationPersisted = { [weak self, weak session] previous, saved in
            guard let self, let session else { return }
            self.adoptSavedClipboardSync(session, previous: previous, saved: saved)
        }
    }

    /// Only the clipboard key changed, so the stopped session is rebound in place. Replacing
    /// it would rebuild the runtime view and silently drop its other unsaved edits.
    private func adoptSavedClipboardSync(_ session: HvfEngineSession, previous: VMConfig, saved: VMConfig) {
        var expected = saved
        expected.id = previous.id
        expected.clipboardSyncEnabled = previous.clipboardSyncEnabled
        if expected == previous, let context = session.config.libraryContext,
           hvfRuntimeSessions.rebind(session, from: previous, to: saved) {
            var rebound = session.config
            rebound.libraryContext = HvfLibraryLaunchContext(config: saved, rootURL: context.rootURL,
                e2eHostDiagnosticStopAdmitted: context.e2eHostDiagnosticStopAdmitted)
            rebound.clipboardSync = saved.clipboardSyncEnabled ?? true
            session.config = rebound
        }
        reload()
        if hvfRuntimeSession(for: saved) !== session {
            operationError = "클립보드 동기화 설정을 저장했지만 VM 등록 정보도 바뀌어 VM 화면을 저장된 설정으로 다시 불러왔습니다. 저장하지 않은 다른 화면 설정은 적용되지 않았으니 확인하세요."
        }
    }
}
