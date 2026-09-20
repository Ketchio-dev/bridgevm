import Foundation
struct NativeRuntimeProcessObservation: Codable, Equatable, Sendable {
    let token: String
    let processID: Int32
}
struct NativeRuntimeExitObservation: Codable, Equatable, Sendable {
    let process: NativeRuntimeProcessObservation
    let reason: String
    let status: Int32
}

struct NativeRuntimeSessionObservation: Codable, Equatable, Sendable {
    enum Ownership: String, Codable, Sendable {
        case owned
        case attachedObservation = "attached-observation"
        case ownedExitObserved = "owned-exit-observed"
        case notObserved = "not-observed"
    }
    enum ConfigurationMatch: String, Codable, Sendable { case same, different, unknown }
    enum GraphicsMode: String, Codable, Sendable { case basic3DOff = "basic-3d-off", experimental3D = "experimental-3d", unverified }
    let ownership: Ownership
    /// Retained connection state only; nil when there is no process observation.
    let connectionState: String?
    let acceptedConfigurationDigest: String?
    let configurationMatch: ConfigurationMatch
    let ownedProcess: NativeRuntimeProcessObservation?
    let lastOwnedExit: NativeRuntimeExitObservation?
    let graphicsMode: GraphicsMode?
}
