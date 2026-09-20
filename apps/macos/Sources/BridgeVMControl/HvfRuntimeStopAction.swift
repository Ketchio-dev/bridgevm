import Foundation

@MainActor
enum HvfRuntimeStopAction {
    static func perform(
        session: HvfEngineSession?,
        requestStop: (HvfEngineSession, HvfOwnedRuntimeIdentity) -> HvfRuntimeStopOutcome,
        report: (String) -> Void,
        refresh: () -> Void
    ) {
        guard let session else { report("VM 런타임 세션을 찾지 못했습니다."); return }
        guard let identity = session.ownedProcessIdentity else {
            if session.hasRetainedAttachment {
                report("다른 실행 경로에서 시작된 VM은 이 앱이 안전하게 정지할 수 없습니다.")
            } else if session.hasPendingRuntimeStart {
                report("VM 시작 준비가 끝난 뒤 다시 정지하세요.")
            } else {
                report("이 앱이 소유한 실행 프로세스를 찾지 못했습니다.")
            }
            return
        }
        switch requestStop(session, identity) {
        case .requested: report("VM 정지 요청을 보냈습니다.")
        case .alreadyStopping: report("VM 정지가 이미 진행 중입니다.")
        case .alreadyStopped: report("VM이 이미 정지되었습니다.")
        case .notOwned: report("실행 프로세스 소유권이 변경되어 정지를 중단했습니다.")
        }
        refresh()
    }
}
