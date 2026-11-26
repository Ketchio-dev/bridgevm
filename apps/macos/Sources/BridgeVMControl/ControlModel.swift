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
    var acceptedResourceConfiguration: VMConfig?
    var onResourceUpdateFailed: (@MainActor () -> Void)?
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

    func publishSavedResources(memoryMiB: Int, cpuCount: Int) {
        invalidateRefreshes()
        config.memMiB = memoryMiB; config.cpuCount = cpuCount
        acceptedResourceConfiguration = nil
        memGiB = Double(memoryMiB) / 1024.0; cpu = cpuCount
        onConfigurationPersisted?()
    }

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

}

extension ControlModel {
    static func packageInstallCommand(_ packages: [String]) -> String? {
        let names = packages.map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
        guard !names.isEmpty,
              names.allSatisfy({
                  !$0.isEmpty && $0.range(
                      of: #"^[a-z0-9][a-z0-9+.-]*(?::[a-z0-9][a-z0-9-]*)?$"#,
                      options: .regularExpression
                  ) != nil
              }) else { return nil }
        let arguments = names.map(Shell.shQuote).joined(separator: " ")
        return Shell.shellCommand("bash", ["-o", "pipefail", "-c", "sudo DEBIAN_FRONTEND=noninteractive apt-get install -y \(arguments) 2>&1 | tail -25"])
    }

    static func boundedLog(_ value: String, limit: Int) -> String {
        guard limit > 0, value.count > limit else { return value }
        return "… 이전 로그 생략 …\n" + String(value.suffix(limit))
    }
}

extension ControlModel {
    func runTerminalCommand() {
        guard !refuseRetainedMutation(), !busy, !lifecycleBusy, running, backend.supportsGuestCommands else { return }
        let cmd = terminalInput.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !cmd.isEmpty else { return }
        terminalLog = Self.boundedLog(terminalLog + "dev@guest$ \(cmd)\n", limit: Self.terminalLogCharacterLimit)
        terminalInput = ""
        busy = true
        let backend = self.backend
        Task.detached {
            let r = backend.runInGuest(cmd)
            await MainActor.run {
                var addition = r.output
                if !r.output.hasSuffix("\n") { addition += "\n" }
                if r.code != 0 { addition += "[exit \(r.code)]\n" }
                addition += "\n"
                self.terminalLog = Self.boundedLog(
                    self.terminalLog + addition,
                    limit: Self.terminalLogCharacterLimit
                )
                self.busy = false
            }
        }
    }

    func installPackages(_ packages: [String], label: String) {
        guard !refuseRetainedMutation(), !busy, !lifecycleBusy, running, backend.supportsPackageInstall else { return }
        guard let command = Self.packageInstallCommand(packages) else {
            softwareLog = "설치 요청이 올바르지 않습니다. 패키지 이름을 확인해 주세요.\n"
            return
        }
        busy = true
        softwareLog = "\(label) 설치 중… (apt, 잠시 걸립니다)\n"
        let backend = self.backend
        Task.detached {
            let r = backend.runInGuest(command)
            await MainActor.run {
                let result = r.output + ((r.code == 0) ? "\n✅ 설치 완료: \(label)\n" : "\n❌ 실패 (exit \(r.code))\n")
                self.softwareLog = Self.boundedLog(
                    self.softwareLog + result,
                    limit: Self.softwareLogCharacterLimit
                )
                self.busy = false
            }
        }
    }

}

struct ControlRuntimeLaunch {
    let accepted: Bool
    let owned: GenericOwnedRuntime?

    static func begin(_ backend: VMBackend) -> Self {
        if let backend = backend as? VMOwnedLaunchingBackend {
            guard let plan = backend.ownedLaunchPlan() else { return .init(accepted: false, owned: nil) }
            return .init(accepted: true, owned: GenericOwnedRuntime(plan: plan))
        }
        return .init(accepted: backend.start(), owned: nil)
    }

    static func stop(_ owned: GenericOwnedRuntime) async -> Bool {
        owned.requestStop()
        let deadline = DispatchTime.now() + .seconds(12)
        while owned.snapshot.isActive, DispatchTime.now() < deadline {
            try? await Task.sleep(nanoseconds: 20_000_000)
        }
        return !owned.snapshot.isActive
    }
}

