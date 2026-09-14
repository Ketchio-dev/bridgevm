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
        let process = Process()
        let output = Pipe()
        process.executableURL = executable
        process.arguments = arguments
        process.environment = ["PATH": "/usr/bin:/bin:/usr/sbin:/sbin", "LANG": "C"]
        process.standardInput = FileHandle.nullDevice
        process.standardOutput = output
        process.standardError = output
        try process.run()
        // Drain while the child runs; even an error must not fill its pipe.
        let bytes = output.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        guard process.terminationReason == .exit, process.terminationStatus == 0 else {
            let message = String(decoding: bytes, as: UTF8.self)
                .trimmingCharacters(in: .whitespacesAndNewlines)
            throw failure(message.isEmpty ? "디스크와 부팅 설정을 가져오지 못했습니다." : message)
        }
    }

    private static func failure(_ message: String) -> NSError {
        NSError(domain: "BridgeVM.MediaImport", code: 1,
                userInfo: [NSLocalizedDescriptionKey: message])
    }
}
