import Foundation

extension NativeRuntimeStartObservation {
    init(_ value: HvfOwnedStartObservation) throws {
        guard let phase = Phase(rawValue: value.phase.rawValue) else { throw NativeRuntimeError.invalidMessage }
        operationID = value.operationID.uuidString
        expectedSavedConfigurationDigest = value.expectedSavedConfigurationDigest
        self.phase = phase
        acceptedUptime = value.acceptedUptime; deadlineUptime = value.deadlineUptime
        target = value.target.map { .init(token: $0.token.uuidString, processID: $0.processID) }
        proof = value.proof.map { .init(target: .init(token: $0.target.token.uuidString, processID: $0.target.processID),
            manifestSHA256: $0.manifestSHA256, helperPID: $0.helperPID, helperGeneration: $0.helperGeneration) }
        if let reason = value.failure {
            guard let mapped = NativeRuntimeStartFailure(rawValue: reason.rawValue) else { throw NativeRuntimeError.invalidMessage }
            failure = mapped
        } else { failure = nil }
        workerPending = value.workerPending; mayHaveOwnedWork = value.mayHaveOwnedWork
        try NativeRuntimeStartCodec.validate(self)
    }
}
