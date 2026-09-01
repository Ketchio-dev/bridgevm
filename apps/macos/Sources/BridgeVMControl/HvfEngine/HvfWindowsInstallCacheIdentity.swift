import CryptoKit
import Foundation

enum HvfWindowsInstallCacheIdentity {
    private static let recipeVersion = "bridgevm-winpe-source-v2"
    private static let recipePaths = [
        "scripts/build-hvf-windows-scripted-source.sh",
        "scripts/hvf-disk-image-utils.sh",
        "scripts/win-assets/winpeshl.ini",
        "scripts/win-assets/bvinstall.cmd",
        "scripts/win-assets/bvdiskpart.txt",
        "scripts/win-assets/unattend.xml",
    ]

    static func key(isoSHA256: String, repoRoot: URL) -> String {
        var hasher = SHA256()
        hasher.update(data: Data((recipeVersion + "\n" + isoSHA256 + "\n").utf8))
        for relative in recipePaths {
            hasher.update(data: Data((relative + "\n").utf8))
            update(&hasher, withFile: repoRoot.appendingPathComponent(relative).path)
        }
        let wimlib = HvfWindowsInstallPlan.wimlibCandidates.first {
            FileManager.default.isExecutableFile(atPath: $0)
        }
        hasher.update(data: Data("wimlib\n".utf8))
        update(&hasher, withFile: wimlib)
        return "win11-" + hasher.finalize().map { String(format: "%02x", $0) }.joined()
    }

    private static func update(_ hasher: inout SHA256, withFile path: String?) {
        let attributes = path.flatMap { try? FileManager.default.attributesOfItem(atPath: $0) }
        guard let path, attributes?[.type] as? FileAttributeType == .typeRegular,
              let handle = FileHandle(forReadingAtPath: path) else {
            hasher.update(data: Data("absent\n".utf8))
            return
        }
        defer { try? handle.close() }
        do {
            while true {
                let chunk = try handle.read(upToCount: 1024 * 1024) ?? Data()
                if chunk.isEmpty { break }
                hasher.update(data: chunk)
            }
        } catch {
            hasher.update(data: Data("read-error\n".utf8))
        }
    }
}
