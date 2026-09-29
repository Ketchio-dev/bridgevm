import Foundation
@testable import BridgeVMControl

extension HvfWindowsSnapshotInvokeTests {
    static let blocked = "invoke did not return: the helper is blocked on a full pipe"

    /// nil when invoke is still blocked at the deadline. The blocked child is left
    /// behind and dies of a broken pipe when this test process exits.
    func invokeShell(_ script: String, seconds: Int = 15) -> Result<String, Error>? {
        let outcome = Outcome()
        let done = DispatchSemaphore(value: 0)
        Thread.detachNewThread {
            outcome.set(Result { try HvfWindowsSnapshotCommand.invoke(URL(fileURLWithPath: "/bin/sh"), ["-c", script]) })
            done.signal()
        }
        guard done.wait(timeout: .now() + .seconds(seconds)) == .success else { return nil }
        return outcome.get()
    }

    private final class Outcome: @unchecked Sendable {
        private let lock = NSLock()
        private var value: Result<String, Error>?

        func set(_ result: Result<String, Error>) {
            lock.lock(); defer { lock.unlock() }
            value = result
        }

        func get() -> Result<String, Error>? {
            lock.lock(); defer { lock.unlock() }
            return value
        }
    }
}
