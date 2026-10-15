import Foundation
import Combine

@MainActor
final class ControlModel: ObservableObject {
    static let terminalLogCharacterLimit = 200_000
    static let softwareLogCharacterLimit = 100_000
    // Status
    @Published var running = false { didSet { finishRetainedObservationIfTerminal() } }
    @Published var ip: String = "—"
    @Published var memGiB: Double = 8
    @Published var cpu: Int = 4
    @Published var busy = false { didSet { finishRetainedObservationIfTerminal() } }
    @Published private(set) var lifecycleBusy = false { didSet { finishRetainedObservationIfTerminal() } }
    @Published var statusNote = ""

    // Resource editor (pending values until Apply)
    @Published var pendingMemGiB: Double = 8
    @Published var pendingCPU: Double = 4

    // Terminal
    @Published var terminalInput = ""
    @Published var terminalLog = "BridgeVM Control — 게스트 터미널\n준비되면 명령을 입력하세요 (예: uname -a, df -h).\n\n"

    // Software install output
    @Published var softwareLog = ""

    private(set) var config: VMConfig
    let backend: VMBackend
    var workAdmission: LibraryWorkAdmission?
    var legacyStopAdmission: LibraryWorkAdmission?
    var onConfigurationPersisted: (@MainActor () -> Void)?
    var timer: Timer?
    var startConfirmationDeadline: Date?
    var refreshGeneration: UInt64 = 0
    var tick = 0
    var ownedRuntime: GenericOwnedRuntime?
    var retainedObservationEnded = false
    private(set) var isRetained = false

    func retainForRecovery() { isRetained = true }
    func setLifecycleBusy(_ value: Bool) { lifecycleBusy = value }

    init(config: VMConfig, backend: VMBackend? = nil, startsAutomatically: Bool = true) {
        self.config = config
        self.backend = backend ?? config.makeBackend()
        if startsAutomatically {
            refresh()
            startPolling()
        }
    }

    deinit { timer?.invalidate() }

    var displayName: String { backend.displayName }
    var hasAcceptedOperation: Bool { lifecycleBusy || busy || startConfirmationDeadline != nil }

    func applyResources() {
        guard !refuseRetainedMutation(), !busy, !lifecycleBusy else { return }
        guard workAdmission?(true) == nil else { return }
        guard backend.supportsResourceChanges else {
            statusNote = "이 엔진은 리소스 변경을 지원하지 않습니다."
            return
        }
        guard let allocation = ControlResourceAllocation(memoryGiB: pendingMemGiB, cpu: pendingCPU) else {
            statusNote = "메모리와 CPU 값이 올바르지 않습니다."
            return
        }
        let mem = allocation.memoryMiB, cpus = allocation.cpu
        invalidateRefreshes()
        busy = true
        lifecycleBusy = true
        statusNote = "리소스 적용 중 (\(Int(pendingMemGiB))GB / \(cpus)코어)… VM 재시작"
        let backend = self.backend
        let previousOwner = ownedRuntime
        Task.detached {
            let ok = backend.setResources(memMiB: mem, cpu: cpus)
            guard ok else {
                await MainActor.run {
                    self.busy = false
                    self.lifecycleBusy = false
                    self.statusNote = "리소스 적용 실패 — VM은 재시작하지 않았습니다."
                }
                return
            }
            await MainActor.run {
                self.invalidateRefreshes()
                self.config.memMiB = mem; self.config.cpuCount = cpus
                self.memGiB = Double(mem) / 1024.0
                self.cpu = cpus
                self.onConfigurationPersisted?()
            }
            let wasRunning = previousOwner.map { $0.snapshot.isActive } ?? backend.isRunning()
            if wasRunning {
                let stopped: Bool
                if let previousOwner { stopped = await ControlRuntimeLaunch.stop(previousOwner) }
                else {
                    backend.stop()
                    try? await Task.sleep(nanoseconds: 2_500_000_000)
                    stopped = !backend.isRunning()
                }
                if !stopped {
                    await MainActor.run {
                        self.busy = false
                        self.lifecycleBusy = false
                        self.running = true
                        self.statusNote = "리소스 적용됨, VM 정지 실패 — 재시작하지 않았습니다."
                        self.scheduleRefreshBurst()
                    }
                    return
                }
                let launch = ControlRuntimeLaunch.begin(backend)
                if let owned = launch.owned { await MainActor.run { self.ownedRuntime = owned } }
                if !launch.accepted {
                    await MainActor.run {
                        self.busy = false
                        self.lifecycleBusy = false
                        self.running = false
                        self.ip = "—"
                        self.statusNote = "리소스 적용됨, VM 재시작 실패"
                        self.scheduleRefreshBurst()
                    }
                    return
                }
            }
            await MainActor.run {
                self.busy = false
                self.lifecycleBusy = false
                self.statusNote = "리소스 적용됨: \(mem / 1024)GB / \(cpus)코어"
                self.scheduleRefreshBurst()
            }
        }
    }

}
