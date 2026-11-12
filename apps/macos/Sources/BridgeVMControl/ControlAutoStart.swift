import AppKit

@MainActor
enum ControlAutoStart {
    static var skipAtLaunch = false
    static func selection(_ configs: [VMConfig], optionHeld: Bool) -> [VMConfig] {
        optionHeld ? [] : configs.filter { $0.startWithBridgeVM == true && $0.installPending != true }
    }
    static func run(_ configs: [VMConfig], optionHeld: Bool,
                    start: (VMConfig) async throws -> Void, failed: (VMConfig, Error) -> Void) async {
        for config in selection(configs, optionHeld: optionHeld) {
            do { try await start(config) } catch { failed(config, error) }
        }
    }
    static func isStartable(_ config: VMConfig, library: LibraryModel) -> Bool {
        guard config.installPending != true, !library.shouldShowWindowsInstall(for: config),
              !HvfWindowsInstallTransactionResidue.isPresent(config, rootURL: library.rootURL) else { return false }
        guard config.engineKind == .hvfEngine else {
            let model = library.model(for: config)
            return !model.hasActiveRuntimeWork && !model.backend.isRunning()
        }
        guard let session = library.hvfRuntimeDetailSession(for: config),
              !session.hasActiveRuntimeWork, !session.hasRetainedAttachment,
              !session.processIsRunning(session.config.targetDiskPath),
              session.config.readiness(repoRoot: session.repoRoot).launchReady else { return false }
        return true
    }
    static func launch(_ library: LibraryModel, optionHeld: Bool) async {
        await run(library.vms, optionHeld: optionHeld, start: { config in
            guard !ControlDisplayClose.quitting, isStartable(config, library: library) else { return }
            let model = library.model(for: config)
            if config.engineKind == .hvfEngine {
                if let session = library.hvfRuntimeDetailSession(for: config) {
                    // Like the ordinary Start click, but never attach to a surviving runtime.
                    switch session.requestGUIStart(configuration: session.config, policy: .requireNew) {
                    case .accepted:
                        HvfDisplayWindowController.present(session: session, title: config.name)
                    case let .refused(message): model.statusNote = message
                    }
                    while session.hasPendingRuntimeStart {
                        try await Task.sleep(nanoseconds: 50_000_000)
                    }
                }
            } else {
                model.start()
                while model.lifecycleBusy { try await Task.sleep(nanoseconds: 50_000_000) }
            }
        }, failed: { config, _ in library.model(for: config).statusNote = "Windows를 시작하지 못했으니 다시 시작해 보세요." })
    }
}

extension LibraryModel {
    func canSetStartWithBridgeVM(for config: VMConfig) -> Bool {
        let saved = latestConfiguration(for: config)
        if saved.engineKind == .hvfEngine {
            return hvfRuntimeDetailSession(for: saved)?.libraryPreferencesEditable == true
        }
        return !model(for: saved).hasActiveRuntimeWork
    }

    func setStartWithBridgeVM(_ enabled: Bool, for config: VMConfig) {
        var saved = latestConfiguration(for: config)
        guard canSetStartWithBridgeVM(for: saved) else { return }
        if saved.engineKind == .hvfEngine {
            guard let session = hvfRuntimeDetailSession(for: saved) else { return }
            if let message = session.saveLibraryPreference("자동 시작 설정",
                publish: { VMRegistrationWriter.commit($0, to: $1) },
                change: { $0.startWithBridgeVM = enabled }) {
                operationError = message
            }
            return
        }
        saved.startWithBridgeVM = enabled
        guard VMLibrary.save(saved, rootURL: rootURL) else {
            operationError = "자동 시작 설정을 저장하지 못했으니 다시 시도하세요."
            return
        }
        reload()
    }
}
