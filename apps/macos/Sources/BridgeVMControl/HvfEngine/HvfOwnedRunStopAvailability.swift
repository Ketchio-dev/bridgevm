import Foundation

extension HvfOwnedRunController {
    static let guestGrace: TimeInterval = 180
    static let completionObservation: TimeInterval = 12
    /// How long a Force Stop lets Windows run `shutdown.exe /p /f` (about 7 s measured) before the runner is cancelled.
    static let forcedGuestWindow: TimeInterval = 30
    var mayHaveOwnedWork: Bool { !teardownConfirmed }

    func observation(target: HvfOwnedRuntimeIdentity, operationID: UUID) -> HvfOwnedStopObservation? {
        guard target == identity, operation?.operationID == operationID else { return nil }
        return operation?.observation
    }

    var forceStopTarget: HvfForceStopTarget? {
        guard forceStopAvailable, let operation else { return nil }
        return HvfForceStopTarget(identity: identity, operationID: operation.operationID)
    }

    /// What the stop controls tell the user while Windows has not turned off yet.
    var guestStopWait: HvfGuestStopWait? {
        guard forceStopAvailable else { return nil }
        if graceExpired { return .windowsStillRunning }
        return guestStopRequested ? .windowsClosingApps : .waitingForGuestAgent
    }
}
