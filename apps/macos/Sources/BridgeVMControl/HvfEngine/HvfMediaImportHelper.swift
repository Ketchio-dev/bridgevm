import Foundation

/// Uses the same packaged native selector as powered-off snapshots.
enum HvfMediaImportHelper {
    static var bundled: URL {
        HvfEngineSession.defaultRepoRoot()
            .appendingPathComponent("target/release/examples/snapshot_pair_cli")
    }

    static func invoke(_ executable: URL, arguments: [String]) throws {
        let canonical = executable.resolvingSymlinksInPath().standardizedFileURL
        let values = try executable.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey])
        guard executable.standardizedFileURL == canonical,
              values.isRegularFile == true, values.isSymbolicLink != true,
              FileManager.default.isExecutableFile(atPath: executable.path) else {
            throw failure("가져오기에 필요한 엔진 도구가 없거나 안전하지 않습니다.")
        }
        // The helper prints a six-line manifest; output past the shared limit
        // is not retained and fails the import instead of passing unread.
        let completion = try HvfHelperProcess.run(executable, arguments)
        guard completion.succeeded else {
            let message = completion.diagnostic
            throw failure(message.isEmpty ? "디스크와 부팅 설정을 가져오지 못했습니다." : message)
        }
    }

    private static func failure(_ message: String) -> NSError {
        NSError(domain: "BridgeVM.MediaImport", code: 1,
                userInfo: [NSLocalizedDescriptionKey: message])
    }
}
