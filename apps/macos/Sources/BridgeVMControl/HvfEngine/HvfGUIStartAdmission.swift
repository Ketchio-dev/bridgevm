import Foundation

extension HvfEngineSession {
    @discardableResult
    func requestGUIStart(configuration: HvfEngineConfig,
                         policy: HvfRuntimeStartPolicy = .attachOrStart) -> HvfGUIStartAdmission {
        guard !settingsSheetOpen else {
            return .refused("Close Settings before starting Windows.")
        }
        // The pending flags matter only for a dead run: otherwise hasActiveRuntimeWork includes them.
        guard (!hasActiveRuntimeWork || runnerDiedWithoutCleanup), !hasPendingRuntimeStart,
              !hasPendingRuntimeMutation, !hasRetainedAttachment, !workAdmissionGate.isChecking else {
            return .refused("Another operation is in progress for this VM. Wait for it to finish, then try again.")
        }
        // A dead run remains a conflict for mutations; this retry reserves first so
        // checkReservedWork excludes only its own session, not another live owner.
        // Every reservation path refuses while the gate is checking, so state cannot change here.
        if !runnerDiedWithoutCleanup,
           let refusal = workAdmissionGate.check(workAdmission, reportRefusal: true) { return .refused(refusal) }
        let ticket = HvfGUIStartOperation(configuration: configuration)
        guiStartOperation = ticket
        guiStartCleanupOnly = false
        // Published configuration can synchronously call observers; the ticket already owns admission.
        config = configuration
        guard admitGUIStartEffect(ticket) else {
            let reason = ticket.invalidationReason ?? "The selected VM changed. Select it again and start Windows."
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
            ticket.invalidate("The saved VM changed before startup. Review its settings and start Windows again.")
            objectWillChange.send()
            return false
        }
        return ticket.effectAdmission.isValid
    }
}
