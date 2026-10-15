#if (DEBUG && BRIDGEVM_APP_UI_HOST) || BRIDGEVM_APP_UI_DRIVER
import Foundation

enum AppUIInstallCancelOperationPolicy {
    static let syntheticFailure = AppUIInstallCancelConstants.refusal

    static func operations(for phase: AppUIInstallCancelPhase) -> Set<AppUIInstallCancelOperation> {
        if let mutation = mutation(for: phase) { return [.readState, mutation] }
        return [.readState]
    }

    static func mutation(for phase: AppUIInstallCancelPhase) -> AppUIInstallCancelOperation? {
        switch phase {
        case .idle, .cancelled: return nil
        case .validating: return .start
        case .cancelling: return .cancel
        case .retryFailed: return .retry
        }
    }

    static func isMutation(_ operation: AppUIInstallCancelOperation) -> Bool { operation != .readState }
    static func window(for operation: AppUIInstallCancelOperation) -> AppUIDriverWindow { .main }

    static func completes(_ phase: AppUIInstallCancelPhase, reply: AppUIInstallCancelReply) -> Bool {
        guard reply.operation == .readState, reply.outcome == .observed, let value = reply.values,
              value.installVisible, value.startVisible else { return false }
        switch phase {
        case .idle:
            return value.stage == "대기" && value.title == "설치 시작" && value.startEnabled
                && !value.cancelVisible && !value.cancelEnabled && value.failure == nil
        case .validating:
            return value.stage == "설치 입력 확인" && !value.startEnabled
                && value.cancelVisible && value.cancelEnabled && value.failure == nil
        case .cancelling:
            return value.stage == "취소 중…" && !value.startEnabled
                && value.cancelVisible && !value.cancelEnabled && value.failure == nil
        case .cancelled:
            return value.stage == "취소됨" && value.title == "다시 시도" && value.startEnabled
                && !value.cancelVisible && !value.cancelEnabled && value.failure == nil
        case .retryFailed:
            return value.stage == "실패" && value.title == "다시 시도" && value.startEnabled
                && !value.cancelVisible && !value.cancelEnabled && value.failure == syntheticFailure
        }
    }
}
#endif
