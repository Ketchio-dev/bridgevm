import AppKit

@MainActor
enum LibraryDashboardPrimaryAction {
    static func perform(config: VMConfig, model: ControlModel, library: LibraryModel) {
        let saved = library.latestConfiguration(for: config)
        let launch = HvfEngineConfig.libraryVM(saved, rootURL: library.rootURL)
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
