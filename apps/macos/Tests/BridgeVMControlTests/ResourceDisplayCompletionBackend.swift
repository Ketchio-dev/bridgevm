import Foundation
import XCTest
@testable import BridgeVMControl

/// Uses the actual Fast VZ reader/writer; every process-facing method stays inert.
final class ResourceDisplayCompletionBackend: VMBackend {
    let displayName = "Owned resource display", kind = "owned-resource-display"
    let supportsGuestCommands = false, supportsPackageInstall = false
    let supportsClipboard = false, supportsSSH = false, supportsResourceChanges = true
    private let wrapped: VMBackend
    private let saved = DispatchSemaphore(value: 0), finish = DispatchSemaphore(value: 0)
    private let lock = NSLock()
    private var timedOut = false, effects = 0, writes = 0
    private var staleReadProbe: ResourceDisplayStaleReadProbe?

    init(_ wrapped: VMBackend) { self.wrapped = wrapped }
    var gateTimedOut: Bool { lock.withLock { timedOut } }
    var processEffects: Int { lock.withLock { effects } }
    var writeCalls: Int { lock.withLock { writes } }
    func install(_ probe: ResourceDisplayStaleReadProbe) { lock.withLock { staleReadProbe = probe } }
    func release() { finish.signal() }
    func waitForSaved() async -> Bool {
        await Task.detached { [saved] in saved.wait(timeout: .now() + 3) == .success }.value
    }
    func setResources(memMiB: Int, cpu: Int) -> Bool {
        lock.withLock { writes += 1 }
        lock.withLock { staleReadProbe }?.beforeWrite()
        let result = wrapped.setResources(memMiB: memMiB, cpu: cpu)
        saved.signal()
        if finish.wait(timeout: .now() + 5) == .timedOut { lock.withLock { timedOut = true } }
        return result
    }
    func resources() -> (memMiB: Int, cpu: Int) {
        let value = wrapped.resources()
        return lock.withLock { staleReadProbe }?.holdFirstRead(value) ?? value
    }
    func isRunning() -> Bool { false }
    func currentIP() -> String? { nil }
    func start() -> Bool { forbiddenEffect(); return false }
    func stop() { forbiddenEffect() }
    func runInGuest(_ command: String) -> (output: String, code: Int32) {
        forbiddenEffect(); return ("", -1)
    }
    private func forbiddenEffect() {
        lock.withLock { effects += 1 }
        XCTFail("Resource display tests must not launch, stop, or contact a VM")
    }
}
