extension HvfEngineSession {
    var runtimePendingWorkText: String? {
        if hasPendingRuntimeMutation { return "작업 중" }
        if let start = guiStartOperation, start.workerPending {
            return start.invalidationReason == nil ? "시작 준비 중" : "작업 정리 확인 중"
        }
        if let start = ownedStartOperation, start.workerPending {
            return start.observation.failure == nil ? "시작 준비 중" : "작업 정리 확인 중"
        }
        return nil
    }

    var runtimeSupervisionFailureText: String? {
        guard !runnerDiedWithoutCleanup, ownedController?.supervisionLost == true else { return nil }
        return "실행 중인 Windows와 연결이 끊겼으니 강제 중단으로 Windows를 멈추세요."
    }

    var guiStartFailureText: String? {
        if !hasPendingOwnedStart, let outcome = guiStartOperation?.outcome {
            switch outcome {
            case let .refused(reason): return reason
            case let .failed(_, detail): return "VM을 시작하지 못했습니다: \(detail)"
            case .ownedLaunchAccepted, .observedAttachment: break
            }
        }
        return runnerDiedWithoutCleanup ? "Windows가 예기치 않게 종료되었으니 잠시 후 다시 시작하세요." : nil
    }
}
