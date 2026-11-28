import Foundation

struct NativeRuntimeStopTarget: Codable, Equatable, Sendable {
    let appInstanceID: String
    let runToken: String
    let processID: Int32
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
    /// One plain sentence for the user when the stop was refused.
    let reason: String?
}

/// The CLI only tells these apart: a restarted app, a status query for an unknown operation, and
/// every other refusal, whose `reason` sentence is printed as is.
enum NativeRuntimeStopRefusal: String, Codable, Sendable {
    case ownerChanged, operationUnknown, refused
}

/// A stop the app or the CLI will not do, with the sentence the CLI prints.
struct NativeRuntimeStopRefused: Error, Equatable, Sendable {
    let reason: String
    static let ownerChanged = Self(reason: "The BridgeVM app restarted since this VM started; run the command again.")
    static let operationUnknown = Self(reason: "The app has no record of this stop; run the command again.")
    static let operationConflict = Self(reason: "This stop operation belongs to another run; run the command again.")
    static let operationUnavailable = Self(reason: "The app is still accepting this stop; run the command again.")
    static let modelUnavailable = Self(reason: "The app's VM library is not open yet; open the library window and try again.")
    static let targetUnavailable = Self(reason: "This VM is not running in the app; check `bridgevm app status`.")
    static let ambiguousTarget = Self(reason: "The app keeps more than one run of this VM; shut it down from the app's VM window.")
    static let unsupportedRuntime = Self(reason: "The app only observes this VM and cannot stop it; shut it down from the app's VM window.")
    static let stopInProgress = Self(reason: "Another stop of this VM is in progress; wait for it to finish.")
    static let notOwned = Self(reason: "The app no longer owns this run; check `bridgevm app status`.")
}

/// The one stop-failure type, shared by the run controller and the wire.
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
