import CryptoKit
import Foundation

enum HvfWindowsInstallCacheIdentity {
    private static let recipeVersion = "bridgevm-winpe-source-v4"
    private static let recipePaths = [
        "scripts/build-hvf-windows-scripted-source.sh",
        "scripts/stage-hvf-windows-guest-payload.sh",
        "scripts/hvf-disk-image-utils.sh",
        "scripts/win-assets/winpeshl.ini",
        "scripts/win-assets/bvinstall.cmd",
        "scripts/win-assets/bvdiskpart.txt",
        "scripts/win-assets/unattend.xml",
        "scripts/win-assets/bvagent.ps1",
        "scripts/win-assets/bvagent-firstboot.ps1",
    ]

    static func key(
        isoSHA256: String,
        guestPayloadIdentity: String = "absent",
        unattendedIdentity: String = "default",
        repoRoot: URL
    ) -> String {
        var hasher = SHA256()
        hasher.update(data: Data(
            (recipeVersion + "\n" + isoSHA256 + "\n" + guestPayloadIdentity
                + "\n" + unattendedIdentity + "\n").utf8))
        for relative in recipePaths {
            hasher.update(data: Data((relative + "\n").utf8))
            update(&hasher, withFile: repoRoot.appendingPathComponent(relative).path)
        }
        hasher.update(data: Data("wimlib\n".utf8))
        update(&hasher, withFile: HvfWindowsWimlib.resolve(repoRoot: repoRoot))
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
