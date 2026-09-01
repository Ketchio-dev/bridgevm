import Foundation

enum HvfWindowsWimlib {
    static let bundledRelativePath = "helpers/wimlib-imagex"

    static func candidates(repoRoot: URL) -> [String] {
        var paths = [repoRoot.appendingPathComponent(bundledRelativePath).path]
        #if DEBUG
        // Development checkouts may use a locally installed copy. Release
        // builds compile this branch out and execute only the signed helper in
        // the app resource directory.
        paths.append(contentsOf: [
            "/opt/homebrew/bin/wimlib-imagex",
            "/usr/local/bin/wimlib-imagex",
        ])
        #endif
        return paths
    }

    static func resolve(repoRoot: URL, fileManager: FileManager = .default) -> String? {
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
