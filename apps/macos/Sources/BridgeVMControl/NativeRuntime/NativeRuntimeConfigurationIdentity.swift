import CryptoKit
import Foundation

enum NativeRuntimeConfigurationIdentity {
    static func digest(config: VMConfig) throws -> String {
        var accepted = config
        accepted.id = config.slug
        return SHA256.hash(data: try NativeRuntimeCodec.encode(accepted))
            .map { String(format: "%02x", $0) }.joined()
    }
}
