#if DEBUG && BRIDGEVM_APP_UI_HOST
import Foundation

@MainActor
final class AppUIInstallCancelProcess: HvfWindowsInstallProcessRunning {
    let counters: AppUIInstallCancelCounters
    init(_ counters: AppUIInstallCancelCounters) { self.counters = counters }
    func reset() { counters.increment("process_resets") }
    func cancel() { counters.increment("process_cancels") }
    func run(plan: HvfWindowsInstallPlan, arguments: [String], environment: [String: String],
             progressLog: URL?, output: @escaping (String) -> Void) async -> Bool {
        counters.increment("process_runs")
        return false
    }
}

/// No method delegates to a production backend, subprocess or host service.
final class AppUIInstallCancelBackend: VMBackend {
    let counters: AppUIInstallCancelCounters
    init(_ counters: AppUIInstallCancelCounters) { self.counters = counters }
    private func refused() { counters.increment("backend_effects") }
    func isRunning() -> Bool { refused(); return false }
    func resources() -> (memMiB: Int, cpu: Int) { refused(); return (4096, 2) }
}
#endif
