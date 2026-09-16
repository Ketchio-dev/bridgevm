import Foundation

enum HvfOwnedRuntimeCodec {
    static let maximumPayload = 8192

    static func payload<T: Encodable>(_ value: T) throws -> Data {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
        let data = try encoder.encode(value)
        guard !data.isEmpty, data.count <= maximumPayload,
              data.allSatisfy({ $0 >= 0x20 && $0 <= 0x7e }) else {
            throw HvfOwnedRuntimeProtocolError.invalidMessage
        }
        return data
    }

    static func decode<T: Codable>(_ type: T.Type, payload data: Data) throws -> T {
        guard !data.isEmpty, data.count <= maximumPayload else {
            throw HvfOwnedRuntimeProtocolError.invalidFrame
        }
        do {
            let value = try JSONDecoder().decode(type, from: data)
            // Enforces required explicit nulls, exact keys, canonical scalars, and no duplicate keys.
            guard try payload(value) == data else { throw HvfOwnedRuntimeProtocolError.invalidMessage }
            return value
        } catch { throw HvfOwnedRuntimeProtocolError.invalidMessage }
    }

    static func frame<T: Encodable>(_ value: T) throws -> Data {
        let body = try payload(value)
        var length = UInt32(body.count).bigEndian
        var result = withUnsafeBytes(of: &length) { Data($0) }
        result.append(body)
        return result
    }

    static func takeFrame(from buffer: inout Data) throws -> Data? {
        guard buffer.count >= 4 else { return nil }
        let length = buffer.prefix(4).reduce(0) { ($0 << 8) | Int($1) }
        guard length > 0, length <= maximumPayload else { throw HvfOwnedRuntimeProtocolError.invalidFrame }
        guard buffer.count >= 4 + length else { return nil }
        let body = Data(buffer.dropFirst(4).prefix(length))
        buffer.removeFirst(4 + length)
        return body
    }

    static func canonicalUUID(_ value: String) -> Bool {
        UUID(uuidString: value)?.uuidString.lowercased() == value
    }

    static func hex(_ value: String, bytes: Int) -> Bool {
        value.utf8.count == bytes * 2 && value.utf8.allSatisfy { (48...57).contains($0) || (97...102).contains($0) }
    }
}
