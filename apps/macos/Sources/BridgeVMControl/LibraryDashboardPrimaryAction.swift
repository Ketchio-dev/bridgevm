import AppKit
import Foundation

@MainActor
enum LibraryDashboardPrimaryAction {
    static func perform(config: VMConfig, model: ControlModel, library: LibraryModel) {
        let saved = library.latestConfiguration(for: config)
        let launch = HvfEngineConfig.libraryVM(saved, rootURL: library.rootURL,
            e2eHostDiagnosticStopAdmitted: library.e2eHostDiagnosticStopAdmitted)
        perform(observedRunning: model.running,
            session: library.hvfRuntimeDetailSession(for: saved),
            requestStart: { session in
                guard let launch else { return .refused("저장된 VM 실행 구성을 읽지 못했습니다.") }
                return session.requestGUIStart(configuration: launch)
            },
            attach: { $0.attachIfStopped() },
            present: { HvfDisplayWindowController.present(session: $0, title: saved.name) },
            report: { model.statusNote = $0 })
    }

    static func stop(config: VMConfig, model: ControlModel, library: LibraryModel) {
        stop(session: library.hvfRuntimeDetailSession(for: config),
            requestStop: { $0.stopOwned(expectedToken: $1.token) },
            report: { model.statusNote = $0 }, refresh: model.refreshStatus)
    }
}

extension LibraryDashboardPrimaryAction {
    static func stop(
        session: HvfEngineSession?,
        requestStop: (HvfEngineSession, HvfOwnedRuntimeIdentity) -> HvfRuntimeStopOutcome,
        report: (String) -> Void,
        refresh: () -> Void
    ) {
        HvfRuntimeStopAction.perform(
            session: session, requestStop: requestStop, report: report, refresh: refresh)
    }
}

extension LibraryDashboardPrimaryAction {
    static func perform(observedRunning: Bool, session: HvfEngineSession?,
                        requestStart: (HvfEngineSession) -> HvfGUIStartAdmission,
                        attach: (HvfEngineSession) -> Bool,
                        present: (HvfEngineSession) -> Void,
                        report: (String) -> Void) {
        guard let session else { report("VM 런타임 세션을 준비하지 못했습니다."); return }
        if session.hasActiveRuntimeWork && !session.runnerDiedWithoutCleanup { present(session); return }
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
