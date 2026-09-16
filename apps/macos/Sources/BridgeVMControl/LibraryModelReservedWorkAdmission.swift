import Foundation

extension LibraryModel {
    func bindRuntimeWorkAdmission(_ session: HvfEngineSession, configuration: VMConfig) {
        let owns: @MainActor (LibraryModel, VMConfig) -> Bool = { [weak session] owner, current in
            guard let session else { return false }
            return current.engineKind == .hvfEngine && current.installPending != true
                && owner.hvfRuntimeSessions.owns(session, for: current)
        }
        session.workAdmission = boundWorkAdmission(slug: configuration.slug, owns: owns)
        session.reservedWorkAdmission = { [weak self, weak session] token, reportRefusal in
            guard let self, let session, session.matchesRuntimeReservation(token) else {
                return "VM 작업 예약이 변경되었습니다. 다시 시도하세요."
            }
            let refusal = self.libraryWorkRefusal(slug: configuration.slug,
                excluding: (session, token), owns: owns)
            if reportRefusal, let refusal { self.operationError = refusal }
            return refusal
        }
    }

    private func runtimeWorkConflicts(slug: String, excluding reservation: (HvfEngineSession, UUID)?) -> Bool {
        guard let records = try? runtimeRecords(slug: slug) else { return true }
        return records.contains { record in
            if let (session, token) = reservation, record.session === session,
               session.matchesRuntimeReservation(token) { return false }
            return record.session.hasActiveRuntimeWork
        }
    }

    func libraryWorkRefusal(
        slug: String, excluding reservation: (HvfEngineSession, UUID)? = nil,
        owns: @MainActor (LibraryModel, VMConfig) -> Bool
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
        if windowsInstallSessions.isActive(slug: slug) || windowsInstallSessions.cliOperations.isActive(vmID: slug) || retainedControlStore.records.contains(where: {
            if case let .install(config, session) = $0.descriptor { return config.slug == slug && session.isRunning }
            return false
        }) {
            return "Windows 설치가 진행 중입니다. 설치를 완료하거나 취소한 뒤 다시 시도하세요."
        }
        if runtimeWorkConflicts(slug: slug, excluding: reservation) {
            return "VM이 실행 중이거나 시작·중지 처리 중입니다. 제어 화면에서 완전히 중지한 뒤 다시 시도하세요."
        }
        if hasAcceptedControlOperation(for: slug) {
            return "이 VM의 제어 작업이 진행 중입니다. 완료 후 다시 시도하세요."
        }
        return nil
    }
}
