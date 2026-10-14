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

    func runtimeWorkConflicts(slug: String, excluding reservation: (HvfEngineSession, UUID)?) -> Bool {
        guard let records = try? runtimeRecords(slug: slug) else { return true }
        return records.contains { record in
            if let (session, token) = reservation, record.session === session,
               session.matchesRuntimeReservation(token) { return false }
            return record.session.hasActiveRuntimeWork
        }
    }

}
