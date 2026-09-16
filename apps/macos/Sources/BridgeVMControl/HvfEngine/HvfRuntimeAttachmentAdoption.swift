import Foundation

extension HvfEngineSession {
    /// Caller has already checked the external runtime and holds a transition or start reservation.
    func adoptObservedAttachment(reportDuplicateLaunch: Bool) {
        timer?.invalidate(); timer = nil
        process = nil; ownedController = nil
        closeLiveInput()
        attachedToExistingProcess = true
        nextAttachedLivenessCheck = HvfAttachedLivenessSchedule.next(after: Date())
        resetObservedRuntimeState(clearEvents: true)
        connectionState = .booting
        startPolling()
        if reportDuplicateLaunch {
            append(.unknown("attached to the already running HVF engine; duplicate launch prevented"))
        }
    }
}
