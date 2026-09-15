import Foundation

enum HvfRuntimeStartPolicy: Equatable { case attachOrStart, requireNew }

/// A correlation token for an actual Process retained by this session, not a credential.
struct HvfOwnedRuntimeIdentity: Equatable {
    let token: UUID
    let processID: Int32
}

enum HvfRuntimeStartFailureStage: Equatable { case readiness, preparation, helper, keyAccess, processLaunch, keyDelivery }

enum HvfRuntimeStartOutcome: Equatable {
    case ownedLaunchAccepted(HvfOwnedRuntimeIdentity)
    case observedAttachment
    case refused(String)
    case failed(HvfRuntimeStartFailureStage, String)
}

enum HvfRuntimeStopOutcome: Equatable {
    case requested(deadline: Date?)
    case alreadyStopping(deadline: Date?)
    case alreadyStopped
    case notOwned
}

/// Observed exit of the retained child; this does not establish a guest shutdown result.
struct HvfOwnedRuntimeExit: Equatable {
    enum Reason: Equatable { case exit, uncaughtSignal, unknown }
    let identity: HvfOwnedRuntimeIdentity
    let reason: Reason
    let status: Int32

    init(identity: HvfOwnedRuntimeIdentity, process: Process) {
        self.identity = identity
        switch process.terminationReason {
        case .exit: reason = .exit
        case .uncaughtSignal: reason = .uncaughtSignal
        @unknown default: reason = .unknown
        }
        status = process.terminationStatus
    }
}
