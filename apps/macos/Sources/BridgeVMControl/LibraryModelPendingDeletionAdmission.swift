import Foundation

extension LibraryModel {
    /// Accepted plans can outlive their registration or a subsequently edited request.
    func pendingDeletionReferenceRefusal(for config: VMConfig) -> String? {
        guard config.engineKind == .hvfEngine, config.installPending == true else { return nil }
        let entry = rootURL.appendingPathComponent(config.slug)
        let controls = windowsInstallSessions.retainedControls(excludingSlugs: [])
            + hvfRuntimeSessions.retainedControls(excludingSlugs: [])
            + retainedControlStore.records.map(\.descriptor)
        for control in controls where control.isActive {
            let request: HvfWindowsInstallRequest?
            switch control {
            case let .install(_, session): request = session.plan.request
            case .runtime, .generic: request = nil
            }
            if HvfPendingDeletionReferences.references(config: control.config,
                                                        request: request, removedEntry: entry) {
                return "다른 VM의 실행·설치 작업이 이 VM의 파일을 사용하고 있습니다. 해당 작업을 완료하거나 취소한 뒤 다시 시도하세요."
            }
        }
        return nil
    }
}
