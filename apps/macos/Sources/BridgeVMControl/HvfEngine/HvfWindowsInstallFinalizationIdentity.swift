import Foundation

enum HvfWindowsInstallFinalizationIdentity {
    struct Sealed: Equatable {
        var bytes: UInt64
        var sha256: String
    }

    static func seal(_ url: URL) throws -> Sealed {
        let bytes = try HvfWindowsInstallDurability.fileSize(url)
        guard let sha256 = HvfWindowsInstallCacheIdentity.sha256File(url.path),
              sha256.count == 64,
              try HvfWindowsInstallDurability.fileSize(url) == bytes else {
            throw HvfWindowsInstallFinalizationError.invalidState(
                "The installation file changed while it was being verified: \(url.path)")
        }
        return Sealed(bytes: bytes, sha256: sha256)
    }

    static func verify(_ url: URL, bytes: UInt64, sha256: String) throws {
        guard sha256.count == 64,
              try seal(url) == Sealed(bytes: bytes, sha256: sha256) else {
            throw HvfWindowsInstallFinalizationError.invalidState(
                "The installation file’s SHA-256 digest changed: \(url.path)")
        }
    }
}
