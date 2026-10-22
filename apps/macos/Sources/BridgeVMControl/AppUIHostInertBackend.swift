#if DEBUG && BRIDGEVM_APP_UI_HOST
import Foundation

/// Backend of each seeded overview row's control model. Rows read only the
/// model's published state; in production every method below reaches process
/// lookup, launch, Keychain or bundle files, so here each one is a counted,
/// fatal `backend_calls` tripwire. It holds no configuration and no path.
final class AppUIHostInertBackend: VMBackend, Sendable {
    private let refuse: @Sendable () -> Never
    init(refuse: @escaping @Sendable () -> Never) { self.refuse = refuse }
    let displayName = "Owned UI fixture"
    let kind = "hvf-engine"
    let supportsGuestCommands = false, supportsPackageInstall = false, supportsClipboard = false
    let supportsSSH = false, supportsResourceChanges = false
    func isRunning() -> Bool { refuse() }
    func currentIP() -> String? { refuse() }
    func start() -> Bool { refuse() }
    func stop() { refuse() }
    func resources() -> (memMiB: Int, cpu: Int) { refuse() }
    func setResources(memMiB: Int, cpu: Int) -> Bool { refuse() }
    func runInGuest(_ command: String) -> (output: String, code: Int32) { refuse() }
}

extension AppUIHostCapture {
    /// Backend methods are synchronous and a model refresh runs them detached,
    /// so a call may arrive off the main actor; it is still counted and saved by
    /// the owning capture before the host stops, like every other tripwire.
    nonisolated func backendTripwire() -> Never {
        if Thread.isMainThread { MainActor.assumeIsolated { tripwire("backend_calls") } }
        DispatchQueue.main.sync { MainActor.assumeIsolated { tripwire("backend_calls") } }
    }
}
#endif
