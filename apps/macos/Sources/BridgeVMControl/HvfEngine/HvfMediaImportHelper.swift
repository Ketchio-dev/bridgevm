import Foundation

/// Uses the same packaged native selector as powered-off snapshots.
enum HvfMediaImportHelper {
    static var bundled: URL {
        HvfEngineSession.defaultRepoRoot()
            .appendingPathComponent("target/release/examples/snapshot_pair_cli")
    }

    /// `progress` sees the helper's `progress <percent>` lines, which then stay out of its diagnostic.
    static func invoke(_ executable: URL, arguments: [String], progress: ((Int) -> Void)? = nil) throws {
        let canonical = executable.resolvingSymlinksInPath().standardizedFileURL
        guard executable.standardizedFileURL == canonical,
              let values = try? executable.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey]),
              values.isRegularFile == true, values.isSymbolicLink != true,
              FileManager.default.isExecutableFile(atPath: executable.path) else {
            throw CocoaError(.executableNotLoadable, userInfo: [NSFilePathErrorKey: executable.path])
        }
        // The helper prints a six-line manifest; output past the shared limit
        // is not retained and fails the import instead of passing unread.
        let completion = try HvfHelperProcess.run(executable, arguments, progress: progress)
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
