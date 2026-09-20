import AppKit

@MainActor
enum LibraryDashboardPrimaryAction {
    static func perform(config: VMConfig, model: ControlModel, library: LibraryModel) {
        perform(running: model.running, start: model.start,
            resolve: { library.hvfRuntimeDetailSession(for: config) },
            present: { HvfDisplayWindowController.present(session: $0, title: config.name) },
            report: { model.statusNote = $0 })
    }

    static func perform(running: Bool, start: () -> Void,
                        resolve: () -> HvfEngineSession?,
                        present: (HvfEngineSession) -> Void,
                        report: (String) -> Void) {
        guard running else { start(); return }
        guard let session = resolve() else {
            report("실행 중인 VM의 화면 세션을 찾지 못했습니다."); return
        }
        let attached = session.attachIfStopped()
        guard attached || session.hasRetainedAttachment || session.process?.isRunning == true
                || session.hasPendingRuntimeStart || session.mayHaveOwnedWork else {
            report("실행 중인 VM 화면에 연결하지 못했습니다."); return
        }
        present(session)
    }
}
