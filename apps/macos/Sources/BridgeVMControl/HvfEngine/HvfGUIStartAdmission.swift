import Foundation

extension HvfEngineSession {
    /// With the VM's microphone setting on and macOS not asked yet (a reset, a re-signed build,
    /// another Mac), macOS asks first and the start continues with either answer: the launch
    /// reads the permission again. `microphone: nil` never asks, for a CLI start.
    @discardableResult
    func requestGUIStart(configuration: HvfEngineConfig,
                         policy: HvfRuntimeStartPolicy = .attachOrStart,
                         microphone: (any HvfMicrophonePermission)? = HvfSystemMicrophonePermission()) -> HvfGUIStartAdmission {
        guard openSettingsSheets.isEmpty else {
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
        if configuration.audioEnabled, configuration.microphoneEnabled, let microphone, microphone.status == .undetermined {
            microphone.requestAccess { _ in execution.begin() }
        } else {
            execution.begin()
        }
        return .accepted(ticket)
    }

    func admitGUIStartEffect(_ ticket: HvfGUIStartOperation) -> Bool {
        guard guiStartOperation === ticket, ticket.workerPending,
              ticket.invalidationReason == nil, ticket.effectAdmission.isValid else { return false }
        guard config == ticket.configuration, checkReservedWork(ticket.id) else {
            ticket.invalidate()
            objectWillChange.send()
            return false
        }
        return ticket.effectAdmission.isValid
    }
}
