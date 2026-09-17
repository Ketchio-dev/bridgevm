import Foundation

enum A9ImportedKeyCleanup {
    static func run(request: A9ImportRequest, fileManager: FileManager) -> Bool {
        let config = URL(fileURLWithPath: request.libraryRootPath)
            .appendingPathComponent(request.vmSlug).appendingPathComponent("vm.json")
        guard regular(config, fileManager: fileManager) else { return true }
        let process = Process()
        process.executableURL = URL(fileURLWithPath: request.appExecutablePath)
        process.arguments = [
            "--vtpm-lifecycle", "forget-import",
            "--stable-vm-id", request.vmSlug,
            "--state-dir", request.vtpmStatePath,
            "--package", request.sourceVtpmPackagePath,
            "--recovery-code-file", request.sourceVtpmCodePath,
        ]
        process.standardOutput = FileHandle.nullDevice; process.standardError = FileHandle.nullDevice
        do { try process.run(); process.waitUntilExit() } catch { return false }
        return process.terminationReason == .exit && process.terminationStatus == 0
    }

    private static func regular(_ url: URL, fileManager: FileManager) -> Bool {
        guard fileManager.fileExists(atPath: url.path),
              let values = try? url.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey]) else {
            return false
        }
        return values.isRegularFile == true && values.isSymbolicLink != true
    }
}
