import Foundation

enum HvfWindowsCatalogVerifier {
    static let bundledRelativePath = "helpers/bridgevm-catalog-verify"

    static func candidates(repoRoot: URL) -> [String] {
        var paths = [repoRoot.appendingPathComponent(bundledRelativePath).path]
        #if DEBUG
        paths.append(repoRoot.appendingPathComponent(
            "target/tools/bridgevm-catalog-verify").path)
        #endif
        return paths
    }

    static func resolve(
        repoRoot: URL, fileManager: FileManager = .default
    ) -> String? {
        candidates(repoRoot: repoRoot).first { path in
            guard fileManager.isExecutableFile(atPath: path),
                  let values = try? URL(fileURLWithPath: path)
                    .resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey]) else {
                return false
            }
            return values.isRegularFile == true && values.isSymbolicLink != true
        }
    }
}
