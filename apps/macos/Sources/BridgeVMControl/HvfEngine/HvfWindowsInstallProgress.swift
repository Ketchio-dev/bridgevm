import Foundation

enum HvfWindowsInstallStage: Equatable {
    case idle
    case validating
    case preparingSource
    case installing
    case finalizing
    case recovering
    case cancelling
    case cancelled
    case done
    case failed(String)

    var isRunning: Bool {
        switch self {
        case .validating, .preparingSource, .installing, .finalizing, .recovering, .cancelling: return true
        default: return false
        }
    }
}
