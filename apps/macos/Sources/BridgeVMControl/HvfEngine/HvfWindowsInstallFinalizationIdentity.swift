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
                "transaction 산출물을 안정적으로 봉인하지 못했습니다: \(url.path)")
        }
        return Sealed(bytes: bytes, sha256: sha256)
    }

    static func verify(_ url: URL, bytes: UInt64, sha256: String) throws {
        guard sha256.count == 64,
              try seal(url) == Sealed(bytes: bytes, sha256: sha256) else {
            throw HvfWindowsInstallFinalizationError.invalidState(
                "transaction 산출물 digest가 변경되었습니다: \(url.path)")
        }
    }
}
