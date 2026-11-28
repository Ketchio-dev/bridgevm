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
/// It also knows the memory of each running VM, for the notices below.
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
    @Published private(set) var runningMemoryMiB: [UUID: Int] = [:]
    private(set) var held = false
    @Published var enabled: Bool { didSet { reconcile() } }
    /// The running Windows tasks while the assertion could not be taken.
    @Published private(set) var keepAwakeFailed = Set<UUID>()

    init(api: any ControlPowerAsserting, enabled: Bool = true) {
        self.api = api; self.enabled = enabled
    }
    func update(_ runtime: UUID, active: Bool, ramMiB: Int? = nil) {
        if active { runtimes.insert(runtime) } else { runtimes.remove(runtime) }
        let memory = active ? ramMiB : nil
        if runningMemoryMiB[runtime] != memory { runningMemoryMiB[runtime] = memory }
        reconcile()
    }
    /// Guest memory is ordinary swappable Mac memory with no balloon: VMs that together use more than
    /// one VM may (CreateVMMemoryOptions) make macOS swap. Each of them says so; nothing is refused.
    /// One VM alone says so too: one made on a Mac with more memory keeps its amount (Settings keeps
    /// the saved value selectable).
    func sharedMemoryNotice(for runtime: UUID,
                            hostBytes: UInt64 = ProcessInfo.processInfo.physicalMemory) -> String? {
        let totalMiB = runningMemoryMiB.values.reduce(0, +)
        guard runningMemoryMiB[runtime] != nil,
              totalMiB > CreateVMMemoryOptions.ceilingMiB(hostBytes: hostBytes) else { return nil }
        guard runningMemoryMiB.count > 1 else {
            return "This VM uses \((totalMiB + 512) / 1024) GiB of memory, more than this Mac can spare, "
                + "so Windows and your Mac may slow down. Give it less memory in Settings."
        }
        return "Your running VMs use \((totalMiB + 512) / 1024) GiB of memory together, more than this Mac can spare, "
            + "so Windows and your Mac may slow down. Stop one, or give the VMs less memory in Settings."
    }
    /// Each running VM and Windows install shows it (HvfKeepAwakeNoticeView) while the Mac can't be
    /// kept awake; it goes away once it can, the setting is off or that task ends.
    func keepAwakeNotice(for runtime: UUID) -> String? {
        guard keepAwakeFailed.contains(runtime) else { return nil }
        return "BridgeVM couldn’t keep your Mac awake, so check its sleep settings before leaving Windows running unattended."
    }
    func setEnabled(_ value: Bool, defaults: UserDefaults = .standard) {
        defaults.set(value, forKey: Self.preferenceKey)
        enabled = value
    }
    private func reconcile() {
        let needed = enabled && !runtimes.isEmpty
        if needed && !held { held = api.acquire() } else if !needed && held { api.release(); held = false }
        let failed = needed && !held ? runtimes : []
        if keepAwakeFailed != failed { keepAwakeFailed = failed }
    }
    func shutdown() {
        runtimes.removeAll(); runningMemoryMiB.removeAll()
        if held { api.release(); held = false }
    }
}
#endif
