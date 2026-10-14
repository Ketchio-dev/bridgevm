import Foundation

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
