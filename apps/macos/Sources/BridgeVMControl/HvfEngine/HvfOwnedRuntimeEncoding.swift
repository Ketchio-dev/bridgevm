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
    /// Writes `keyHex: null` explicitly, matching the shared golden HELLO payload.
    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: OwnedRuntimeKey.self)
        try c.put(schemaVersion, "schemaVersion"); try c.put(kind, "kind")
        try c.put(runToken, "runToken"); try c.put(sequence, "sequence")
        try c.put(manifestSHA256, "manifestSHA256"); try c.put(keyHex, "keyHex")
    }
}
