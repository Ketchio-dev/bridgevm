import CryptoKit
import Foundation

/// What the probe prints for a CLIPGET reply instead of the guest's text
/// (follow-up of 2026-10-23): the text's UTF-8 byte length and the first 16
/// hex digits of its SHA-256.
enum T17ClipboardFingerprint {
    static func fields(of text: String) -> String {
        let bytes = Data(text.utf8)
        let digest = SHA256.hash(data: bytes).map { String(format: "%02x", $0) }.joined()
        return "bytes=\(bytes.count) sha256=\(digest.prefix(16))"
    }
}
