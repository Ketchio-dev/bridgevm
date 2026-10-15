import Foundation
import Darwin

/// Keep nohup's app-lifetime behavior while retaining the exact spawned child.
enum GenericOwnedProcessLauncher {
    static func launch(_ spec: GenericOwnedProcessSpec) throws -> Process {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/nohup")
        process.arguments = [spec.executable] + spec.arguments
        process.environment = ProcessInfo.processInfo.environment.merging(spec.environmentOverrides) { _, new in new }
        process.standardInput = FileHandle.nullDevice
        let output: FileHandle
        switch spec.output {
        case .discard: output = .nullDevice
        case let .file(path):
            let descriptor = open(path, O_WRONLY | O_CREAT | O_NONBLOCK | O_NOFOLLOW | O_CLOEXEC, 0o600)
            guard descriptor >= 0 else { throw POSIXError(POSIXErrorCode(rawValue: errno) ?? .EIO) }
            var info = stat()
            guard fstat(descriptor, &info) == 0, info.st_mode & S_IFMT == S_IFREG else {
                close(descriptor)
                throw CocoaError(.fileWriteInvalidFileName)
            }
            output = FileHandle(fileDescriptor: descriptor, closeOnDealloc: true)
            do { try output.truncate(atOffset: 0) }
            catch { try? output.close(); throw error }
        }
        defer { if case .file = spec.output { try? output.close() } }
        process.standardOutput = output
        process.standardError = output
        try process.run()
        return process
    }
}
