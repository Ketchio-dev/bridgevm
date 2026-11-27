import Foundation
@testable import BridgeVMControl

/// Counts every backend call so admission fixtures can show none happened.
final class HvfRuntimeWorkAdmissionBackend: VMBackend {
    struct Calls: Equatable { var liveness = 0, resources = 0 }
    private let lock = NSLock()
    private var recorded = Calls()

    var calls: Calls { lock.lock(); defer { lock.unlock() }; return recorded }
    private func record(_ key: WritableKeyPath<Calls, Int>) {
        lock.lock(); recorded[keyPath: key] += 1; lock.unlock()
    }
    func isRunning() -> Bool { record(\.liveness); return false }
    func resources() -> (memMiB: Int, cpu: Int) { record(\.resources); return (4096, 2) }
}
