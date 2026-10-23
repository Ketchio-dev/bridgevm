import Foundation

enum ProductE2EMode: Equatable {
    case isoInstall
    case installedDiskImport
}

struct ProductE2ECLI {
    let mode: ProductE2EMode
    let request: URL
    let result: URL
    /// The opt-in T19 BitLocker restart check, which only the sealed T19 manifest row turns on.
    var bitLockerRestartCheck = false

    static func parse(_ arguments: [String]) throws -> ProductE2ECLI {
        var request: String?
        var result: String?
        var mode: ProductE2EMode?
        var bitLocker = false
        var index = 0
        while index < arguments.count {
            switch arguments[index] {
            case "--windows-product-e2e", "--windows-import-product-e2e":
                guard mode == nil else { throw T17Blocker(code: "invalid-request", detail: "duplicate or mixed mode marker") }
                mode = arguments[index] == "--windows-product-e2e" ? .isoInstall : .installedDiskImport
                index += 1
            case "--bitlocker-restart-check":
                guard !bitLocker else { throw T17Blocker(code: "invalid-request", detail: "duplicate BitLocker restart check") }
                bitLocker = true
                index += 1
            case "--request", "--result":
                let isRequest = arguments[index] == "--request"
                let value = try ProductE2EOptionValue.take(arguments, at: index, current: isRequest ? request : result)
                if isRequest { request = value } else { result = value }
                index += 2
            default:
                throw T17Blocker(code: "invalid-request", detail: "unknown command option")
            }
        }
        guard let mode, let request, let result, request.hasPrefix("/"), result.hasPrefix("/") else {
            throw T17Blocker(code: "invalid-request", detail: "exact mode, request and result are required")
        }
        guard !bitLocker || mode == .installedDiskImport else {
            throw T17Blocker(code: "invalid-request", detail: "the BitLocker restart check is T19 only")
        }
        let resultURL = try ProductE2EResultOutput.absent(result)
        return ProductE2ECLI(mode: mode, request: URL(fileURLWithPath: request).standardizedFileURL, result: resultURL,
                             bitLockerRestartCheck: bitLocker)
    }
}

typealias T17CLI = ProductE2ECLI
