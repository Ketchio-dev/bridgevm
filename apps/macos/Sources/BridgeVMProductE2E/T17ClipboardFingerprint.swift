import CryptoKit
import Foundation

/// The clipboard log fingerprint: byte length and the first 16 hex digits of SHA-256.
enum T17ClipboardFingerprint {
    static func fields(of text: String) -> String {
        fields(of: Data(text.utf8))
    }

    static func fields(of bytes: Data) -> String {
        let digest = SHA256.hash(data: bytes).map { String(format: "%02x", $0) }.joined()
        return "bytes=\(bytes.count) sha256=\(digest.prefix(16))"
    }
}
