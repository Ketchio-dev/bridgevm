import Foundation
import XCTest
@testable import BridgeVMControl

/// Records entry into Start without creating or discovering any process.
final class RetainedTerminalRecoveryBackend: VMBackend {
    let displayName = "Owned retained recovery", kind = "owned-retained-recovery"
    let supportsGuestCommands = false, supportsPackageInstall = false
    let supportsClipboard = false, supportsSSH = false, supportsResourceChanges = false
    private let lock = NSLock()
    private var starts = 0, effects = 0
    var startCalls: Int { lock.withLock { starts } }
    var forbiddenCalls: Int { lock.withLock { effects } }
    func start() -> Bool { lock.withLock { starts += 1 }; return false }
    func isRunning() -> Bool { false }
    func currentIP() -> String? { nil }
    func resources() -> (memMiB: Int, cpu: Int) { (4096, 2) }
    func stop() { forbidden() }
    func setResources(memMiB: Int, cpu: Int) -> Bool { forbidden(); return false }
    func runInGuest(_ command: String) -> (output: String, code: Int32) {
        forbidden(); return ("", -1)
    }
    private func forbidden() {
        lock.withLock { effects += 1 }
        XCTFail("Retained recovery fixture must not stop, mutate, or contact a VM")
    }
}

@MainActor
final class RetainedTerminalRecoveryFactory {
    private(set) var models: [ControlModel] = []
    private(set) var backends: [RetainedTerminalRecoveryBackend] = []
    func make(_ config: VMConfig) -> ControlModel {
        let backend = RetainedTerminalRecoveryBackend()
        let model = ControlModel(config: config, backend: backend, startsAutomatically: false)
        backends.append(backend); models.append(model)
        return model
    }
}
