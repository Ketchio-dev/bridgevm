typealias LibraryWorkAdmission = @MainActor (_ reportRefusal: Bool) -> String?

extension LibraryModel {
    func deletionImpact(for cfg: VMConfig) -> VMLibraryDeletionImpact {
        VMLibrary.deletionImpact(for: cfg, rootURL: rootURL)
    }

}

extension LibraryModel {
    func latestConfiguration(for config: VMConfig) -> VMConfig {
        vms.first(where: { $0.slug == config.slug }) ?? config
    }

    func boundWorkAdmission(
        slug: String,
        owns: @escaping @MainActor (LibraryModel, VMConfig) -> Bool
    ) -> LibraryWorkAdmission {
        { [weak self] reportRefusal in
            guard let self else {
                return "VM 라이브러리를 사용할 수 없습니다. 라이브러리 화면에서 다시 시도하세요."
            }
            let refusal = self.libraryWorkRefusal(slug: slug, owns: owns)
            if reportRefusal, let refusal { self.operationError = refusal }
            return refusal
        }
    }

    private func libraryWorkRefusal(
        slug: String, owns: @MainActor (LibraryModel, VMConfig) -> Bool
    ) -> String? {
        guard let config = vms.first(where: { $0.slug == slug }) else {
            return "VM 등록 정보가 변경되었거나 제거되었습니다. 목록을 새로고침한 뒤 다시 시도하세요."
        }
        guard owns(self, config) else {
            return "VM 제어 대상이 변경되었습니다. 목록을 새로고침하고 VM 화면을 다시 여세요."
        }
        if deletingSlugs.contains(slug) || cloningSlugs.contains(slug) || movingSlugs.contains(slug) {
            return "이 VM의 삭제·복제·이동 작업이 진행 중입니다. 완료 후 다시 시도하세요."
        }
        if windowsInstallSessions.isActive(slug: slug) {
            return "Windows 설치가 진행 중입니다. 설치를 완료하거나 취소한 뒤 다시 시도하세요."
        }
        if hvfRuntimeSessions.isActive(slug: slug) {
            return "VM이 실행 중이거나 시작·중지 처리 중입니다. 제어 화면에서 완전히 중지한 뒤 다시 시도하세요."
        }
        if hasAcceptedControlOperation(for: slug) {
            return "이 VM의 제어 작업이 진행 중입니다. 완료 후 다시 시도하세요."
        }
        return nil
    }
}
