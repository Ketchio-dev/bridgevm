import AppKit

@MainActor
enum ControlAutoStart {
    static let skipAtLaunch = NSEvent.modifierFlags.contains(.option)
    static func selection(_ configs: [VMConfig], optionHeld: Bool) -> [VMConfig] {
        optionHeld ? [] : configs.filter { $0.startWithBridgeVM == true && $0.installPending != true }
    }
    static func run(_ configs: [VMConfig], optionHeld: Bool,
                    start: (VMConfig) async throws -> Void, failed: (VMConfig, Error) -> Void) async {
        for config in selection(configs, optionHeld: optionHeld) {
            do { try await start(config) } catch { failed(config, error) }
        }
    }
    static func launch(_ library: LibraryModel, optionHeld: Bool) async {
        await run(library.vms, optionHeld: optionHeld, start: { config in
            guard !ControlDisplayClose.quitting else { return }
            let model = library.model(for: config)
            if config.engineKind == .hvfEngine {
                LibraryDashboardPrimaryAction.perform(config: config, model: model, library: library)
                if let session = library.hvfRuntimeDetailSession(for: config) {
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
    func setStartWithBridgeVM(_ enabled: Bool, for config: VMConfig) {
        var saved = latestConfiguration(for: config)
        saved.startWithBridgeVM = enabled
        guard VMLibrary.save(saved, rootURL: rootURL) else {
            operationError = "자동 시작 설정을 저장하지 못했으니 다시 시도하세요."
            return
        }
        reload()
    }
}
