import Foundation
import Combine

/// Polled status of one library VM: rows, the dashboard and the library's
/// delete/duplicate/move guards read it. Starting, stopping and changing a
/// Windows VM go through its HvfEngineSession, never through this model.
@MainActor
final class ControlModel: ObservableObject {
    @Published var running = false
    @Published var memGiB: Double = 8
    @Published var cpu: Int = 4
    @Published var statusNote = ""

    let config: VMConfig
    let backend: VMBackend
    var timer: Timer?
    var refreshGeneration: UInt64 = 0
    var tick = 0

    init(config: VMConfig, backend: VMBackend? = nil, startsAutomatically: Bool = true) {
        self.config = config
        self.backend = backend ?? config.makeBackend()
        if startsAutomatically {
            refresh()
            startPolling()
        }
    }

    deinit { timer?.invalidate() }
}

extension ControlModel {
    func startPolling() {
        timer = Timer.scheduledTimer(withTimeInterval: 3.0, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.pollTick() }
        }
    }

    /// Running VMs refresh every 3s; idle VMs every ~12s. Idle VMs rarely change
    /// except via in-app actions, which refresh on their own — polling them
    /// hard just spins `pgrep` subprocesses and drains battery for nothing.
    func pollTick() {
        tick &+= 1
        if running || tick % 4 == 0 { refreshStatus() }
    }

    /// Lean periodic refresh: process liveness only. Skips the static resources
    /// spec, which changes only when the VM's settings are saved.
    func refreshStatus() {
        refreshGeneration &+= 1
        let generation = refreshGeneration
        let backend = self.backend
        Task.detached {
            let running = backend.isRunning()
            await MainActor.run {
                guard self.refreshGeneration == generation else { return }
                self.applyRuntimeStatus(running: running)
            }
        }
    }

    /// Full refresh including the resources spec — for init and explicit refreshes.
    func refresh() {
        refreshGeneration &+= 1
        let generation = refreshGeneration
        let backend = self.backend
        Task.detached {
            let running = backend.isRunning()
            let res = backend.resources()
            await MainActor.run {
                guard self.refreshGeneration == generation else { return }
                self.applyRuntimeStatus(running: running)
                if res.memMiB > 0 { self.memGiB = Double(res.memMiB) / 1024.0 }
                if res.cpu > 0 { self.cpu = res.cpu }
            }
        }
    }

    func applyRuntimeStatus(running: Bool) {
        let wasRunning = self.running
        self.running = running
        guard wasRunning != running else { return }
        statusNote = running ? "Running" : "VM exited"
    }
}