extension ControlModel {
    var hasActiveRuntimeWork: Bool {
        running || hasAcceptedOperation || ownedRuntime?.snapshot.isActive == true
    }

    var retainedStopAvailable: Bool { ownedRuntime?.snapshot.isActive == true && !lifecycleBusy }

    func refuseRetainedMutation() -> Bool {
        guard isRetained else { return false }
        statusNote = "Wait for this session to finish, then open it under Recent operations in the sidebar, choose Dismiss, and try again."
        return true
    }

    func finishRetainedObservationIfTerminal() {
        guard isRetained, ownedRuntime == nil, !running, !hasAcceptedOperation else { return }
        retainedObservationEnded = true
        timer?.invalidate()
        timer = nil
        invalidateRefreshes()
    }

    func applyOwnedRuntimeStatus() -> Bool {
        guard let ownedRuntime else { return false }
        let state = ownedRuntime.snapshot
        switch state.phase {
        case .starting: return true // Keep the accepted startup observation until it completes.
        case .running: return false
        case .stopping:
            running = state.primaryRunning
            statusNote = "VM 정지 중…"
        case .terminal:
            running = false
            ip = "—"
            startConfirmationDeadline = nil
            statusNote = state.failure ?? "정지됨"
        }
        return true
    }
}

extension ControlModel {
    func start() {
        guard !lifecycleBusy, !hasActiveRuntimeWork, !refuseRetainedMutation() else { return }
        guard workAdmission?(true) == nil else { return }
        invalidateRefreshes()
        setLifecycleBusy(true)
        statusNote = "VM 시작 중…"
        startConfirmationDeadline = Date().addingTimeInterval(10)
        running = true                 // optimistic: the click registers instantly
        let backend = self.backend
        Task.detached {
            let launch = ControlRuntimeLaunch.begin(backend)
            await MainActor.run {
                self.setLifecycleBusy(false)
                if let owned = launch.owned { self.ownedRuntime = owned }
                if launch.accepted {
                    self.statusNote = "VM 부팅 중…"
                } else {
                    self.startConfirmationDeadline = nil
                    self.running = false
                    self.ip = "—"
                    self.statusNote = "VM 시작 실패"
                }
                self.scheduleRefreshBurst()
            }
        }
    }

    func stop() {
        guard !lifecycleBusy, running || ownedRuntime?.snapshot.isActive == true else { return }
        guard !isRetained || ownedRuntime != nil else {
            statusNote = "이 실행의 프로세스 소유권을 확인할 수 없어 정지할 수 없습니다."
            return
        }
        guard ownedRuntime != nil || legacyStopAdmission?(true) == nil else { return }
        invalidateRefreshes()
        setLifecycleBusy(true)
        let owned = ownedRuntime
        let wasStarting = startConfirmationDeadline != nil
        startConfirmationDeadline = nil
        statusNote = "VM 정지 중…"
        running = false                // optimistic
        let backend = self.backend
        Task.detached {
            if let owned {
                let stopped = await ControlRuntimeLaunch.stop(owned)
                await MainActor.run {
                    self.setLifecycleBusy(false)
                    self.running = owned.snapshot.primaryRunning
                    self.statusNote = stopped ? "정지됨" : "VM 정지 확인 필요"
                    self.scheduleRefreshBurst()
                }
                return
            }
            if wasStarting {
                let appearanceDeadline = Date().addingTimeInterval(3)
                while !backend.isRunning(), Date() < appearanceDeadline { usleep(50_000) }
            }
            if backend.isRunning() { backend.stop() }
            let stillRunning = backend.isRunning()
            await MainActor.run {
                self.setLifecycleBusy(false)
                self.running = stillRunning
                self.statusNote = stillRunning ? "VM 정지 실패" : "정지됨"
                self.scheduleRefreshBurst()
            }
        }
    }

}

