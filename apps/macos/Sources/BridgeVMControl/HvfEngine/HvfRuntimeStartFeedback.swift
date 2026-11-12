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
        // A failed lifecycle receipt alone says nothing about storage, and must not hide
        // the specific startup error. Only the helper's final-persistence diagnostic does.
        if guiStartFailureText == nil, diskSyncNotice.finalSaveFailed {
            return "Windows를 종료하면서 디스크를 Mac에 안전하게 저장하지 못했으니 Mac의 저장 공간과 디스크 연결을 확인하세요."
        }
        guard ownedController?.supervisionLost == true else { return nil }
        return "실행 중인 Windows와 연결이 끊겼으니 강제 중단으로 Windows를 멈추세요."
    }

    var guiStartFailureText: String? {
        guard !hasPendingOwnedStart, let outcome = guiStartOperation?.outcome else { return nil }
        switch outcome {
        case let .refused(reason): return reason
        case let .failed(_, detail): return "VM을 시작하지 못했습니다: \(detail)"
        case .ownedLaunchAccepted, .observedAttachment: return nil
        }
    }
}
