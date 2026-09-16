#if (DEBUG && BRIDGEVM_APP_UI_HOST) || BRIDGEVM_APP_UI_DRIVER
import CryptoKit
import Foundation

enum AppUIDriverCanonicalJSON {
    static func encode<T: Encodable>(_ value: T) throws -> Data {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
        let data = try encoder.encode(value)
        guard !data.isEmpty, data.count <= AppUIDriverConstants.maximumBytes else {
            throw AppUIDriverFailure.protocolLimit
        }
        return data
    }

    static func decode<T: Codable>(_ data: Data, as type: T.Type) throws -> T {
        guard !data.isEmpty, data.count <= AppUIDriverConstants.maximumBytes else {
            throw AppUIDriverFailure.protocolLimit
        }
        let value = try JSONDecoder().decode(type, from: data)
        // Our two Swift producers write exactly this encoding. The equality also
        // refuses duplicate/unknown keys that JSONDecoder alone would discard.
        guard try encode(value) == data else { throw AppUIDriverFailure.invalidFile }
        return value
    }

    static func digest(_ data: Data) -> String {
        SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
    }
}
#endif
