import Foundation

struct NativeRuntimeStopTarget: Codable, Equatable, Sendable {
    let appInstanceID: String
    let runToken: String
    let processID: Int32
    let acceptedConfigurationDigest: String
}

struct NativeRuntimeControlRequest: Codable, Equatable, Sendable {
    enum Operation: String, Codable, Sendable { case stop, stopStatus }
    let schema: String
    let operation: Operation
    let requestID: String
    let library: NativeRuntimeLibraryIdentity
    let vmID: String
    let operationID: String
    let target: NativeRuntimeStopTarget
}

struct NativeRuntimeControlResponse: Codable, Equatable, Sendable {
    enum Disposition: String, Codable, Sendable { case accepted, existing, status, refused }
    let schema: String
    let scope: String
    let requestID: String
    let library: NativeRuntimeLibraryIdentity
    let vmID: String
    let appInstanceID: String
    let requestedOperationID: String
    let target: NativeRuntimeStopTarget
    let disposition: Disposition
    let observation: NativeRuntimeStopObservation?
    let refusal: NativeRuntimeStopRefusal?
}

enum NativeRuntimeStopRefusal: String, Codable, Error, Sendable {
    case ownerChanged, operationUnknown, operationConflict, ledgerFull
    case modelUnavailable, targetUnavailable, ambiguousTarget, configurationChanged
    case notOwned, unsupportedRuntime, operationUnavailable
}

enum NativeRuntimeStopFailure: String, Codable, Sendable {
    case protocolInvalid, channelFailed, cleanupUnconfirmed, deadlineExceeded
    case runnerExitedWithoutComplete, cleanupFailed
}

struct NativeRuntimeStopObservation: Codable, Equatable, Sendable {
    enum Phase: String, Codable, Sendable { case guestGrace, cancelling, awaitingRunnerExit, completed, unconfirmed }
    let operationID: String
    let phase: Phase
    let acceptedUptime: Double
    let deadlineUptime: Double
    let supervisorComplete: NativeRuntimeCleanupObservation?
    let runnerExit: NativeRuntimeExitObservation?
    let failure: NativeRuntimeStopFailure?
}

struct NativeRuntimeCleanupObservation: Codable, Equatable, Sendable {
    let operationID: String?
    let helperSpawnedCount: UInt64
    let helperReapedCount: UInt64
    let swtpmSpawnedCount: UInt64
    let swtpmReapedCount: UInt64
    let mediaLeaseDisposition: String
    let runtimeDirectoryDisposition: String
    let failureCode: String?
}
