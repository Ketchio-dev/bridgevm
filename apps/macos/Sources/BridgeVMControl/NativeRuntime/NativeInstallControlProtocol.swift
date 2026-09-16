import Foundation

struct NativeInstallControlRequest: Codable, Equatable, Sendable {
    enum Operation: String, Codable, Sendable { case install, installStatus, installCancel }
    let schema: String
    let operation: Operation
    let requestID: String
    let library: NativeRuntimeLibraryIdentity
    let vmID: String
    let appInstanceID: String
    let expectedSavedConfigurationDigest: String
    let operationID: String?
}

struct NativeInstallControlResponse: Codable, Equatable, Sendable {
    enum Disposition: String, Codable, Sendable { case accepted, existing, status, cancelAccepted, refused }
    let schema: String
    let scope: String
    let requestID: String
    let library: NativeRuntimeLibraryIdentity
    let vmID: String
    let appInstanceID: String
    let expectedSavedConfigurationDigest: String
    let requestedOperationID: String?
    let disposition: Disposition
    let observation: NativeInstallObservation?
    let refusal: NativeInstallControlRefusal?
}

enum NativeInstallControlRefusal: String, Codable, Error, Sendable {
    case ownerChanged, operationConflict, ledgerFull, modelUnavailable
    case targetUnavailable, ambiguousTarget, configurationChanged, unsupportedTarget
    case busy, admissionRefused, cancellationUnavailable
}

struct NativeInstallObservation: Codable, Equatable, Sendable {
    enum Phase: String, Codable, Sendable {
        case preparingPlan, validating, preparingSource, installing, finalizing
        case recovering, cancelling, done, failed, cancelled
    }
    let operationID: String
    let expectedSavedConfigurationDigest: String
    let phase: Phase
    let acceptedUptime: Double
    let workerPending: Bool
    let sessionRunning: Bool
    let canCancel: Bool
    let logTail: [String]
    let failure: String?
}
