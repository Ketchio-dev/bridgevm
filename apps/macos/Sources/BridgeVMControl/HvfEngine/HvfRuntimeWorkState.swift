import Foundation

extension HvfEngineSession {
    var hasPendingRuntimeMutation: Bool { mutationReservation != nil }
    var hasPendingOwnedStart: Bool { ownedStartOperation?.reservesSession == true }
    var hasPendingGUIStart: Bool { guiStartOperation?.workerPending == true }
    var hasPendingRuntimeStart: Bool { hasPendingOwnedStart || hasPendingGUIStart }
    var runtimeStartupWorkerPending: Bool {
        hasPendingGUIStart || ownedStartOperation?.workerPending == true
    }
    var mayHaveOwnedWork: Bool { ownedController?.mayHaveOwnedWork == true }
    var hasRetainedAttachment: Bool { attachedToExistingProcess }
    var hasActiveRuntimeWork: Bool {
        runtimeTransitionInProgress || hasPendingRuntimeStart || hasPendingRuntimeMutation
            || mayHaveOwnedWork || process?.isRunning == true || connectionState != .stopped
    }
    func runtimeConfigurationChanged() {
        if let reservation = mutationReservation, config != reservation.configuration { reservation.invalidate() }
        if let ticket = ownedStartOperation, config != ticket.configuration { ticket.fail(.configurationChanged) }
        if let ticket = guiStartOperation, config != ticket.configuration {
            ticket.invalidate("VM 설정이 변경되어 시작을 중단했습니다.")
            if ticket.workerPending, process != nil { guiStartCleanupOnly = true }
        }
    }
    func matchesRuntimeReservation(_ id: UUID) -> Bool {
        if let mutationReservation, mutationReservation.id == id { return mutationReservation.isActive }
        if let ticket = guiStartOperation, ticket.id == id {
            return ticket.workerPending && ticket.effectAdmission.isValid
        }
        return ownedStartOperation?.operationID == id && ownedStartOperation?.reservesSession == true
    }
}
