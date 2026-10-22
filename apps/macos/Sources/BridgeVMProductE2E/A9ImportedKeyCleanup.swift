import Foundation

/// Forgets the imported key of every registration in the lane's private library (see A9LaneRegistrations).
enum A9ImportedKeyCleanup {
    static func run(request: A9ImportRequest, fileManager: FileManager) -> Bool {
        guard let slugs = A9LaneRegistrations.slugs(request: request, fileManager: fileManager),
              let suffix = A9LaneRegistrations.stateSuffix(request: request) else { return false }
        let library = URL(fileURLWithPath: request.libraryRootPath, isDirectory: true)
        return slugs.map { forget($0, state: library.appendingPathComponent($0).appendingPathComponent(suffix), request: request) }
            .allSatisfy { $0 }
    }

    private static func forget(_ slug: String, state: URL, request: A9ImportRequest) -> Bool {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: request.appExecutablePath)
        process.arguments = [
            "--vtpm-lifecycle", "forget-import",
            "--stable-vm-id", slug,
            "--state-dir", state.path,
            "--package", request.sourceVtpmPackagePath,
            "--recovery-code-file", request.sourceVtpmCodePath,
        ]
        process.standardOutput = FileHandle.nullDevice; process.standardError = FileHandle.nullDevice
        do { try process.run(); process.waitUntilExit() } catch { return false }
        return process.terminationReason == .exit && process.terminationStatus == 0
    }
}
