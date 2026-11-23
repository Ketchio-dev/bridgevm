import Foundation
import Darwin

/// Worker-thread client. Successful completion requires a live native owner
/// throughout the normal protocol; callers must handle operation rollback.
final class HvfMediaLeaseSession {
    enum Mode: String {
        case ordinary = "lease"
        case pendingDelete = "lease-pending-delete"
    }

    private let process = Process()
    private let input = Pipe()
    private let output = Pipe()
    private let timeout: TimeInterval
    private var released = false
    private static let ready = Data("bridgevm-media-lease-v1\n".utf8)

    init(executable: URL, disk: String, vars: String, mode: Mode = .ordinary,
         timeout: TimeInterval = 5) throws {
        self.timeout = timeout
        guard timeout.isFinite, timeout > 0,
              executable.standardizedFileURL == executable.resolvingSymlinksInPath().standardizedFileURL,
              FileManager.default.isExecutableFile(atPath: executable.path),
              (try executable.resourceValues(forKeys: [.isRegularFileKey])).isRegularFile == true else {
            throw Self.failure("native media lease helper is unavailable or unsafe")
        }
        guard fcntl(input.fileHandleForWriting.fileDescriptor, F_SETNOSIGPIPE, 1) == 0 else {
            throw Self.failure("cannot configure lease control pipe")
        }
        process.executableURL = executable
        process.arguments = [mode.rawValue, disk, vars]
        process.environment = ["PATH": "/usr/bin:/bin:/usr/sbin:/sbin", "LANG": "C"]
        process.standardInput = input
        process.standardOutput = output
        process.standardError = FileHandle.nullDevice
        do {
            try process.run()
            try input.fileHandleForReading.close()
            try output.fileHandleForWriting.close()
            try receiveReady()
            guard process.isRunning else { throw Self.failure("lease helper exited before operation") }
        } catch {
            abort()
            throw error
        }
    }

    func finish() throws {
        guard !released, process.isRunning else { throw Self.failure("native media ownership was lost") }
        try input.fileHandleForWriting.write(contentsOf: Data("release\n".utf8))
        try input.fileHandleForWriting.close()
        guard waitForExit(timeout), process.terminationReason == .exit, process.terminationStatus == 0 else {
            throw Self.failure("native media lease release failed")
        }
        released = true
    }

    func abort() {
        try? input.fileHandleForWriting.close()
        if process.isRunning {
            process.terminate()
            if !waitForExit(1) {
                _ = kill(process.processIdentifier, SIGKILL)
                _ = waitForExit(1)
            }
        }
        try? output.fileHandleForReading.close()
    }

    deinit { abort() }

    private func receiveReady() throws {
        let deadline = ProcessInfo.processInfo.systemUptime + timeout
        var received = Data()
        while received.count < Self.ready.count {
            guard ProcessInfo.processInfo.systemUptime < deadline else {
                throw Self.failure("timed out waiting for native media ownership")
            }
            var descriptor = pollfd(fd: output.fileHandleForReading.fileDescriptor, events: Int16(POLLIN), revents: 0)
            let status = poll(&descriptor, 1, 50)
            if status < 0 && errno == EINTR { continue }
            guard status >= 0 else { throw Self.failure("lease readiness poll failed") }
            if status == 0 { continue }
            guard let bytes = try output.fileHandleForReading.read(upToCount: Self.ready.count - received.count),
                  !bytes.isEmpty else { throw Self.failure("lease helper closed before readiness") }
            received.append(bytes)
        }
        guard received == Self.ready else { throw Self.failure("invalid native media lease readiness") }
    }

    private func waitForExit(_ seconds: TimeInterval) -> Bool {
        let deadline = ProcessInfo.processInfo.systemUptime + seconds
        while process.isRunning && ProcessInfo.processInfo.systemUptime < deadline {
            Thread.sleep(forTimeInterval: 0.01)
        }
        return !process.isRunning
    }

    private static func failure(_ message: String) -> NSError {
        NSError(domain: "BridgeVM.MediaOwnership", code: 1,
                userInfo: [NSLocalizedDescriptionKey: message])
    }
}

extension HvfMediaLeaseSession {
    private static func acquire(config: VMConfig, mode: Mode = .ordinary) throws -> HvfMediaLeaseSession {
        let root = HvfEngineSession.defaultRepoRoot()
        let helper = root.appendingPathComponent("target/release/examples/snapshot_pair_cli")
        let media = finalMediaPaths(config)
        return try HvfMediaLeaseSession(executable: helper, disk: media.disk, vars: media.vars, mode: mode)
    }

    static func withOwnership<T>(config: VMConfig, mode: Mode = .ordinary, operation: () throws -> T) throws -> T {
        let lease = try acquire(config: config, mode: mode)
        defer { lease.abort() }
        let value = try operation()
        try lease.finish()
        return value
    }

    static func withCopyOwnership<T>(config: VMConfig, operation: (() -> Bool) -> T?) -> T? {
        guard let lease = try? acquire(config: config) else { return nil }
        return copyWhileOwned(session: lease, operation: operation)
    }

    /// The operation must validate the completed copy before publishing it.
    /// The source may resume after validation; subsequent work uses only the copy.
    static func copyWhileOwned<T>(session: HvfMediaLeaseSession, operation: (() -> Bool) -> T?) -> T? {
        defer { session.abort() }
        var verified = false
        let result = operation {
            guard !verified else { return false }
            do {
                try session.finish()
                verified = true
                return true
            } catch { return false }
        }
        return verified ? result : nil
    }
}
