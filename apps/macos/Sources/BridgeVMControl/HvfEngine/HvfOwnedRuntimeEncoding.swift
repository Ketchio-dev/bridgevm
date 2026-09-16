import Foundation

private struct OwnedRuntimeKey: CodingKey {
    let stringValue: String
    var intValue: Int? { nil }
    init(_ value: String) { stringValue = value }
    init?(stringValue: String) { self.init(stringValue) }
    init?(intValue: Int) { return nil }
}

private extension KeyedEncodingContainer where Key == OwnedRuntimeKey {
    mutating func put<T: Encodable>(_ value: T, _ key: String) throws {
        try encode(value, forKey: OwnedRuntimeKey(key))
    }
}

extension HvfOwnedRuntimeHello {
    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: OwnedRuntimeKey.self)
        try c.put(schemaVersion, "schemaVersion"); try c.put(kind, "kind")
        try c.put(runToken, "runToken"); try c.put(sequence, "sequence")
        try c.put(manifestSHA256, "manifestSHA256"); try c.put(keyHex, "keyHex")
    }
}

extension HvfOwnedRuntimeEvent {
    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: OwnedRuntimeKey.self)
        try c.put(schemaVersion, "schemaVersion"); try c.put(kind, "kind")
        try c.put(runToken, "runToken"); try c.put(runnerPID, "runnerPID")
        try c.put(sequence, "sequence"); try c.put(ready, "ready")
        try c.put(stopAck, "stopAck"); try c.put(child, "child")
        try c.put(unconfirmed, "unconfirmed"); try c.put(complete, "complete")
    }
}

extension HvfOwnedRuntimeChild {
    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: OwnedRuntimeKey.self)
        try c.put(role, "role"); try c.put(pid, "pid"); try c.put(generation, "generation")
        try c.put(reason, "reason"); try c.put(status, "status")
    }
}

extension HvfOwnedRuntimeUnconfirmed {
    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: OwnedRuntimeKey.self)
        try c.put(role, "role"); try c.put(pid, "pid"); try c.put(generation, "generation")
        try c.put(reason, "reason"); try c.put(mediaLeaseDisposition, "mediaLeaseDisposition")
    }
}

extension HvfOwnedRuntimeRoleSummary {
    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: OwnedRuntimeKey.self)
        try c.put(spawnedCount, "spawnedCount"); try c.put(reapedCount, "reapedCount")
        try c.put(last, "last")
    }
}

extension HvfOwnedRuntimeComplete {
    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: OwnedRuntimeKey.self)
        try c.put(operationID, "operationID"); try c.put(cause, "cause")
        try c.put(outcome, "outcome"); try c.put(failureCode, "failureCode")
        try c.put(helper, "helper"); try c.put(swtpm, "swtpm")
        try c.put(mediaLeaseDisposition, "mediaLeaseDisposition")
        try c.put(runtimeDirectoryDisposition, "runtimeDirectoryDisposition")
    }
}
