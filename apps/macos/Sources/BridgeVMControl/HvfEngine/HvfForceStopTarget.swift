import Foundation

struct HvfForceStopTarget: Equatable, Sendable {
    let identity: HvfOwnedRuntimeIdentity
    let operationID: UUID
}

enum HvfForceStopRefusal: Equatable, Sendable {
    case targetChanged, operationChanged, notWaiting
}

enum HvfForceStopResult: Equatable, Sendable {
    case requested
    case refused(HvfForceStopRefusal)

    var message: String {
        switch self {
        case .requested: return "Force Stop requested. Waiting for the VM to turn off."
        case .refused(.targetChanged), .refused(.operationChanged):
            return "The running VM or shutdown request changed. Check the VM’s current status before trying again."
        case .refused(.notWaiting): return "The VM is no longer waiting for Windows to shut down. Check its current status."
        }
    }
}
