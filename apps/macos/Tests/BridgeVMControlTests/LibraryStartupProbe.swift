import Foundation
@testable import BridgeVMControl

/// A finite synthetic gate, not a latency criterion. Every shared observation is locked.
final class LibraryStartupProbe: @unchecked Sendable {
    private let lock = NSLock()
    private let entered = DispatchSemaphore(value: 0), released = DispatchSemaphore(value: 0)
    private var threads: [Bool] = []
    private var timedOut = false
    private let failFirst: Bool
    init(failFirst: Bool = false) { self.failFirst = failFirst }

    func scan(rootURL: URL) throws -> VMLibraryScan {
        lock.lock()
        threads.append(Thread.isMainThread)
        let first = threads.count == 1
        lock.unlock()
        if first {
            entered.signal()
            let timeout = !Thread.isMainThread && released.wait(timeout: .now() + 5) == .timedOut
            lock.lock(); timedOut = timeout; lock.unlock()
            if failFirst { throw NativeRuntimeError.libraryChanged }
        }
        return LibraryInitialScan.perform(rootURL: rootURL, migrateLegacy: false)
    }
    func waitForEntry() async -> Bool {
        await Task.detached { self.waitSynchronously() }.value
    }
    private func waitSynchronously() -> Bool { entered.wait(timeout: .now() + 5) == .success }
    func release() { released.signal() }
    var observations: (threads: [Bool], timedOut: Bool) {
        lock.lock(); defer { lock.unlock() }
        return (threads, timedOut)
    }
}
