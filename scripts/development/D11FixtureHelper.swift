import Foundation
import Darwin

// Development executable only. Reuse the existing CSPRNG writer unchanged;
// its diagnostic error type has no product or UI dependencies.
struct T17Blocker: Error { let code: String; let detail: String }

extension Bundle {
    static var module: Bundle {
        get {
            let executable = URL(fileURLWithPath: CommandLine.arguments[0]).standardizedFileURL
            guard let bundle = Bundle(url: executable.deletingLastPathComponent()
                .appendingPathComponent("D11Resources.bundle")) else {
                fatalError("D11 sealed resources unavailable")
            }
            return bundle
        }
    }
}

@main
struct D11FixtureHelper {
    static func main() {
        umask(0o077)
        do {
            let args = Array(CommandLine.arguments.dropFirst())
            guard args.count == 3 else { throw T17Blocker(code: "arguments", detail: "invalid operation") }
            let output = URL(fileURLWithPath: args[1])
            guard args[1].hasPrefix("/"), !FileManager.default.fileExists(atPath: args[1]) else {
                throw T17Blocker(code: "output", detail: "new absolute output required")
            }
            switch args[0] {
            case "unattend":
                try T17PrivateUnattend.write(to: output, nonce: args[2], fileManager: .default)
            case "initial-vars":
                guard args[2] == "DEVELOPMENT_ONLY" else {
                    throw T17Blocker(code: "classification", detail: "development only")
                }
                let seed = try HvfWindowsBootSeed.bundledSeed()
                guard seed.count == 64 * 1024 * 1024 else { throw HvfWindowsBootSeed.SeedError.varstoreUnreadable }
                try seed.write(to: output, options: [.withoutOverwriting])
            case "seed-vars":
                // A distinct final vars file keeps installation vars retained.
                let seed = try HvfWindowsBootSeed.bundledSeed()
                guard seed.count == 64 * 1024 * 1024 else { throw HvfWindowsBootSeed.SeedError.varstoreUnreadable }
                try seed.write(to: output, options: [.withoutOverwriting])
                _ = try HvfWindowsBootSeed.seedFile(varsPath: args[1], diskPath: args[2])
            default: throw T17Blocker(code: "operation", detail: "unknown operation")
            }
            try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: output.path)
        } catch {
            // Never interpolate errors or argument values into credential logs.
            FileHandle.standardError.write(Data("D11 helper refused\n".utf8))
            exit(1)
        }
    }
}
