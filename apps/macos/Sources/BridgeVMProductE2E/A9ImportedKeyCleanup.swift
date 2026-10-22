import Foundation

/// Forgets the imported key of every registration in the lane's private library (see A9LaneRegistrations).
/// forget-import opens the package against a state that must match it, and a booted lane has rewritten its
/// own, so the sealed source state the package was made from is used; the source is checked unchanged later.
enum A9ImportedKeyCleanup {
    static func run(request: A9ImportRequest, fileManager: FileManager) -> Bool {
        guard let slugs = A9LaneRegistrations.slugs(request: request, fileManager: fileManager) else { return false }
        return slugs.map { forget($0, request: request) }.allSatisfy { $0 }
    }

    private static func forget(_ slug: String, request: A9ImportRequest) -> Bool {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: request.appExecutablePath)
        process.arguments = [
            "--vtpm-lifecycle", "forget-import",
            "--stable-vm-id", slug,
            "--state-dir", request.sourceVtpmPath,
            "--package", request.sourceVtpmPackagePath,
            "--recovery-code-file", request.sourceVtpmCodePath,
        ]
        process.standardOutput = FileHandle.nullDevice; process.standardError = FileHandle.nullDevice
        do { try process.run(); process.waitUntilExit() } catch { return false }
        return process.terminationReason == .exit && process.terminationStatus == 0
    }
}
