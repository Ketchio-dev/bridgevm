import Foundation

/// The helper's result path must name an absent file in a regular directory, so a lane result is never
/// written over another or through a link.
enum ProductE2EResultOutput {
    static func absent(_ path: String) throws -> URL {
        let resultURL = URL(fileURLWithPath: path).standardizedFileURL
        guard !FileManager.default.fileExists(atPath: resultURL.path),
              (try resultURL.deletingLastPathComponent().resourceValues(
                forKeys: [.isDirectoryKey, .isSymbolicLinkKey])).isDirectory == true else {
            throw T17Blocker(code: "invalid-request", detail: "result output is not an absent file in a regular directory")
        }
        return resultURL
    }
}

/// The value after `--request` or `--result`: present, and given once.
enum ProductE2EOptionValue {
    static func take(_ arguments: [String], at index: Int, current: String?) throws -> String {
        guard index + 1 < arguments.count else { throw T17Blocker(code: "invalid-request", detail: "missing option value") }
        guard current == nil else { throw T17Blocker(code: "invalid-request", detail: "duplicate \(arguments[index].dropFirst(2))") }
        return arguments[index + 1]
    }
}
