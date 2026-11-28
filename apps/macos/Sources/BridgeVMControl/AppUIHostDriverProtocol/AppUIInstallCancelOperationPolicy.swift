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
        let stage = currentLabel(value.stage)
        let title = currentLabel(value.title)
        switch phase {
        case .idle:
            return stage == "Ready to install" && title == "Start Installation" && value.startEnabled
                && !value.cancelVisible && !value.cancelEnabled && value.failure == nil
        case .validating:
            return stage == "Checking installation files" && !value.startEnabled
                && value.cancelVisible && value.cancelEnabled && value.failure == nil
        case .cancelling:
            return stage == "Canceling…" && !value.startEnabled
                && value.cancelVisible && !value.cancelEnabled && value.failure == nil
        case .cancelled:
            return stage == "Canceled" && title == "Try Again" && value.startEnabled
                && !value.cancelVisible && !value.cancelEnabled && value.failure == nil
        case .retryFailed:
            return stage == "Failed" && title == "Try Again" && value.startEnabled
                && !value.cancelVisible && !value.cancelEnabled && value.failure.map(currentLabel) == syntheticFailure
        }
    }

    // Previous development builds exposed these action titles and stages in Korean, and then
    // spelled the cancel stages "Cancelling…" and "Cancelled".
    static func currentLabel(_ text: String) -> String {
        switch text {
        case "대기": return "Ready to install"
        case "설치 입력 확인": return "Checking installation files"
        case "취소 중…", "Cancelling…": return "Canceling…"
        case "취소됨", "Cancelled": return "Canceled"
        case "실패": return "Failed"
        case "설치 시작": return "Start Installation"
        case "다시 시도": return "Try Again"
        case "합성 UI 검증 종료": return "Synthetic UI check complete"
        default: return text
        }
    }
}
#endif
