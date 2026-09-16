import Foundation

enum NativeRuntimeCodec {
    static let requestSchema = "bridgevm.app-runtime-request.v1"
    static let responseSchema = "bridgevm.app-runtime.v1"
    static let scope = "app-observation"
    static let maximumSessions = 32
    static let maximumRequestBytes = 8_192
    static let maximumResponseBytes = 65_536

    static func encode<T: Encodable>(_ value: T) throws -> Data {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
        return try encoder.encode(value)
    }

    static func decode<T: Codable>(_ type: T.Type, from data: Data, limit: Int) throws -> T {
        guard !data.isEmpty, data.count <= limit else { throw NativeRuntimeError.invalidMessage }
        do {
            let value = try JSONDecoder().decode(type, from: data)
            guard try encode(value) == data else { throw NativeRuntimeError.invalidMessage }
            return value
        } catch { throw NativeRuntimeError.invalidMessage }
    }

    static func validate(_ request: NativeRuntimeRequest) throws {
        let saved = request.savedConfiguration
        guard request.schema == requestSchema, request.operation == "status",
              validUUID(request.requestID), !request.vmID.isEmpty,
              request.vmID.utf8.count <= 1_024, !request.vmID.contains("/"),
              request.vmID != ".", request.vmID != "..",
              request.library.canonicalPath.hasPrefix("/"),
              request.library.inode != 0,
              (saved.state == .present ? validDigest(saved.digest) : saved.digest == nil)
        else { throw NativeRuntimeError.invalidMessage }
    }

    static func validate(_ response: NativeRuntimeResponse, for request: NativeRuntimeRequest) throws {
        guard response.schema == responseSchema, response.scope == scope,
              response.requestID == request.requestID, response.library == request.library,
              response.vmID == request.vmID, validUUID(response.appInstanceID),
              response.observedAt.isFinite, response.observedAt > 0,
              response.sessions.count <= maximumSessions else { throw NativeRuntimeError.invalidMessage }
        for session in response.sessions { try validate(session, saved: request.savedConfiguration) }
    }

    static func validUUID(_ value: String) -> Bool { UUID(uuidString: value)?.uuidString == value }
    static func validDigest(_ value: String?) -> Bool {
        guard let value else { return false }
        return value.utf8.count == 64 && value.utf8.allSatisfy { (48...57).contains($0) || (97...102).contains($0) }
    }
}
