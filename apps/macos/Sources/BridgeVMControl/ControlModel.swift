import Foundation
import Combine

@MainActor
final class ControlModel: ObservableObject {
    static let terminalLogCharacterLimit = 200_000
    static let softwareLogCharacterLimit = 100_000
    // Status
    @Published var running = false
    @Published var ip: String = "—"
    @Published var memGiB: Double = 8
    @Published var cpu: Int = 4
    @Published var busy = false
    @Published private(set) var lifecycleBusy = false
    @Published var statusNote = ""

    // Resource editor (pending values until Apply)
    @Published var pendingMemGiB: Double = 8
    @Published var pendingCPU: Double = 4

    // Terminal
    @Published var terminalInput = ""
    @Published var terminalLog = "BridgeVM Control — 게스트 터미널\n준비되면 명령을 입력하세요 (예: uname -a, df -h).\n\n"

    // Software install output
    @Published var softwareLog = ""

    let config: VMConfig
    let backend: VMBackend
    private var timer: Timer?
    private var startConfirmationDeadline: Date?
    private var refreshGeneration: UInt64 = 0

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

    func startPolling() {
        timer = Timer.scheduledTimer(withTimeInterval: 3.0, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.pollTick() }
        }
    }

    private var tick = 0
    /// Running VMs refresh every 3s; idle VMs every ~12s. Idle VMs rarely change
    /// except via in-app actions, which fire their own refresh burst — polling them
    /// hard just spins `pgrep` subprocesses and drains battery for nothing.
    private func pollTick() {
        tick &+= 1
        if running || tick % 4 == 0 { refreshStatus() }
    }

    /// Lean periodic refresh: process liveness (+ IP only while running). Skips the
    /// static resources spec, which only changes via applyResources().
    func refreshStatus() {
        refreshGeneration &+= 1
        let generation = refreshGeneration
        let backend = self.backend
        Task.detached {
            let running = backend.isRunning()
            let ip = running ? backend.currentIP() : nil
            await MainActor.run {
                guard self.refreshGeneration == generation else { return }
                self.applyRuntimeStatus(running: running, ip: ip)
            }
        }
    }

    /// Full refresh including the resources spec — for init and post-action bursts.
    func refresh() {
        refreshGeneration &+= 1
        let generation = refreshGeneration
        let backend = self.backend
        Task.detached {
            let running = backend.isRunning()
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

    func start() {
        guard !lifecycleBusy, !running else { return }
        invalidateRefreshes()
        lifecycleBusy = true
        statusNote = "VM 시작 중…"
        startConfirmationDeadline = Date().addingTimeInterval(10)
        running = true                 // optimistic: the click registers instantly
        let backend = self.backend
        Task.detached {
            let launched = backend.start() // off the main thread — process spawn no longer hitches the UI
            await MainActor.run {
                self.lifecycleBusy = false
                if launched {
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
        guard !lifecycleBusy, running else { return }
        invalidateRefreshes()
        lifecycleBusy = true
        let wasStarting = startConfirmationDeadline != nil
        startConfirmationDeadline = nil
        statusNote = "VM 정지 중…"
        running = false                // optimistic
        let backend = self.backend
        Task.detached {
            if wasStarting {
                let appearanceDeadline = Date().addingTimeInterval(3)
                while !backend.isRunning(), Date() < appearanceDeadline { usleep(50_000) }
            }
            if backend.isRunning() { backend.stop() }
            let stillRunning = backend.isRunning()
            await MainActor.run {
                self.lifecycleBusy = false
                self.running = stillRunning
                self.statusNote = stillRunning ? "VM 정지 실패" : "정지됨"
                self.scheduleRefreshBurst()
            }
        }
    }

    func applyResources() {
        guard !busy, !lifecycleBusy else { return }
        guard backend.supportsResourceChanges else {
            statusNote = "이 엔진은 리소스 변경을 지원하지 않습니다."
            return
        }
        guard pendingMemGiB.isFinite, pendingCPU.isFinite,
              pendingMemGiB >= Double(VMResourceLimits.minimumMemoryMiB) / 1024,
              pendingMemGiB <= Double(VMResourceLimits.maximumMemoryMiB) / 1024,
              pendingCPU.rounded() == pendingCPU,
              pendingCPU >= Double(VMResourceLimits.minimumCPU),
              pendingCPU <= Double(VMResourceLimits.maximumCPU) else {
            statusNote = "메모리와 CPU 값이 올바르지 않습니다."
            return
        }
        let mem = Int((pendingMemGiB * 1024).rounded())
        let cpus = Int(pendingCPU)
        invalidateRefreshes()
        busy = true
        lifecycleBusy = true
        statusNote = "리소스 적용 중 (\(Int(pendingMemGiB))GB / \(cpus)코어)… VM 재시작"
        let backend = self.backend
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
            let wasRunning = backend.isRunning()
            if wasRunning {
                backend.stop()
                try? await Task.sleep(nanoseconds: 2_500_000_000)
                if backend.isRunning() {
                    await MainActor.run {
                        self.busy = false
                        self.lifecycleBusy = false
                        self.running = true
                        self.statusNote = "리소스 적용됨, VM 정지 실패 — 재시작하지 않았습니다."
                        self.scheduleRefreshBurst()
                    }
                    return
                }
                if !backend.start() {
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
                self.statusNote = "리소스 적용됨: \(Int(self.pendingMemGiB))GB / \(cpus)코어"
                self.scheduleRefreshBurst()
            }
        }
    }

    func runTerminalCommand() {
        guard !busy, !lifecycleBusy, running, backend.supportsGuestCommands else { return }
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
        guard !busy, !lifecycleBusy, running, backend.supportsPackageInstall else { return }
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
        return "sudo DEBIAN_FRONTEND=noninteractive apt-get install -y \(arguments) 2>&1 | tail -25"
    }

    static func boundedLog(_ value: String, limit: Int) -> String {
        guard limit > 0, value.count > limit else { return value }
        return "… 이전 로그 생략 …\n" + String(value.suffix(limit))
    }

    private func scheduleRefreshBurst() {
        for delay in [1.0, 3.0, 6.0, 10.5] {
            DispatchQueue.main.asyncAfter(deadline: .now() + delay) { [weak self] in self?.refresh() }
        }
    }

    private func invalidateRefreshes() {
        refreshGeneration &+= 1
    }

    private func applyRuntimeStatus(running: Bool, ip: String?) {
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
