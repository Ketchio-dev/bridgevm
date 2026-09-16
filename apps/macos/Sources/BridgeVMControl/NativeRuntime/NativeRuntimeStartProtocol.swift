import Foundation

struct NativeRuntimeStartRequest: Codable, Equatable, Sendable {
    enum Operation: String, Codable, Sendable { case start, startStatus }
    let schema: String
    let operation: Operation
    let requestID: String
    let library: NativeRuntimeLibraryIdentity
    let vmID: String
    let appInstanceID: String
    let expectedSavedConfigurationDigest: String
    let operationID: String
}

struct NativeRuntimeStartResponse: Codable, Equatable, Sendable {
    enum Disposition: String, Codable, Sendable { case accepted, existing, status, refused }
    let schema: String
    let scope: String
    let requestID: String
    let library: NativeRuntimeLibraryIdentity
    let vmID: String
    let appInstanceID: String
    let expectedSavedConfigurationDigest: String
    let requestedOperationID: String
    let disposition: Disposition
    let observation: NativeRuntimeStartObservation?
    let refusal: NativeRuntimeStartRefusal?
}

enum NativeRuntimeStartRefusal: String, Codable, Error, Sendable {
    case ownerChanged, operationUnknown, operationConflict, operationUnavailable, ledgerFull
    case modelUnavailable, targetUnavailable, ambiguousTarget, configurationChanged, unsupportedRuntime
    case busy, runtimeActive, mutationActive, configurationMismatch, admissionRefused
}

enum NativeRuntimeStartFailure: String, Codable, Sendable {
    case readiness, preparation, keyUnavailable, configurationChanged, helperUnavailable
    case processLaunch, protocolInvalid, runnerExited, deadlineExceeded
}

struct NativeRuntimeStartObservation: Codable, Equatable, Sendable {
    enum Phase: String, Codable, Sendable { case preparing, awaitingStartup, started, failed, unconfirmed }
    let operationID: String
    let expectedSavedConfigurationDigest: String
    let phase: Phase
    let acceptedUptime: Double
    let deadlineUptime: Double
    let target: NativeRuntimeProcessObservation?
    let proof: NativeRuntimeStartProof?
    let failure: NativeRuntimeStartFailure?
    let workerPending: Bool
    let mayHaveOwnedWork: Bool
}

struct NativeRuntimeStartProof: Codable, Equatable, Sendable {
    let target: NativeRuntimeProcessObservation
    let manifestSHA256: String
    let helperPID: Int32
    let helperGeneration: UInt64
}
