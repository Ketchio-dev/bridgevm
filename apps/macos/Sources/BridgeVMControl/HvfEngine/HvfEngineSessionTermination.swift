import Foundation

extension HvfEngineSession: ControlAppTerminationSession {
    /// The app's exit closes an owned runner's input, which cancels it; a pending start may still spawn one.
    var blocksAppTermination: Bool { mayHaveOwnedWork || hasPendingRuntimeStart }

    func requestTerminationStop() {
        guard let controller = ownedController, controller.operation == nil,
              let identity = ownedProcessIdentity, identity == controller.identity else { return }
        stopOwned(expectedToken: identity.token)
    }

    func requestTerminationForceStop() {
        guard let target = forceStopTarget else { return }
        _ = requestForceStop(target)
    }

    func pollTerminationWait() { poll() }
}
