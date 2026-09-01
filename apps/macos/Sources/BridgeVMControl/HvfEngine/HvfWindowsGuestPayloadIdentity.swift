import CryptoKit
import Foundation

enum HvfWindowsGuestPayloadIdentity {
    private static let maximumFiles = 4096
    private static let maximumBytes: UInt64 = 1024 * 1024 * 1024

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

        let keys: [URLResourceKey] = [.isDirectoryKey, .isRegularFileKey, .isSymbolicLinkKey, .fileSizeKey]
        var enumerationFailed = false
        guard let enumerator = FileManager.default.enumerator(
            at: root, includingPropertiesForKeys: keys,
            options: [], errorHandler: { _, _ in enumerationFailed = true; return false }
        ) else {
            return Inspection(digest: "invalid", error: "게스트 payload 폴더를 열거할 수 없습니다.")
        }
        var files: [(String, URL, UInt64)] = []
        var totalBytes: UInt64 = 0
        for case let url as URL in enumerator {
            guard let values = try? url.resourceValues(forKeys: Set(keys)),
                  values.isSymbolicLink != true else {
                return Inspection(digest: "invalid", error: "게스트 payload에는 심볼릭 링크를 둘 수 없습니다.")
            }
            if values.isDirectory == true { continue }
            guard values.isRegularFile == true, let size = values.fileSize, size >= 0 else {
                return Inspection(digest: "invalid", error: "게스트 payload에는 일반 파일만 둘 수 있습니다.")
            }
            let relative = String(url.path.dropFirst(root.path.count + 1))
            guard !relative.isEmpty, !relative.contains("\n"), !relative.contains("\r") else {
                return Inspection(digest: "invalid", error: "게스트 payload 파일 이름이 안전하지 않습니다.")
            }
            let addition = totalBytes.addingReportingOverflow(UInt64(size))
            guard !addition.overflow else {
                return Inspection(digest: "invalid", error: "게스트 payload 크기를 안전하게 계산할 수 없습니다.")
            }
            totalBytes = addition.partialValue
            files.append((relative, url, UInt64(size)))
            guard files.count <= maximumFiles, totalBytes <= maximumBytes else {
                return Inspection(digest: "invalid", error: "게스트 payload는 4096개 파일과 1 GiB 한도를 넘을 수 없습니다.")
            }
        }
        guard !enumerationFailed else {
            return Inspection(digest: "invalid", error: "게스트 payload를 끝까지 읽을 수 없습니다.")
        }
        guard !files.isEmpty else {
            return Inspection(digest: "invalid", error: "게스트 payload 폴더가 비어 있습니다.")
        }
        var hasher = SHA256()
        hasher.update(data: Data("bridgevm-windows-guest-payload-identity-v1\nmanifest\t\(manifestDigest)\n".utf8))
        for (relative, url, size) in files.sorted(by: { $0.0 < $1.0 }) {
            guard let digest = HvfWindowsInstallCacheIdentity.sha256File(url.path) else {
                return Inspection(digest: "invalid", error: "게스트 payload 파일의 SHA-256을 계산할 수 없습니다: \(relative)")
            }
            hasher.update(data: Data("file\t\(relative)\t\(size)\t\(digest)\n".utf8))
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
