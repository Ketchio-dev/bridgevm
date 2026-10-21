import Foundation

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
        statusNote = "보관한 제어에서는 새 작업을 시작할 수 없습니다. 실행이 끝난 뒤 사이드바 '진행 중이거나 종료된 작업'에서 이 항목을 '목록에서 닫기'로 닫고 다시 시도하세요."
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
