import Foundation

/// A VM saved with a removed engine (Fast VZ or QEMU). It stays in the library so
/// the user can see it and delete it, but nothing here starts, stops or writes
/// anything: its files are never touched automatically.
final class UnsupportedEngineBackend: VMBackend {
    let config: VMConfig
    init(_ config: VMConfig) { self.config = config }

    var displayName: String { config.displayName }
    var kind: String { config.backendKind }
    let supportsGuestCommands = false
    let supportsPackageInstall = false
    let supportsClipboard = false
    let supportsSSH = false
    let supportsResourceChanges = false

    func isRunning() -> Bool { false }
    func currentIP() -> String? { nil }
    @discardableResult func start() -> Bool { false }
    func stop() {}
    func resources() -> (memMiB: Int, cpu: Int) { (config.memMiB ?? 0, config.cpuCount ?? 0) }
    func setResources(memMiB: Int, cpu: Int) -> Bool { false }
    func runInGuest(_ command: String) -> (output: String, code: Int32) { (BackendKind.unsupportedNotice, -1) }
}
