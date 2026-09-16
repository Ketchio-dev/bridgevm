import Foundation

extension NativeRuntimeStartCodec {
    static func validate(_ value: NativeRuntimeStartObservation) throws {
        guard NativeRuntimeCodec.validUUID(value.operationID),
              NativeRuntimeCodec.validDigest(value.expectedSavedConfigurationDigest),
              value.acceptedUptime.isFinite, value.deadlineUptime.isFinite, value.acceptedUptime >= 0,
              abs(value.deadlineUptime - value.acceptedUptime - 30) <= 0.001
        else { throw NativeRuntimeError.invalidMessage }
        if let target = value.target {
            guard NativeRuntimeCodec.validUUID(target.token), target.processID > 0 else {
                throw NativeRuntimeError.invalidMessage
            }
        }
        if let proof = value.proof {
            guard proof.target == value.target, NativeRuntimeCodec.validDigest(proof.manifestSHA256),
                  proof.helperPID > 0, proof.helperPID != proof.target.processID,
                  proof.helperGeneration == 0 else { throw NativeRuntimeError.invalidMessage }
        }
        switch value.phase {
        case .preparing:
            guard value.target == nil, value.proof == nil, value.failure == nil, value.workerPending else {
                throw NativeRuntimeError.invalidMessage
            }
        case .awaitingStartup:
            guard value.target != nil, value.proof == nil, value.failure == nil,
                  !value.workerPending, value.mayHaveOwnedWork else { throw NativeRuntimeError.invalidMessage }
        case .started:
            // This is historical startup proof; later stop/exit does not erase it.
            guard value.target != nil, value.proof != nil, value.failure == nil, !value.workerPending else {
                throw NativeRuntimeError.invalidMessage
            }
        case .failed:
            guard value.proof == nil, value.failure != nil, !value.workerPending, !value.mayHaveOwnedWork else {
                throw NativeRuntimeError.invalidMessage
            }
        case .unconfirmed:
            guard value.proof == nil, value.failure != nil, value.workerPending || value.mayHaveOwnedWork else {
                throw NativeRuntimeError.invalidMessage
            }
        }
    }
}
