import CryptoKit
import Foundation

enum A9ImportTreeDigest {
    static func compute(_ root: URL, fileManager: FileManager = .default) throws -> String {
        let rootValues = try root.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey])
        guard rootValues.isDirectory == true, rootValues.isSymbolicLink != true,
              let entries = fileManager.enumerator(at: root, includingPropertiesForKeys:
                [.isRegularFileKey, .isDirectoryKey, .isSymbolicLinkKey]) else {
            throw T17Blocker(code: "import-media-invalid", detail: "vTPM tree is missing or unsafe")
        }
        var records: [String] = []
        var count = 0
        let canonicalRoot = root.resolvingSymlinksInPath().standardizedFileURL.path
        for case let item as URL in entries {
            count += 1
            guard count <= 1_024 else {
                throw T17Blocker(code: "import-media-invalid", detail: "vTPM tree exceeds the entry bound")
            }
            let values = try item.resourceValues(forKeys: [.isRegularFileKey, .isDirectoryKey, .isSymbolicLinkKey])
            guard values.isSymbolicLink != true else {
                throw T17Blocker(code: "import-media-invalid", detail: "vTPM tree contains a symlink")
            }
            let canonicalItem = item.resolvingSymlinksInPath().standardizedFileURL.path
            guard canonicalItem.hasPrefix(canonicalRoot + "/") else {
                throw T17Blocker(code: "import-media-invalid", detail: "vTPM entry escapes its source tree")
            }
            let relative = String(canonicalItem.dropFirst(canonicalRoot.count + 1))
            if relative == ".lock" { continue }
            if values.isDirectory == true { records.append("D\t\(relative)\n") }
            else if values.isRegularFile == true {
                records.append("F\t\(relative)\t\(try T17Evidence.sha256(item))\n")
            } else {
                throw T17Blocker(code: "import-media-invalid", detail: "vTPM tree contains a non-regular entry")
            }
        }
        return SHA256.hash(data: Data(records.sorted().joined().utf8))
            .map { String(format: "%02x", $0) }.joined()
    }
}
