import Darwin
import Foundation

struct T17CLI {
    let request: URL
    let result: URL

    static func parse(_ arguments: [String]) throws -> T17CLI {
        var request: String?
        var result: String?
        var marker = false
        var index = 0
        while index < arguments.count {
            switch arguments[index] {
            case "--windows-product-e2e":
                guard !marker else { throw T17Blocker(code: "invalid-request", detail: "duplicate mode marker") }
                marker = true; index += 1
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
        guard marker, let request, let result, request.hasPrefix("/"), result.hasPrefix("/") else {
            throw T17Blocker(code: "invalid-request", detail: "exact mode, request and result are required")
        }
        let resultURL = URL(fileURLWithPath: result).standardizedFileURL
        guard !FileManager.default.fileExists(atPath: resultURL.path),
              (try resultURL.deletingLastPathComponent().resourceValues(
                forKeys: [.isDirectoryKey, .isSymbolicLinkKey])).isDirectory == true else {
            throw T17Blocker(code: "invalid-request", detail: "result output is not an absent file in a regular directory")
        }
        return T17CLI(request: URL(fileURLWithPath: request).standardizedFileURL, result: resultURL)
    }
}

@main
enum BridgeVMProductE2EMain {
    static func main() {
        do {
            let cli = try T17CLI.parse(Array(CommandLine.arguments.dropFirst()))
            let request = try T17Request.load(cli.request)
            let outcome = T17ProductRunner(request: request).run()
            let result = outcome.evidence.result(
                request: request, failureCode: outcome.failureCode,
                cleanupVerified: outcome.cleanupVerified,
                installerSourcePath: outcome.installerSourcePath,
                uiFrontendAutomated: outcome.uiFrontendAutomated
            )
            try T17ResultWriter.write(result, to: cli.result)
            exit(EXIT_SUCCESS)
        } catch let blocker as T17Blocker {
            FileHandle.standardError.write(Data("BridgeVMProductE2E BLOCKER[\(blocker.code)]: \(blocker.detail)\n".utf8))
        } catch {
            FileHandle.standardError.write(Data("BridgeVMProductE2E BLOCKER[internal-error]: request or result processing failed\n".utf8))
        }
        exit(EXIT_FAILURE)
    }
}
