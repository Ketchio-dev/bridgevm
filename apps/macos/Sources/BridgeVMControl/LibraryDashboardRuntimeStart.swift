import Foundation

extension LibraryDashboardPrimaryAction {
    static func perform(observedRunning: Bool, session: HvfEngineSession?,
                        requestStart: (HvfEngineSession) -> HvfGUIStartAdmission,
                        attach: (HvfEngineSession) -> Bool,
                        present: (HvfEngineSession) -> Void,
                        report: (String) -> Void) {
        guard let session else { report("VM 런타임 세션을 준비하지 못했습니다."); return }
        if session.hasActiveRuntimeWork { present(session); return }
        if !observedRunning {
            switch requestStart(session) {
            case .accepted: report("VM 시작 중…"); present(session)
            case let .refused(reason): report(reason)
            }
            return
        }
        let attached = attach(session)
        guard attached || session.hasRetainedAttachment || session.process?.isRunning == true
                || session.hasPendingRuntimeStart || session.mayHaveOwnedWork else {
            report("실행 중인 VM 화면에 연결하지 못했습니다."); return
        }
        present(session)
    }
}
