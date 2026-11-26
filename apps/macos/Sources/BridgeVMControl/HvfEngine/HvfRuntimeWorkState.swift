import Foundation

extension HvfEngineSession {
    var hasPendingRuntimeMutation: Bool { mutationReservation != nil }
    var hasPendingGUIStart: Bool { guiStartOperation?.workerPending == true }
    var hasPendingRuntimeStart: Bool { hasPendingGUIStart }
    var runtimeStartupWorkerPending: Bool { hasPendingGUIStart }
    var mayHaveOwnedWork: Bool { ownedController?.mayHaveOwnedWork == true }
    var hasRetainedAttachment: Bool { attachedToExistingProcess }
    var runnerDiedWithoutCleanup: Bool {
        guard let controller = ownedController else { return false }
        return controller.runnerExit != nil && !controller.process.isRunning && !controller.teardownConfirmed
    }
    var hasActiveRuntimeWork: Bool {
        runtimeTransitionInProgress || hasPendingRuntimeStart || hasPendingRuntimeMutation
            || mayHaveOwnedWork || process?.isRunning == true || connectionState != .stopped
    }
    func runtimeConfigurationChanged() {
        if let reservation = mutationReservation, config != reservation.configuration { reservation.invalidate() }
        if let ticket = guiStartOperation, config != ticket.configuration {
            ticket.invalidate()
            if ticket.workerPending, process != nil { guiStartCleanupOnly = true }
        }
    }
    func matchesRuntimeReservation(_ id: UUID) -> Bool {
        if let mutationReservation, mutationReservation.id == id { return mutationReservation.isActive }
        if let ticket = guiStartOperation, ticket.id == id {
            return ticket.workerPending && ticket.effectAdmission.isValid
        }
        return false
    }
}
