typealias LibraryActionJob = @Sendable () async -> Void
typealias LibraryActionScheduler = @MainActor (@escaping LibraryActionJob) -> Void

enum LibraryFileAction { case deletion, clone, move }

extension LibraryModel {
    func libraryActionRefusal(for cfg: VMConfig) -> String? {
        guard vms.first(where: { $0.slug == cfg.slug }) == cfg else {
            return "VM 등록 정보가 변경되었거나 제거되었습니다. 목록을 새로고침한 뒤 다시 시도하세요."
        }
        if deletingSlugs.contains(cfg.slug) || cloningSlugs.contains(cfg.slug) || movingSlugs.contains(cfg.slug) {
            return "이 VM의 삭제·복제·이동 작업이 진행 중입니다. 완료 후 다시 시도하세요."
        }
        if windowsInstallSessions.isActive(slug: cfg.slug) {
            return "Windows 설치가 진행 중입니다. 설치를 완료하거나 취소한 뒤 다시 시도하세요."
        }
        if hvfRuntimeSessions.isActive(slug: cfg.slug) {
            return "VM이 실행 중이거나 시작·중지 처리 중입니다. 제어 화면에서 완전히 중지한 뒤 다시 시도하세요."
        }
        if hasAcceptedControlOperation(for: cfg.slug) {
            return "이 VM의 제어 작업이 진행 중입니다. 완료 후 다시 시도하세요."
        }
        if hasMismatchedCachedControlConfiguration(for: cfg) {
            return "VM 제어 정보가 최신 등록 정보와 다릅니다. 기존 VM을 완전히 중지하고 목록을 새로고침한 뒤 다시 시도하세요."
        }
        return nil
    }

    func admitLibraryAction(_ cfg: VMConfig, action: LibraryFileAction) -> Bool {
        guard let refusal = libraryActionRefusal(for: cfg) else { return true }
        switch action {
        case .deletion: deletionError = refusal
        case .clone: cloneError = refusal
        case .move: moveError = refusal
        }
        return false
    }
}
