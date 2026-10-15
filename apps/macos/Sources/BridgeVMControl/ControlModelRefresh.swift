import Foundation

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
