import Foundation

extension HvfEngineSession {
    /// The CLI's stop is bounded (`.cancelRunner`); the app's Stop and quit wait for Windows (`.offerForceStop`).
    func requestOwnedStop(target: HvfOwnedRuntimeIdentity, operationID: UUID,
                          afterGrace: HvfOwnedStopGraceEnd = .cancelRunner) -> HvfOwnedStopAdmission {
        guard let ownedController else { return .refused(.supervisionUnavailable) }
        resumeBeforeStop() // a paused Windows cannot shut down
        let result = ownedController.requestStop(target: target, operationID: operationID, afterGrace: afterGrace,
            pressPowerButton: { [weak self] in self?.pressGuestPowerButton() ?? false })
        if case .refused = result { return result }
        cancelOrderedInputTarget()
        ownedRuntimeChanged(token: ownedController.identity.token)
        return result
    }

    func ownedStopObservation(target: HvfOwnedRuntimeIdentity, operationID: UUID) -> HvfOwnedStopObservation? {
        ownedController?.observation(target: target, operationID: operationID)
    }

    func requestForceStop(_ target: HvfForceStopTarget) -> HvfForceStopResult {
        defer { refreshForceStopTarget() }
        guard ownedProcessIdentity == target.identity, let controller = ownedController,
              controller.identity == target.identity else { return .refused(.targetChanged) }
        guard connectionState == .stopping, !runtimeStartupWorkerPending else { return .refused(.notWaiting) }
        resumeBeforeStop() // the helper then saves the disk and UEFI variables as it stops
        let result = controller.requestForceStop(target)
        ownedRuntimeChanged(token: target.identity.token)
        return result
    }

    func refreshForceStopTarget() {
        let target: HvfForceStopTarget?
        if connectionState == .stopping, !runtimeStartupWorkerPending,
           let controller = ownedController, ownedProcessIdentity == controller.identity {
            target = controller.forceStopTarget
        } else { target = nil }
        if forceStopTarget != target { forceStopTarget = target }
        let wait = target == nil ? nil : ownedController?.guestStopWait
        if stopWait != wait { stopWait = wait }
    }
}
