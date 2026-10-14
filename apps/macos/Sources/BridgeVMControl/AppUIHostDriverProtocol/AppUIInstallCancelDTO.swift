#if (DEBUG && BRIDGEVM_APP_UI_HOST) || BRIDGEVM_APP_UI_DRIVER
import Foundation

enum AppUIInstallCancelConstants { static let refusal = "합성 UI 검증 종료" }

enum AppUIInstallCancelOperation: String, Codable, CaseIterable { case readState, start, cancel, retry }
enum AppUIInstallCancelPhase: String, Codable, CaseIterable {
    case idle, validating, cancelling, cancelled, retryFailed
}

struct AppUIInstallCancelSession: Codable, Equatable {
    var schemaVersion = 1
    var kind = "native-app-ui-install-cancel-session"
    let nonce: String
    let startedUptime: Double
    let deadlineUptime: Double
    let host: AppUIDriverProcessIdentity
    let driver: AppUIDriverProcessIdentity
}

struct AppUIInstallCancelRequest: Codable, Equatable {
    var schemaVersion = 1
    var kind = "native-app-ui-install-cancel-request"
    let sessionSHA256: String
    let nonce: String
    let sequence: UInt32
    let phase: AppUIInstallCancelPhase
    let phaseDeadlineUptime: Double
    let window: AppUIDriverWindow
    let operation: AppUIInstallCancelOperation
}

struct AppUIInstallCancelReply: Codable, Equatable {
    var schemaVersion = 1
    var kind = "native-app-ui-install-cancel-reply"
    let sessionSHA256: String
    let nonce: String
    let sequence: UInt32
    let requestSHA256: String
    let hostPID: Int32
    let driverPID: Int32
    let operation: AppUIInstallCancelOperation
    let outcome: AppUIDriverOutcome
    let values: AppUIInstallCancelValues?
    let failureCode: AppUIDriverFailure?
    let axError: Int32?
}

struct AppUIInstallCancelReady: Codable, Equatable {
    var schemaVersion = 1
    var kind = "native-app-ui-install-cancel-ready"
    let sessionSHA256: String
    let nonce: String
    let driver: AppUIDriverProcessIdentity
    let trusted: Bool
    let failureCode: AppUIDriverFailure?
    let codeRequirement: String?
    let codeRequirementUnavailableReason: String?
}

struct AppUIInstallCancelValues: Codable, Equatable {
    let installVisible: Bool
    let startVisible: Bool
    let startEnabled: Bool
    let cancelVisible: Bool
    let cancelEnabled: Bool
    let stage: String
    let title: String
    var failure: String? = nil
}
#endif
