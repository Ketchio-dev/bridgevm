import CryptoKit
import Foundation

enum HvfWindowsGuestPayloadIdentity {
    struct Inspection {
        let digest: String
        let error: String?
    }

    static let notAManifest = "This file isn’t a BridgeVM driver manifest. Choose the manifest that came with the drivers, or use Download Drivers."

    /// The records stage-hvf-windows-guest-payload.sh requires first (one `schema` of
    /// bridgevm-windows-guest-payload-v1 and one `architecture` of arm64), so a plain `shasum` list
    /// is refused at Create instead of failing the setup files at Start. The script checks the rest.
    static func manifestFormatError(_ path: String?) -> String? {
        guard let path, let data = FileManager.default.contents(atPath: path), data.count <= 1 << 20,
              let text = String(data: data, encoding: .utf8) else { return notAManifest }
        let records = text.replacingOccurrences(of: "\r", with: "").split(separator: "\n")
            .map { $0.split(separator: "\t", omittingEmptySubsequences: false).map(String.init) }
        let schema = records.filter { $0.first == "schema" }, architecture = records.filter { $0.first == "architecture" }
        guard schema.count == 1, schema[0] == ["schema", "bridgevm-windows-guest-payload-v1"] else { return notAManifest }
        guard architecture.count == 1, architecture[0] == ["architecture", "arm64"] else {
            return "These drivers aren’t for Windows on Arm. Choose ARM64 drivers, or use Download Drivers."
        }
        return nil
    }

    static func inspect(payloadDirectory: String?, manifestPath: String?) -> Inspection {
        guard let payloadDirectory, let manifestPath,
              !payloadDirectory.isEmpty, !manifestPath.isEmpty else {
            return Inspection(digest: "absent", error: "Select the signed ARM64 storage, serial and network drivers and their manifest.")
        }
        let rootInput = URL(fileURLWithPath: payloadDirectory)
        let manifestInput = URL(fileURLWithPath: manifestPath)
        guard rootInput.path.hasPrefix("/"), manifestInput.path.hasPrefix("/") else {
            return Inspection(digest: "invalid", error: "Use full paths for the Windows driver folder and manifest.")
        }
        guard regularDirectory(rootInput), regularFile(manifestInput) else {
            return Inspection(digest: "invalid", error: "Choose a readable Windows driver folder and manifest. Neither can be a symbolic link.")
        }
        let root = rootInput.resolvingSymlinksInPath().standardizedFileURL
        let manifest = manifestInput.resolvingSymlinksInPath().standardizedFileURL
        guard !VMLibrary.isSameOrDescendant(manifest, of: root) else {
            return Inspection(digest: "invalid", error: "Keep the driver manifest outside the driver folder.")
        }
        guard let manifestDigest = HvfWindowsInstallCacheIdentity.sha256File(manifest.path) else {
            return Inspection(digest: "invalid", error: "BridgeVM couldn’t calculate the driver manifest’s SHA-256 checksum.")
        }

        let files: [HvfWindowsGuestPayloadTree.FileEntry]
        do { files = try HvfWindowsGuestPayloadTree.scan(root) }
        catch {
            let detail = (error as? HvfWindowsGuestPayloadTree.ScanFailure)?.errorDescription
            return Inspection(digest: "invalid", error: detail ?? "BridgeVM couldn’t read all the Windows driver files.")
        }
        guard !files.isEmpty else {
            return Inspection(digest: "invalid", error: "The Windows driver folder is empty.")
        }
        var hasher = SHA256()
        hasher.update(data: Data("bridgevm-windows-guest-payload-identity-v1\nmanifest\t\(manifestDigest)\n".utf8))
        for file in files.sorted(by: { $0.relativePath < $1.relativePath }) {
            guard let digest = HvfWindowsInstallCacheIdentity.sha256File(file.url.path) else {
                return Inspection(digest: "invalid", error: "BridgeVM couldn’t calculate the SHA-256 checksum for this driver file: \(file.relativePath)")
            }
            hasher.update(data: Data("file\t\(file.relativePath)\t\(file.size)\t\(digest)\n".utf8))
        }
        let digest = hasher.finalize().map { String(format: "%02x", $0) }.joined()
        return Inspection(digest: digest, error: nil)
    }

    private static func regularDirectory(_ url: URL) -> Bool {
        guard let values = try? url.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey]) else { return false }
        return values.isDirectory == true && values.isSymbolicLink != true
    }

    private static func regularFile(_ url: URL) -> Bool {
        guard let values = try? url.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey]) else { return false }
        return values.isRegularFile == true && values.isSymbolicLink != true
    }
}
