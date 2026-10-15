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
    let displayName = "합성 설치 UI"
    let kind = "hvf-engine"
    let supportsGuestCommands = false, supportsPackageInstall = false, supportsClipboard = false
    let supportsSSH = false, supportsResourceChanges = false
    private func refused() { counters.increment("backend_effects") }
    func isRunning() -> Bool { refused(); return false }
    func currentIP() -> String? { refused(); return nil }
    func start() -> Bool { refused(); return false }
    func stop() { refused() }
    func resources() -> (memMiB: Int, cpu: Int) { refused(); return (4096, 2) }
    func setResources(memMiB: Int, cpu: Int) -> Bool { refused(); return false }
    func runInGuest(_ command: String) -> (output: String, code: Int32) { refused(); return ("", 1) }
}
#endif
