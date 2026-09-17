import Foundation

enum ProductE2EMode: Equatable {
    case isoInstall
    case installedDiskImport
}

struct ProductE2ECLI {
    let mode: ProductE2EMode
    let request: URL
    let result: URL

    static func parse(_ arguments: [String]) throws -> ProductE2ECLI {
        var request: String?
        var result: String?
        var mode: ProductE2EMode?
        var index = 0
        while index < arguments.count {
            switch arguments[index] {
            case "--windows-product-e2e", "--windows-import-product-e2e":
                guard mode == nil else { throw T17Blocker(code: "invalid-request", detail: "duplicate or mixed mode marker") }
                mode = arguments[index] == "--windows-product-e2e" ? .isoInstall : .installedDiskImport
                index += 1
            case "--request", "--result":
                guard index + 1 < arguments.count else {
                    throw T17Blocker(code: "invalid-request", detail: "missing option value")
                }
                let value = arguments[index + 1]
                if arguments[index] == "--request" {
                    guard request == nil else { throw T17Blocker(code: "invalid-request", detail: "duplicate request") }
                    request = value
                } else {
                    guard result == nil else { throw T17Blocker(code: "invalid-request", detail: "duplicate result") }
                    result = value
                }
                index += 2
            default:
                throw T17Blocker(code: "invalid-request", detail: "unknown command option")
            }
        }
        guard let mode, let request, let result, request.hasPrefix("/"), result.hasPrefix("/") else {
            throw T17Blocker(code: "invalid-request", detail: "exact mode, request and result are required")
        }
        let resultURL = URL(fileURLWithPath: result).standardizedFileURL
        guard !FileManager.default.fileExists(atPath: resultURL.path),
              (try resultURL.deletingLastPathComponent().resourceValues(
                forKeys: [.isDirectoryKey, .isSymbolicLinkKey])).isDirectory == true else {
            throw T17Blocker(code: "invalid-request", detail: "result output is not an absent file in a regular directory")
        }
        return ProductE2ECLI(mode: mode, request: URL(fileURLWithPath: request).standardizedFileURL, result: resultURL)
    }
}

typealias T17CLI = ProductE2ECLI
