import Foundation

/// CLI start names only the VM; the app starts that VM's current saved configuration.
struct NativeRuntimeStartRequest: Codable, Equatable, Sendable {
    enum Operation: String, Codable, Sendable { case start, startStatus }
    let schema: String
    let operation: Operation
    let requestID: String
    let library: NativeRuntimeLibraryIdentity
    let vmID: String
    let appInstanceID: String
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
    let requestedOperationID: String
    let disposition: Disposition
    let observation: NativeRuntimeStartObservation?
    let refusal: NativeRuntimeStartRefusal?
    /// One plain sentence for the user when the start was refused.
    let reason: String?
}

/// The CLI only tells these apart: a restarted app, a status query for an unknown operation, and
/// every other refusal, whose `reason` sentence is printed as is.
enum NativeRuntimeStartRefusal: String, Codable, Error, Sendable {
    case ownerChanged, operationUnknown, refused
}

struct NativeRuntimeStartObservation: Codable, Equatable, Sendable {
    enum Phase: String, Codable, Sendable { case preparing, awaitingStartup, started, failed }
    let operationID: String
    let phase: Phase
    let target: NativeRuntimeProcessObservation?
    let proof: NativeRuntimeStartProof?
    /// The start path's own error sentence (the text the app shows), when the start failed.
    let failure: String?
}

struct NativeRuntimeStartProof: Codable, Equatable, Sendable {
    let target: NativeRuntimeProcessObservation
    let manifestSHA256: String
    let helperPID: Int32
}
