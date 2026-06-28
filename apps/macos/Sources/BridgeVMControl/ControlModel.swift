import Foundation
import Combine

@MainActor
final class ControlModel: ObservableObject {
    // Status
    @Published var running = false
    @Published var ip: String = "—"
    @Published var memGiB: Double = 8
    @Published var cpu: Int = 4
    @Published var busy = false
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

    init(config: VMConfig) {
        self.config = config
        self.backend = config.makeBackend()
        refresh()
        startPolling()
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
        let backend = self.backend
        Task.detached {
            let running = backend.isRunning()
            let ip = running ? backend.currentIP() : nil
            await MainActor.run {
                self.running = running
                self.ip = ip ?? "—"
            }
        }
    }

    /// Full refresh including the resources spec — for init and post-action bursts.
    func refresh() {
        let backend = self.backend
        Task.detached {
            let running = backend.isRunning()
            let ip = running ? backend.currentIP() : nil
            let res = backend.resources()
            await MainActor.run {
                self.running = running
                self.ip = ip ?? "—"
                if res.memMiB > 0 { self.memGiB = Double(res.memMiB) / 1024.0 }
                if res.cpu > 0 { self.cpu = res.cpu }
            }
        }
    }

    func start() {
        statusNote = "VM 시작 중…"
        running = true                 // optimistic: the click registers instantly
        let backend = self.backend
        Task.detached {
            backend.start()            // off the main thread — process spawn no longer hitches the UI
            await MainActor.run { self.scheduleRefreshBurst() }
        }
    }

    func stop() {
        statusNote = "VM 정지 중…"
        running = false                // optimistic
        let backend = self.backend
        Task.detached {
            backend.stop()
            await MainActor.run { self.statusNote = "정지됨"; self.scheduleRefreshBurst() }
        }
    }

    func applyResources() {
        let mem = Int((pendingMemGiB * 1024).rounded())
        let cpus = Int(pendingCPU)
        busy = true
        statusNote = "리소스 적용 중 (\(Int(pendingMemGiB))GB / \(cpus)코어)… VM 재시작"
        let backend = self.backend
        Task.detached {
            let ok = backend.setResources(memMiB: mem, cpu: cpus)
            let wasRunning = backend.isRunning()
            if wasRunning {
                backend.stop()
                try? await Task.sleep(nanoseconds: 2_500_000_000)
                backend.start()
            }
            await MainActor.run {
                self.busy = false
                self.statusNote = ok ? "리소스 적용됨: \(Int(self.pendingMemGiB))GB / \(cpus)코어" : "리소스 적용 실패"
                self.scheduleRefreshBurst()
            }
        }
    }

    func runTerminalCommand() {
        let cmd = terminalInput.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !cmd.isEmpty else { return }
        terminalLog += "dev@guest$ \(cmd)\n"
        terminalInput = ""
        busy = true
        let backend = self.backend
        Task.detached {
            let r = backend.runInGuest(cmd)
            await MainActor.run {
                self.terminalLog += r.output
                if !r.output.hasSuffix("\n") { self.terminalLog += "\n" }
                if r.code != 0 { self.terminalLog += "[exit \(r.code)]\n" }
                self.terminalLog += "\n"
                self.busy = false
            }
        }
    }

    func installPackages(_ packages: [String], label: String) {
        busy = true
        softwareLog = "\(label) 설치 중… (apt, 잠시 걸립니다)\n"
        let backend = self.backend
        let pkgList = packages.joined(separator: " ")
        Task.detached {
            let r = backend.runInGuest("sudo DEBIAN_FRONTEND=noninteractive apt-get install -y \(pkgList) 2>&1 | tail -25")
            await MainActor.run {
                self.softwareLog += r.output
                self.softwareLog += (r.code == 0) ? "\n✅ 설치 완료: \(label)\n" : "\n❌ 실패 (exit \(r.code))\n"
                self.busy = false
            }
        }
    }

    private func scheduleRefreshBurst() {
        for delay in [1.0, 3.0, 6.0, 10.0] {
            DispatchQueue.main.asyncAfter(deadline: .now() + delay) { [weak self] in self?.refresh() }
        }
    }
}
