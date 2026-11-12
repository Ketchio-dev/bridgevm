#if canImport(IOKit)
import Foundation
import IOKit.pwr_mgt

@MainActor
protocol ControlPowerAsserting {
    func acquire() -> Bool
    func release()
}

@MainActor
final class ControlSystemPowerAssertion: ControlPowerAsserting {
    private var assertion: IOPMAssertionID?
    func acquire() -> Bool {
        var id: IOPMAssertionID = 0
        let result = IOPMAssertionCreateWithName(kIOPMAssertionTypePreventUserIdleSystemSleep as CFString,
            IOPMAssertionLevel(kIOPMAssertionLevelOn), "Windows 실행 중 Mac 잠자기 방지" as CFString, &id)
        if result == kIOReturnSuccess { assertion = id; return true }
        return false
    }
    func release() {
        if let assertion { IOPMAssertionRelease(assertion) }
        assertion = nil
    }
}

/// One assertion for all runtimes, including a paused runtime and a failed start still cleaning up.
@MainActor
final class ControlAppPower {
    static let preferenceKey = "preventIdleSleepWhileWindowsRuns"
    static let shared: ControlAppPower = {
        UserDefaults.standard.register(defaults: [preferenceKey: true])
        return ControlAppPower(api: ControlSystemPowerAssertion(),
            enabled: UserDefaults.standard.bool(forKey: preferenceKey))
    }()
    private let api: any ControlPowerAsserting
    private var runtimes = Set<UUID>()
    private(set) var held = false
    var enabled: Bool
    var failure: (() -> Void)?
    private var failureReported = false

    init(api: any ControlPowerAsserting, enabled: Bool = true) {
        self.api = api; self.enabled = enabled
    }
    func update(_ runtime: UUID, active: Bool) {
        if active { runtimes.insert(runtime) } else { runtimes.remove(runtime) }
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
