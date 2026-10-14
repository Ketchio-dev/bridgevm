#if DEBUG && BRIDGEVM_APP_UI_HOST
import Foundation

/// Only authored counters cross the detached validator/plan boundary.
final class AppUIInstallCancelCounters: @unchecked Sendable {
    static let names = ["plan_builds", "model_creations", "install_creations", "validation_calls",
        "validation_finished", "validation_main_thread_calls", "validation_gate_timeouts",
        "process_resets", "process_cancels", "process_runs", "pipeline_jobs", "source_verifications",
        "media_preparations", "finalizations", "completions", "runtime_creations", "backend_effects", "file_jobs"]
    private let lock = NSLock()
    private var values = Dictionary(uniqueKeysWithValues: names.map { ($0, 0) })

    @discardableResult func increment(_ key: String) -> Int {
        lock.lock(); defer { lock.unlock() }
        precondition(Self.names.contains(key))
        values[key, default: 0] += 1
        return values[key]!
    }
    var snapshot: [String: Int] { lock.lock(); defer { lock.unlock() }; return values }
}

final class AppUIInstallCancelGate: @unchecked Sendable {
    let counters: AppUIInstallCancelCounters
    private let lock = NSLock()
    private let gate = DispatchSemaphore(value: 0)
    private var released = false

    init(counters: AppUIInstallCancelCounters) { self.counters = counters }

    func validate(_ plan: HvfWindowsInstallPlan) -> String? {
        let call = counters.increment("validation_calls")
        let main = Thread.isMainThread
        if main { counters.increment("validation_main_thread_calls") }
        lock.lock(); let shouldWait = !released; lock.unlock()
        if call == 1 && !main && shouldWait && gate.wait(timeout: .now() + 20) == .timedOut {
            counters.increment("validation_gate_timeouts")
        }
        counters.increment("validation_finished")
        // Even a missing AX cancellation can never admit an installer pipeline.
        return AppUIInstallCancelConstants.refusal
    }

    func release() {
        lock.lock(); defer { lock.unlock() }
        guard !released else { return }
        released = true
        gate.signal()
    }
}
#endif
