import Foundation

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
        statusNote = "리소스 적용 중 (\(Int(pendingMemGiB))GB / \(cpus)코어)… VM 재시작"
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
                self.statusNote = "리소스 적용됨: \(mem / 1024)GB / \(cpus)코어"
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
