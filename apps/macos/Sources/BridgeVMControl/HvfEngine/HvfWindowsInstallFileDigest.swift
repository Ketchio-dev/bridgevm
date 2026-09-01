import CryptoKit
import Foundation

extension HvfWindowsInstallCacheIdentity {
    static func sha256File(_ path: String) -> String? {
        let attributes = try? FileManager.default.attributesOfItem(atPath: path)
        guard attributes?[.type] as? FileAttributeType == .typeRegular,
              let handle = FileHandle(forReadingAtPath: path) else { return nil }
        defer { try? handle.close() }
        var hasher = SHA256()
        do {
            while true {
                let chunk = try handle.read(upToCount: 8 * 1024 * 1024) ?? Data()
                if chunk.isEmpty { break }
                hasher.update(data: chunk)
            }
        } catch {
            return nil
        }
        return hasher.finalize().map { String(format: "%02x", $0) }.joined()
    }
}
