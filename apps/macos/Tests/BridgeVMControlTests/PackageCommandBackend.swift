import Foundation
@testable import BridgeVMControl

final class PackageCommandBackend: VMBackend {
    let fixture: PackageCommandFixture
    let displayName = "Synthetic package command"
    let kind = "test"
    let supportsGuestCommands = true, supportsPackageInstall = true
    let supportsClipboard = false, supportsSSH = false, supportsResourceChanges = false
    private let lock = NSLock()
    private var count = 0
    var calls: Int { lock.lock(); defer { lock.unlock() }; return count }

    init(_ fixture: PackageCommandFixture) { self.fixture = fixture }
    func isRunning() -> Bool { true }
    func currentIP() -> String? { nil }
    func start() -> Bool { false }
    func stop() {}
    func resources() -> (memMiB: Int, cpu: Int) { (4096, 2) }
    func setResources(memMiB: Int, cpu: Int) -> Bool { false }
    func runInGuest(_ command: String) -> (output: String, code: Int32) {
        lock.lock(); count += 1; lock.unlock()
        return fixture.run(command)
    }
}
