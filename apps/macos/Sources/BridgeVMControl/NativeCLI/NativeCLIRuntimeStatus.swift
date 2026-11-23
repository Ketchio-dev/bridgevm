import Foundation
import Darwin
struct NativeCLIRuntimeStatus: Encodable {
    let schema = NativeRuntimeCodec.responseSchema
    let scope = NativeRuntimeCodec.scope
    let vmID: String
    let libraryPath: String
    let savedConfiguration: NativeRuntimeSavedConfiguration
    let complete: Bool
    let runtimeState: String
    let unavailableReason: String?
    let appInstanceID: String?
    let observedAt: Double?
    let sessions: [NativeRuntimeSessionObservation]

    static func snapshot(rootURL: URL, id: String) -> Self {
        var saved = NativeRuntimeSavedConfiguration(state: .unreadable, digest: nil)
        do {
            let library = try NativeRuntimeLibraryHandle.open(rootURL: rootURL, create: false)
            saved = NativeCLIStatusConfiguration.read(library: library, id: id)
            let request = NativeRuntimeRequest(schema: NativeRuntimeCodec.requestSchema, operation: "status",
                requestID: UUID().uuidString, library: library.identity, vmID: id, savedConfiguration: saved)
            let result = try NativeRuntimeClient.query(rootURL: rootURL, request: request)
            try library.validateCurrentIdentity()
            try NativeRuntimeCodec.validate(result, for: request)
            return Self(vmID: id, libraryPath: library.identity.canonicalPath, savedConfiguration: saved,
                complete: true, runtimeState: result.sessions.contains { $0.ownership != .notObserved } ? "observed" : "unobserved",
                unavailableReason: nil, appInstanceID: result.appInstanceID, observedAt: result.observedAt, sessions: result.sessions)
        } catch {
            return Self(vmID: id, libraryPath: rootURL.path, savedConfiguration: saved,
                complete: false, runtimeState: "unobserved",
                unavailableReason: (error as? NativeRuntimeError)?.rawValue ?? "transportFailure",
                appInstanceID: nil, observedAt: nil, sessions: [])
        }
    }

    var text: String {
        var lines = ["Native VM runtime: \(vmID)", "Observation scope: \(scope)",
                     "Runtime state: \(runtimeState)", "Saved configuration: \(savedConfiguration.state.rawValue)"]
        if let unavailableReason { lines.append("Observation unavailable: \(unavailableReason)") }
        if complete && sessions.isEmpty { lines.append("The running app has no retained session for this ID.") }
        for session in sessions {
            lines.append("Session: \(session.ownership.rawValue), connection: \(session.connectionState ?? "unobserved")")
            lines.append("  Accepted configuration: \(session.configurationMatch.rawValue)")
            lines.append("  Graphics mode: \(session.graphicsMode?.rawValue ?? "not-reported")")
            if let process = session.ownedProcess { lines.append("  Owned child PID: \(process.processID)") }
            if let exit = session.lastOwnedExit {
                lines.append("  Last owned child exit: PID \(exit.process.processID), \(exit.reason), status \(exit.status)")
            }
        }
        lines.append("App-retained observations only; no guest-health or system-wide VM state is inferred.")
        return lines.joined(separator: "\n") + "\n"
    }
}

enum NativeCLIStatusConfiguration {
    static func read(library: NativeRuntimeLibraryHandle, id: String) -> NativeRuntimeSavedConfiguration {
        var entry = stat()
        if fstatat(library.descriptor, id, &entry, AT_SYMLINK_NOFOLLOW) != 0 {
            return .init(state: errno == ENOENT ? .missing : .unreadable, digest: nil)
        }
        guard entry.st_mode & S_IFMT == S_IFDIR else { return .init(state: .unreadable, digest: nil) }
        let directory = openat(library.descriptor, id, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)
        guard directory >= 0 else { return .init(state: .unreadable, digest: nil) }
        defer { close(directory) }
        if fstatat(directory, "vm.json", &entry, AT_SYMLINK_NOFOLLOW) != 0 {
            return .init(state: errno == ENOENT ? .missing : .unreadable, digest: nil)
        }
        do {
            let config = try NativeCLIStatusConfigReader.read(library: library, id: id)
            return .init(state: .present, digest: try NativeRuntimeConfigurationIdentity.digest(config: config))
        } catch { return .init(state: .unreadable, digest: nil) }
    }
}

/// A comparison describes one bounded, unchanged file read, never a later configuration.
enum NativeCLIStatusConfigReader {
    static func read(library: NativeRuntimeLibraryHandle, id: String) throws -> VMConfig {
        let directory = openat(library.descriptor, id, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)
        guard directory >= 0 else { throw NativeRuntimeError.snapshotUnavailable }
        defer { close(directory) }
        let file = openat(directory, "vm.json", O_RDONLY | O_NONBLOCK | O_NOFOLLOW | O_CLOEXEC)
        guard file >= 0 else { throw NativeRuntimeError.snapshotUnavailable }
        defer { close(file) }
        var before = stat()
        guard fstat(file, &before) == 0, before.st_mode & S_IFMT == S_IFREG,
              before.st_size >= 0, before.st_size <= VMLibrary.maximumConfigBytes else {
            throw NativeRuntimeError.snapshotUnavailable
        }
        var data = Data(), buffer = [UInt8](repeating: 0, count: 65_536)
        while data.count <= VMLibrary.maximumConfigBytes {
            let count = Darwin.read(file, &buffer, min(buffer.count, VMLibrary.maximumConfigBytes + 1 - data.count))
            if count < 0 && errno == EINTR { continue }
            guard count >= 0 else { throw NativeRuntimeError.snapshotUnavailable }
            if count == 0 { break }
            data.append(contentsOf: buffer.prefix(count))
        }
        guard data.count == before.st_size, data.count <= VMLibrary.maximumConfigBytes else {
            throw NativeRuntimeError.snapshotUnavailable
        }
        try validateUnchanged(file: file, directory: directory, library: library.descriptor, id: id, before: before)
        var config = try JSONDecoder().decode(VMConfig.self, from: data)
        config.id = id
        return config
    }

    static func validateUnchanged(file: Int32, directory: Int32, library: Int32, id: String, before: stat) throws {
        var after = stat(), registeredFile = stat(), heldDirectory = stat(), registeredDirectory = stat()
        guard fstat(file, &after) == 0, fstatat(directory, "vm.json", &registeredFile, AT_SYMLINK_NOFOLLOW) == 0,
              fstat(directory, &heldDirectory) == 0,
              fstatat(library, id, &registeredDirectory, AT_SYMLINK_NOFOLLOW) == 0,
              sameFile(before, after), sameFile(after, registeredFile),
              heldDirectory.st_dev == registeredDirectory.st_dev,
              heldDirectory.st_ino == registeredDirectory.st_ino,
              registeredDirectory.st_mode & S_IFMT == S_IFDIR else { throw NativeRuntimeError.snapshotUnavailable }
    }

    private static func sameFile(_ lhs: stat, _ rhs: stat) -> Bool {
        lhs.st_dev == rhs.st_dev && lhs.st_ino == rhs.st_ino && lhs.st_mode == rhs.st_mode
            && lhs.st_size == rhs.st_size && lhs.st_uid == rhs.st_uid
            && lhs.st_mtimespec.tv_sec == rhs.st_mtimespec.tv_sec && lhs.st_mtimespec.tv_nsec == rhs.st_mtimespec.tv_nsec
            && lhs.st_ctimespec.tv_sec == rhs.st_ctimespec.tv_sec && lhs.st_ctimespec.tv_nsec == rhs.st_ctimespec.tv_nsec
    }
}
