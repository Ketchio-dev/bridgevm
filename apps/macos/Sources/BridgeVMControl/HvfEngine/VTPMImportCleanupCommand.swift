import Foundation

enum VTPMImportCleanupCommand {
    static func run(arguments: [String]) -> Int32 {
        do {
            let values = try parse(arguments)
            let stableID = try require("--stable-vm-id", values)
            let state = try absoluteURL(require("--state-dir", values), directory: true)
            let package = try absoluteURL(require("--package", values))
            let codeURL = try absoluteURL(require("--recovery-code-file", values))
            try boundedRegular(package, maximum: 1_048_576); try boundedRegular(codeURL, maximum: 4_096, ownerOnly: true)
            let code = try String(contentsOf: codeURL, encoding: .utf8)
                .trimmingCharacters(in: .whitespacesAndNewlines)
            try VTPMIdentityLifecycle(keyStore: KeychainVTPMStateKeyStore()).removeImportedRecovery(
                destinationStableVMID: stableID, stateDirectory: state,
                packageURL: package, recoveryCode: code)
            print("forgot_imported_key=\(stableID)")
            return 0
        } catch {
            FileHandle.standardError.write(Data("ERROR: \(error.localizedDescription)\n".utf8)); return 1
        }
    }

    private static func parse(_ arguments: [String]) throws -> [String: String] {
        let allowed = Set(["--stable-vm-id", "--state-dir", "--package", "--recovery-code-file"])
        guard arguments.count == 8 else { throw VTPMLifecycleCommand.CommandError.usage("forget-import requires four exact options") }
        var values: [String: String] = [:]
        for index in stride(from: 0, to: arguments.count, by: 2) {
            let option = arguments[index]
            guard allowed.contains(option), values[option] == nil else {
                throw VTPMLifecycleCommand.CommandError.usage("unknown or duplicate forget-import option")
            }
            values[option] = arguments[index + 1]
        }
        return values
    }
    private static func require(_ option: String, _ values: [String: String]) throws -> String {
        guard let value = values[option], !value.isEmpty else {
            throw VTPMLifecycleCommand.CommandError.usage("missing required option: \(option)")
        }
        return value
    }

    private static func absoluteURL(_ path: String, directory: Bool = false) throws -> URL {
        guard path.hasPrefix("/") else { throw VTPMLifecycleCommand.CommandError.invalidPath(path) }
        return URL(fileURLWithPath: path, isDirectory: directory).standardizedFileURL
    }

    private static func boundedRegular(_ url: URL, maximum: Int, ownerOnly: Bool = false) throws {
        let values = try url.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey, .fileSizeKey])
        let mode = (try FileManager.default.attributesOfItem(atPath: url.path)[.posixPermissions] as? NSNumber)?.intValue
        guard values.isRegularFile == true, values.isSymbolicLink != true,
              let size = values.fileSize, size > 0, size <= maximum, !ownerOnly || mode.map({ $0 & 0o077 == 0 }) == true else {
            throw VTPMLifecycleCommand.CommandError.missingInput(url.path)
        }
    }
}