extension ControlModel {
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
        var expected = config
        expected.id = config.slug
        expected.memMiB = mem; expected.cpuCount = cpus
        acceptedResourceConfiguration = expected
        invalidateRefreshes()
        busy = true
        setLifecycleBusy(true)
        statusNote = "리소스 적용 중 (\(Int(pendingMemGiB)) GiB / \(cpus)코어)… VM 재시작"
        let backend = self.backend
        let previousOwner = ownedRuntime
        Task.detached {
            let ok = backend.setResources(memMiB: mem, cpu: cpus)
            guard ok else {
                await MainActor.run {
                    self.acceptedResourceConfiguration = nil
                    self.busy = false
                    self.setLifecycleBusy(false)
                    self.statusNote = "리소스 적용 실패 — VM은 재시작하지 않았습니다."
                    self.onResourceUpdateFailed?()
                }
                return
            }
            await MainActor.run {
                self.publishSavedResources(memoryMiB: mem, cpuCount: cpus)
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
                        self.setLifecycleBusy(false)
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
                        self.setLifecycleBusy(false)
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
                self.setLifecycleBusy(false)
                self.statusNote = "리소스 적용됨: \(mem / 1024) GiB / \(cpus)코어"
                self.scheduleRefreshBurst()
            }
        }
    }

    /// Equality recognizes only a still-accepted allocation, never an external writer's identity.
    func matchesRegisteredConfiguration(_ current: VMConfig) -> Bool {
        if config == current { return true }
        return !isRetained && busy && lifecycleBusy && acceptedResourceConfiguration == current
    }
}

extension ControlModel {
    func startPolling() {
        timer = Timer.scheduledTimer(withTimeInterval: 3.0, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.pollTick() }
        }
    }

    /// Running VMs refresh every 3s; idle VMs every ~12s. Idle VMs rarely change
    /// except via in-app actions, which fire their own refresh burst — polling them
    /// hard just spins `pgrep` subprocesses and drains battery for nothing.
    func pollTick() {
        tick &+= 1
        if hasActiveRuntimeWork || tick % 4 == 0 { refreshStatus() }
    }

    /// Lean periodic refresh: process liveness (+ IP only while running). Skips the
    /// static resources spec, which only changes via applyResources().
    func refreshStatus() {
        guard !retainedObservationEnded else { return }
        refreshGeneration &+= 1
        let generation = refreshGeneration
        let backend = self.backend
        let owned = ownedRuntime
        Task.detached {
            let running = owned.map { $0.snapshot.primaryRunning } ?? backend.isRunning()
            let ip = running ? backend.currentIP() : nil
            await MainActor.run {
                guard self.refreshGeneration == generation else { return }
                self.applyRuntimeStatus(running: running, ip: ip)
            }
        }
    }

    /// Full refresh including the resources spec — for init and post-action bursts.
    func refresh() {
        guard !retainedObservationEnded else { return }
        refreshGeneration &+= 1
        let generation = refreshGeneration
        let backend = self.backend
        let owned = ownedRuntime
        Task.detached {
            let running = owned.map { $0.snapshot.primaryRunning } ?? backend.isRunning()
            let ip = running ? backend.currentIP() : nil
            let res = backend.resources()
            await MainActor.run {
                guard self.refreshGeneration == generation else { return }
                self.applyRuntimeStatus(running: running, ip: ip)
                if res.memMiB > 0 { self.memGiB = Double(res.memMiB) / 1024.0 }
                if res.cpu > 0 { self.cpu = res.cpu }
            }
        }
    }

    func scheduleRefreshBurst() {
        for delay in [1.0, 3.0, 6.0, 10.5] {
            DispatchQueue.main.asyncAfter(deadline: .now() + delay) { [weak self] in self?.refresh() }
        }
    }

    func invalidateRefreshes() { refreshGeneration &+= 1 }

    func applyRuntimeStatus(running: Bool, ip: String?) {
        if applyOwnedRuntimeStatus() { return }
        defer { finishRetainedObservationIfTerminal() }
        if let deadline = startConfirmationDeadline {
            self.ip = ip ?? "—"
            if running {
                self.running = true
                startConfirmationDeadline = nil
                statusNote = "실행 중"
            } else if Date() >= deadline {
                self.running = false
                startConfirmationDeadline = nil
                statusNote = "VM 시작 확인 실패"
            }
            // Before the deadline, keep the optimistic running state. A host
            // process commonly appears a little after the launch shell exits.
            return
        }

        let wasRunning = self.running
        self.running = running
        self.ip = ip ?? "—"
        guard wasRunning != running, !lifecycleBusy else { return }
        if running {
            statusNote = "실행 중"
        } else {
            statusNote = "VM이 종료됨"
        }
    }
}
