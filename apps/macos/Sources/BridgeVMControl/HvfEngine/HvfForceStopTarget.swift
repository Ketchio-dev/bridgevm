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
        case .requested: return "강제 중단을 요청했습니다. VM이 멈추는 것을 확인합니다."
        case .refused(.targetChanged), .refused(.operationChanged):
            return "실행 또는 종료 요청이 변경되어 강제 중단하지 않았습니다."
        case .refused(.notWaiting): return "현재는 정상 종료를 기다리는 상태가 아닙니다."
        }
    }
}
