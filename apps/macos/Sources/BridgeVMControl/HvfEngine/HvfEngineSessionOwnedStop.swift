import Foundation

extension HvfEngineSession {
    func requestOwnedStop(target: HvfOwnedRuntimeIdentity, operationID: UUID) -> HvfOwnedStopAdmission {
        guard let ownedController else { return .refused(.supervisionUnavailable) }
        let result = ownedController.requestStop(target: target, operationID: operationID)
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
    }
}
