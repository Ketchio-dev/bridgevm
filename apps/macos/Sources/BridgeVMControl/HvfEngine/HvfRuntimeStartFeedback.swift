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

    var guiStartFailureText: String? {
        guard !hasPendingOwnedStart, let outcome = guiStartOperation?.outcome else { return nil }
        switch outcome {
        case let .refused(reason): return reason
        case let .failed(_, detail): return "VM을 시작하지 못했습니다: \(detail)"
        case .ownedLaunchAccepted, .observedAttachment: return nil
        }
    }
}
