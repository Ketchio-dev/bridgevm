import Foundation

extension NativeInstallControlCodec {
    static let maximumLogLines = 64
    static let maximumLogLineBytes = 4_096
    static let maximumLogBytes = 32_768
    static let maximumFailureBytes = 4_096

    static func validate(_ value: NativeInstallObservation) throws {
        let logSizes = value.logTail.map { $0.utf8.count }
        guard NativeRuntimeCodec.validUUID(value.operationID),
              NativeRuntimeCodec.validDigest(value.expectedSavedConfigurationDigest),
              value.acceptedUptime.isFinite, value.acceptedUptime >= 0,
              value.logTail.count <= maximumLogLines,
              logSizes.allSatisfy({ $0 <= maximumLogLineBytes }),
              logSizes.reduce(0, +) <= maximumLogBytes,
              value.failure?.isEmpty != true,
              (value.failure?.utf8.count ?? 0) <= maximumFailureBytes else {
            throw NativeRuntimeError.invalidMessage
        }
        switch value.phase {
        case .preparingPlan:
            guard value.workerPending, !value.sessionRunning, value.canCancel, value.failure == nil else {
                throw NativeRuntimeError.invalidMessage
            }
        case .validating, .preparingSource, .installing, .finalizing:
            guard !value.workerPending, value.sessionRunning, value.failure == nil else {
                throw NativeRuntimeError.invalidMessage
            }
        case .recovering, .cancelling:
            guard !value.workerPending, value.sessionRunning, !value.canCancel, value.failure == nil else {
                throw NativeRuntimeError.invalidMessage
            }
        case .done, .cancelled:
            guard !value.workerPending, !value.sessionRunning, !value.canCancel, value.failure == nil else {
                throw NativeRuntimeError.invalidMessage
            }
        case .failed:
            guard !value.workerPending, !value.sessionRunning, !value.canCancel, value.failure != nil else {
                throw NativeRuntimeError.invalidMessage
            }
        }
    }
}
