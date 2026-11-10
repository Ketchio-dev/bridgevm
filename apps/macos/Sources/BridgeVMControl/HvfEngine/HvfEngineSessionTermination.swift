import Foundation

extension HvfEngineSession: ControlAppTerminationSession {
    /// The app's exit closes an owned runner's input, which cancels it; a pending start may still spawn one.
    var blocksAppTermination: Bool { mayHaveOwnedWork || hasPendingRuntimeStart }

    /// This app's own run, with no stop under way yet: the Stop below, and the 가상 머신 menu's, can begin.
    var terminationStopAvailable: Bool {
        guard let controller = ownedController, controller.operation == nil,
              let identity = ownedProcessIdentity else { return false }
        return identity == controller.identity
    }

    func requestTerminationStop() {
        guard terminationStopAvailable, let identity = ownedProcessIdentity else { return }
        stopOwned(expectedToken: identity.token)
    }

    func requestTerminationForceStop() {
        guard let target = forceStopTarget else { return }
        _ = requestForceStop(target)
    }

    func pollTerminationWait() { poll() }
}
