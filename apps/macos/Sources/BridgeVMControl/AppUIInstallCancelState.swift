#if DEBUG && BRIDGEVM_APP_UI_HOST
import Foundation

@MainActor
extension AppUIInstallCancelFixture {
    func matches(_ phase: AppUIInstallCancelPhase) throws -> Bool {
        guard let session else { return false }
        guard try preserved() else { throw AppUIDriverFailure.invalidFile }
        let counts = counters.snapshot
        let permitted = ["plan_builds", "model_creations", "install_creations", "validation_calls",
            "validation_finished", "process_resets", "process_cancels"]
        guard counts.allSatisfy({ permitted.contains($0.key) || $0.value == 0 }) else {
            throw AppUIDriverFailure.internalFailure
        }
        let expected = Self.expected(phase)
        guard expected.allSatisfy({ counts[$0.key] == $0.value }) else { return false }
        switch phase {
        case .idle: return session.stage == .idle && !session.isRunning && !session.canCancel
        case .validating: return session.stage == .validating && session.isRunning && session.canCancel
        case .cancelling: return session.stage == .cancelling && session.isRunning && !session.canCancel
        case .cancelled: return session.stage == .cancelled && !session.isRunning && !session.canCancel
        case .retryFailed: return session.stage == .failed(AppUIInstallCancelConstants.refusal)
                && !session.isRunning && !session.canCancel
        }
    }

    static func expected(_ phase: AppUIInstallCancelPhase) -> [String: Int] {
        let calls = phase == .idle ? 0 : phase == .retryFailed ? 2 : 1
        let finished = phase == .cancelled ? 1 : phase == .retryFailed ? 2 : 0
        return ["plan_builds": 1, "model_creations": 1, "install_creations": 1,
            "validation_calls": calls, "validation_finished": finished, "process_resets": calls,
            "process_cancels": [.cancelling, .cancelled, .retryFailed].contains(phase) ? 1 : 0]
    }
}
#endif
