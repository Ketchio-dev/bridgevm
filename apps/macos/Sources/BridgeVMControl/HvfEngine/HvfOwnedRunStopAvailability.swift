import Foundation

extension HvfOwnedRunController {
    static let guestGrace: TimeInterval = 180
    static let completionObservation: TimeInterval = 12
    var mayHaveOwnedWork: Bool { !teardownConfirmed }

    func observation(target: HvfOwnedRuntimeIdentity, operationID: UUID) -> HvfOwnedStopObservation? {
        guard target == identity, operation?.operationID == operationID else { return nil }
        return operation?.observation
    }

    var forceStopTarget: HvfForceStopTarget? {
        guard let operation, operation.observation.phase == .guestGrace,
              failure == nil, ledger.complete == nil, runnerExit == nil, process.isRunning else { return nil }
        return HvfForceStopTarget(identity: identity, operationID: operation.operationID)
    }
}
