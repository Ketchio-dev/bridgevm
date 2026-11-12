#if canImport(IOKit)
import Foundation
import Combine
import IOKit.pwr_mgt

@MainActor
protocol ControlPowerAsserting {
    func acquire() -> Bool
    func release()
}

@MainActor
final class ControlSystemPowerAssertion: ControlPowerAsserting {
    // pmset renders a non-ASCII assertion name as empty; this is a diagnostic, not UI text.
    static let diagnosticName = "BridgeVM: Windows is running"
    private var assertion: IOPMAssertionID?
    func acquire() -> Bool {
        var id: IOPMAssertionID = 0
        let result = IOPMAssertionCreateWithName(kIOPMAssertionTypePreventUserIdleSystemSleep as CFString,
            IOPMAssertionLevel(kIOPMAssertionLevelOn), Self.diagnosticName as CFString, &id)
        if result == kIOReturnSuccess { assertion = id; return true }
        return false
    }
    func release() {
        if let assertion { IOPMAssertionRelease(assertion) }
        assertion = nil
    }
}

/// One assertion for all running Windows tasks, never for a paused or exited runtime.
@MainActor
final class ControlAppPower: ObservableObject {
    static let preferenceKey = "preventIdleSleepWhileWindowsRuns"
    static let shared: ControlAppPower = {
        UserDefaults.standard.register(defaults: [preferenceKey: true])
        return ControlAppPower(api: ControlSystemPowerAssertion(),
            enabled: UserDefaults.standard.bool(forKey: preferenceKey))
    }()
    private let api: any ControlPowerAsserting
    private var runtimes = Set<UUID>()
    private(set) var held = false
    @Published var enabled: Bool { didSet { reconcile() } }
    var failure: (() -> Void)?
    private var failureReported = false

    init(api: any ControlPowerAsserting, enabled: Bool = true) {
        self.api = api; self.enabled = enabled
    }
    func update(_ runtime: UUID, active: Bool) {
        if active { runtimes.insert(runtime) } else { runtimes.remove(runtime) }
        reconcile()
    }
    func setEnabled(_ value: Bool, defaults: UserDefaults = .standard) {
        defaults.set(value, forKey: Self.preferenceKey)
        enabled = value
    }
    private func reconcile() {
        let needed = enabled && !runtimes.isEmpty
        if needed && !held {
            held = api.acquire()
            if !held && !failureReported { failureReported = true; failure?() }
        } else if !needed && held { api.release(); held = false }
    }
    func shutdown() {
        runtimes.removeAll()
        if held { api.release(); held = false }
    }
}
#endif
