import Foundation
import XCTest
@testable import BridgeVMControl

/// Only the real resource reader/writer is forwarded; runtime state is inert.
final class ResourceRegistrationBackend: VMBackend {
    let displayName = "Owned registration", kind = "owned-registration"
    let supportsGuestCommands = false, supportsPackageInstall = false
    let supportsClipboard = false, supportsSSH = false, supportsResourceChanges = true
    private let wrapped: VMBackend
    private let compensate: Bool
    private let saved = DispatchSemaphore(value: 0), finish = DispatchSemaphore(value: 0)
    private let lock = NSLock()
    private var active = true, timedOut = false
    private var starts = 0, stops = 0, writes = 0

    init(_ wrapped: VMBackend, compensate: Bool = false) { self.wrapped = wrapped; self.compensate = compensate }
    var counts: [Int] { lock.withLock { [writes, starts, stops] } }
    var gateTimedOut: Bool { lock.withLock { timedOut } }
    func release() { finish.signal() }
    func waitForSaved() async -> Bool { await Task.detached { self.waitSaved() }.value }
    private func waitSaved() -> Bool { saved.wait(timeout: .now() + 3) == .success }
    func setResources(memMiB: Int, cpu: Int) -> Bool {
        lock.withLock { writes += 1 }
        let result = wrapped.setResources(memMiB: memMiB, cpu: cpu)
        saved.signal()
        if finish.wait(timeout: .now() + 5) == .timedOut { lock.withLock { timedOut = true } }
        if result && compensate {
            // A simulated failed transaction, using the real writer to restore allocation.
            // This is not an injected failure inside FastVZResourceUpdate compensation.
            XCTAssertTrue(wrapped.setResources(memMiB: 4096, cpu: 2))
            return false
        }
        return result
    }
    func resources() -> (memMiB: Int, cpu: Int) { wrapped.resources() }
    func isRunning() -> Bool { lock.withLock { active } }
    func currentIP() -> String? { nil }
    func start() -> Bool { lock.withLock { starts += 1; active = true }; return true }
    func stop() { lock.withLock { stops += 1; active = false } }
    func runInGuest(_ command: String) -> (output: String, code: Int32) {
        XCTFail("Resource registration tests must never contact a guest"); return ("", -1)
    }
}
