import Foundation

struct HvfOwnedRuntimeReady: Codable, Equatable, Sendable {
    let manifestSHA256: String
    let generation: UInt64
    let termMillis: UInt64
    let killReapMillis: UInt64
}

struct HvfOwnedRuntimeStopAck: Codable, Equatable, Sendable {
    let operationID: String
    let disposition: String
}

struct HvfOwnedRuntimeChild: Codable, Equatable, Sendable {
    let role: String
    let pid: Int32
    let generation: UInt64?
    let reason: String?
    let status: Int32?
}

struct HvfOwnedRuntimeUnconfirmed: Codable, Equatable, Sendable {
    let role: String
    let pid: Int32
    let generation: UInt64?
    let reason: String
    let mediaLeaseDisposition: String
}

struct HvfOwnedRuntimeRoleSummary: Codable, Equatable, Sendable {
    let spawnedCount: UInt64
    let reapedCount: UInt64
    let last: HvfOwnedRuntimeChild?
}

struct HvfOwnedRuntimeComplete: Codable, Equatable, Sendable {
    let operationID: String?
    let cause: String
    let outcome: String
    let failureCode: String?
    let helper: HvfOwnedRuntimeRoleSummary
    let swtpm: HvfOwnedRuntimeRoleSummary
    let mediaLeaseDisposition: String
    let runtimeDirectoryDisposition: String
}
