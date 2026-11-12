import Foundation

extension HvfEngineSession {
    @discardableResult
    func requestGUIStart(configuration: HvfEngineConfig,
                         policy: HvfRuntimeStartPolicy = .attachOrStart) -> HvfGUIStartAdmission {
        guard !settingsSheetOpen else {
            return .refused("설정을 완료한 뒤 Windows를 시작하세요.")
        }
        guard (!hasActiveRuntimeWork || runnerDiedWithoutCleanup), !hasPendingRuntimeStart,
              !hasPendingRuntimeMutation, !hasRetainedAttachment, !workAdmissionGate.isChecking else {
            return .refused("VM 작업이 진행 중입니다. 완료 후 다시 시도하세요.")
        }
        // A dead run remains a conflict for mutations; this retry reserves first so
        // checkReservedWork excludes only its own session, not another live owner.
        if !runnerDiedWithoutCleanup,
           let refusal = workAdmissionGate.check(workAdmission, reportRefusal: true) { return .refused(refusal) }
        guard (!hasActiveRuntimeWork || runnerDiedWithoutCleanup), !hasPendingRuntimeStart,
              !hasPendingRuntimeMutation, !hasRetainedAttachment else {
            return .refused("VM 상태가 변경되었습니다. 다시 시도하세요.")
        }
        let ticket = HvfGUIStartOperation(configuration: configuration)
        guiStartOperation = ticket
        guiStartCleanupOnly = false
        // Published configuration can synchronously call observers; the ticket already owns admission.
        config = configuration
        guard admitGUIStartEffect(ticket) else {
            let reason = ticket.invalidationReason ?? "VM 시작 대상이 변경되었습니다. 다시 시도하세요."
            ticket.finish(.refused(reason)); objectWillChange.send()
            return .refused(reason)
        }
        let execution = HvfGUIStartExecution(session: self, ticket: ticket, policy: policy)
        guiStartExecution = execution
        objectWillChange.send()
        updateSleepPrevention()
        execution.begin()
        return .accepted(ticket)
    }

    func admitGUIStartEffect(_ ticket: HvfGUIStartOperation) -> Bool {
        guard guiStartOperation === ticket, ticket.workerPending,
              ticket.invalidationReason == nil, ticket.effectAdmission.isValid else { return false }
        guard config == ticket.configuration, checkReservedWork(ticket.id) else {
            ticket.invalidate("VM 설정이나 등록 정보가 변경되어 시작을 중단했습니다.")
            objectWillChange.send()
            return false
        }
        return ticket.effectAdmission.isValid
    }
}
