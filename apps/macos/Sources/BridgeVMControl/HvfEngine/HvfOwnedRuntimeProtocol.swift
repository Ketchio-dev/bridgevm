import Foundation

struct HvfOwnedRuntimeHello: Codable, Equatable, Sendable {
    let schemaVersion: Int
    let kind: String
    let runToken: String
    let sequence: UInt64
    let manifestSHA256: String
    let keyHex: String?
}

struct HvfOwnedRuntimeStop: Codable, Equatable, Sendable {
    let schemaVersion: Int
    let kind: String
    let runToken: String
    let sequence: UInt64
    let operationID: String
}

struct HvfOwnedRuntimeEvent: Codable, Equatable, Sendable {
    let schemaVersion: Int
    let kind: String
    let runToken: String
    let runnerPID: Int32
    let sequence: UInt64
    let ready: HvfOwnedRuntimeReady?
    let stopAck: HvfOwnedRuntimeStopAck?
    let child: HvfOwnedRuntimeChild?
    let unconfirmed: HvfOwnedRuntimeUnconfirmed?
    let complete: HvfOwnedRuntimeComplete?
}

enum HvfOwnedRuntimeProtocolError: String, Error, Sendable {
    case invalidFrame, invalidMessage, invalidIdentity, invalidSequence, invalidLifecycle
    case timedOut, channelClosed, writeClosed, ioFailure
}
