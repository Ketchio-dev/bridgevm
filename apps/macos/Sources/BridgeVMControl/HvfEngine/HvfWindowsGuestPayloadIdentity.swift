import CryptoKit
import Foundation

enum HvfWindowsGuestPayloadIdentity {
    struct Inspection {
        let digest: String
        let error: String?
    }

    static func inspect(payloadDirectory: String?, manifestPath: String?) -> Inspection {
        guard let payloadDirectory, let manifestPath,
              !payloadDirectory.isEmpty, !manifestPath.isEmpty else {
            return Inspection(digest: "absent", error: "서명된 ARM64 저장장치·직렬·네트워크 드라이버 묶음과 manifest를 선택해야 합니다.")
        }
        let rootInput = URL(fileURLWithPath: payloadDirectory)
        let manifestInput = URL(fileURLWithPath: manifestPath)
        guard rootInput.path.hasPrefix("/"), manifestInput.path.hasPrefix("/") else {
            return Inspection(digest: "invalid", error: "게스트 드라이버 경로는 절대 경로여야 합니다.")
        }
        guard regularDirectory(rootInput), regularFile(manifestInput) else {
            return Inspection(digest: "invalid", error: "게스트 드라이버 폴더 또는 manifest를 읽을 수 없거나 심볼릭 링크입니다.")
        }
        let root = rootInput.resolvingSymlinksInPath().standardizedFileURL
        let manifest = manifestInput.resolvingSymlinksInPath().standardizedFileURL
        guard !VMLibrary.isSameOrDescendant(manifest, of: root) else {
            return Inspection(digest: "invalid", error: "게스트 payload manifest는 payload 폴더 밖에 있어야 합니다.")
        }
        guard let manifestDigest = HvfWindowsInstallCacheIdentity.sha256File(manifest.path) else {
            return Inspection(digest: "invalid", error: "게스트 payload manifest의 SHA-256을 계산할 수 없습니다.")
        }

        let files: [HvfWindowsGuestPayloadTree.FileEntry]
        do { files = try HvfWindowsGuestPayloadTree.scan(root) }
        catch {
            let detail = (error as? HvfWindowsGuestPayloadTree.ScanFailure)?.errorDescription
            return Inspection(digest: "invalid", error: detail ?? "게스트 payload를 끝까지 읽을 수 없습니다.")
        }
        guard !files.isEmpty else {
            return Inspection(digest: "invalid", error: "게스트 payload 폴더가 비어 있습니다.")
        }
        var hasher = SHA256()
        hasher.update(data: Data("bridgevm-windows-guest-payload-identity-v1\nmanifest\t\(manifestDigest)\n".utf8))
        for file in files.sorted(by: { $0.relativePath < $1.relativePath }) {
            guard let digest = HvfWindowsInstallCacheIdentity.sha256File(file.url.path) else {
                return Inspection(digest: "invalid", error: "게스트 payload 파일의 SHA-256을 계산할 수 없습니다: \(file.relativePath)")
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
