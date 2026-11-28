import Foundation

struct NativeRuntimeLibraryIdentity: Codable, Equatable, Sendable {
    let canonicalPath: String
    let device: UInt64
    let inode: UInt64
    let uid: UInt32
}

struct NativeRuntimeSavedConfiguration: Codable, Equatable, Sendable {
    enum State: String, Codable, Sendable { case present, missing, unreadable }
    let state: State
    let digest: String?
}

struct NativeRuntimeRequest: Codable, Equatable, Sendable {
    let schema: String
    let operation: String
    let requestID: String
    let library: NativeRuntimeLibraryIdentity
    let vmID: String
    let savedConfiguration: NativeRuntimeSavedConfiguration
}

struct NativeRuntimeResponse: Codable, Equatable, Sendable {
    let schema: String
    let requestID: String
    let library: NativeRuntimeLibraryIdentity
    let vmID: String
    let appInstanceID: String
    let observedAt: Double
    let scope: String
    let sessions: [NativeRuntimeSessionObservation]
}

enum NativeRuntimeError: String, Error, LocalizedError, Sendable {
    case ownerUnavailable, ownerBusy, invalidEndpoint, libraryChanged, peerRejected
    case timedOut, invalidMessage, snapshotUnavailable, transportFailure
    /// The startup screen shows this sentence; the CLI's --json output and the launch log keep `rawValue`.
    var errorDescription: String? {
        switch self {
        case .ownerBusy: return "BridgeVM is already open with this VM library. Switch to that BridgeVM window."
        case .libraryChanged: return "The VM library folder was moved or replaced. Quit and reopen BridgeVM."
        case .timedOut: return "BridgeVM didn’t answer in time. Try again."
        case .ownerUnavailable, .invalidEndpoint, .peerRejected, .invalidMessage, .snapshotUnavailable, .transportFailure:
            return "BridgeVM couldn’t open the VM library. Quit and reopen BridgeVM."
        }
    }
}
