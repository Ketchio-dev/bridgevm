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
    /// Only Windows VMs on the BridgeVM engine start; a VM saved with a retired engine never can.
    /// One that is not ready (its disk on a drive that is not connected) is left to the start,
    /// which says why in the library as the Start button does.
    static func isStartable(_ config: VMConfig, library: LibraryModel) -> Bool {
        guard config.engineKind == .hvfEngine, config.installPending != true, !library.shouldShowWindowsInstall(for: config),
              !HvfWindowsInstallTransactionResidue.isPresent(config, rootURL: library.rootURL) else { return false }
        guard let session = library.hvfRuntimeDetailSession(for: config),
              !session.hasActiveRuntimeWork, !session.hasRetainedAttachment,
              !session.processIsRunning(session.config.targetDiskPath) else { return false }
        return true
    }
    static func launch(_ library: LibraryModel, optionHeld: Bool) async {
        await run(library.vms, optionHeld: optionHeld, start: { config in
            guard !ControlDisplayClose.quitting, isStartable(config, library: library),
                  let session = library.hvfRuntimeDetailSession(for: config) else { return }
            // Like the ordinary Start click, but never attach to a surviving runtime.
            switch session.requestGUIStart(configuration: session.config, policy: .requireNew) {
            case .accepted:
                HvfDisplayWindowController.present(session: session, title: config.name)
            case let .refused(message): library.model(for: config).statusNote = message
            }
            while session.hasPendingRuntimeStart {
                try await Task.sleep(nanoseconds: 50_000_000)
            }
        }, failed: { config, _ in library.model(for: config).statusNote = "Couldn’t start the VM automatically. Try starting it from your library." })
    }
}

extension LibraryModel {
    /// A VM saved with a retired engine can never start, so its toggle stays disabled.
    func canSetStartWithBridgeVM(for config: VMConfig) -> Bool {
        let saved = latestConfiguration(for: config)
        guard saved.engineKind == .hvfEngine else { return false }
        return hvfRuntimeDetailSession(for: saved)?.libraryPreferencesEditable == true
    }

    func setStartWithBridgeVM(_ enabled: Bool, for config: VMConfig) {
        let saved = latestConfiguration(for: config)
        guard canSetStartWithBridgeVM(for: saved), let session = hvfRuntimeDetailSession(for: saved) else { return }
        if let message = session.saveLibraryPreference("Start when BridgeVM opens",
            publish: { VMRegistrationWriter.commit($0, to: $1) },
            change: { $0.startWithBridgeVM = enabled }) {
            operationError = message
        }
    }
}
