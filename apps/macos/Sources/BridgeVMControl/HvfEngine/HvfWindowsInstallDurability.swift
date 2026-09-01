import Darwin
import Foundation

enum HvfWindowsInstallDurability {
    final class TransactionLock {
        private let descriptor: Int32

        init(url: URL, nonBlocking: Bool) throws {
            try ensureDirectory(url.deletingLastPathComponent())
            descriptor = open(url.path, O_CREAT | O_RDWR | O_NOFOLLOW, S_IRUSR | S_IWUSR)
            guard descriptor >= 0 else { throw HvfWindowsInstallFinalizationError.unsafePath(url.path) }
            let operation = LOCK_EX | (nonBlocking ? LOCK_NB : 0)
            guard flock(descriptor, operation) == 0 else {
                close(descriptor)
                throw HvfWindowsInstallFinalizationError.transactionBusy
            }
        }

        deinit { flock(descriptor, LOCK_UN); close(descriptor) }
    }

    static func ensureDirectory(_ url: URL) throws {
        let fm = FileManager.default
        if fm.fileExists(atPath: url.path) {
            let values = try url.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey])
            guard values.isDirectory == true, values.isSymbolicLink != true else {
                throw HvfWindowsInstallFinalizationError.unsafePath(url.path)
            }
            return
        }
        try fm.createDirectory(at: url, withIntermediateDirectories: true)
        try syncDirectory(url)
        try syncDirectory(url.deletingLastPathComponent())
    }

    static func durableWrite(_ data: Data, to destination: URL) throws {
        let fm = FileManager.default
        try ensureDirectory(destination.deletingLastPathComponent())
        try refuseSymlink(destination)
        let temporary = destination.deletingLastPathComponent()
            .appendingPathComponent(".\(destination.lastPathComponent).\(UUID().uuidString).tmp")
        defer { try? fm.removeItem(at: temporary) }
        try data.write(to: temporary)
        try syncFile(temporary)
        try publish(temporary, to: destination)
    }

    static func durableCloneOrCopy(from source: URL, to destination: URL) throws {
        let fm = FileManager.default
        guard fm.fileExists(atPath: source.path) else {
            throw HvfWindowsInstallFinalizationError.missingArtifact(source.path)
        }
        try refuseSymlink(source)
        try refuseSymlink(destination)
        try ensureDirectory(destination.deletingLastPathComponent())
        let temporary = destination.deletingLastPathComponent()
            .appendingPathComponent(".\(destination.lastPathComponent).\(UUID().uuidString).tmp")
        defer { try? fm.removeItem(at: temporary) }
        let clone = Shell.run("/bin/cp", ["-c", source.path, temporary.path])
        if clone.code != 0 {
            try? fm.removeItem(at: temporary)
            try fm.copyItem(at: source, to: temporary)
        }
        try syncFile(temporary)
        try publish(temporary, to: destination)
    }

    static func durableRemove(_ url: URL) throws {
        guard FileManager.default.fileExists(atPath: url.path) else { return }
        try refuseSymlink(url)
        try FileManager.default.removeItem(at: url)
        try syncDirectory(url.deletingLastPathComponent())
    }

    static func readRegularFile(_ url: URL, maximumBytes: Int? = nil) throws -> Data {
        let values = try url.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey, .fileSizeKey])
        guard values.isSymbolicLink != true, values.isRegularFile == true else {
            throw HvfWindowsInstallFinalizationError.unsafePath(url.path)
        }
        if let maximumBytes, (values.fileSize ?? maximumBytes + 1) > maximumBytes {
            throw HvfWindowsInstallFinalizationError.invalidState("파일이 허용 크기를 초과했습니다: \(url.path)")
        }
        return try Data(contentsOf: url)
    }

    static func fileSize(_ url: URL) throws -> UInt64 {
        try refuseSymlink(url)
        guard try url.resourceValues(forKeys: [.isRegularFileKey]).isRegularFile == true else {
            throw HvfWindowsInstallFinalizationError.unsafePath(url.path)
        }
        let attributes = try FileManager.default.attributesOfItem(atPath: url.path)
        guard let size = attributes[.size] as? NSNumber else {
            throw HvfWindowsInstallFinalizationError.missingArtifact(url.path)
        }
        return size.uint64Value
    }

    static func refuseSymlink(_ url: URL) throws {
        guard FileManager.default.fileExists(atPath: url.path) else { return }
        if try url.resourceValues(forKeys: [.isSymbolicLinkKey]).isSymbolicLink == true {
            throw HvfWindowsInstallFinalizationError.unsafePath(url.path)
        }
    }

    static func canonical(_ url: URL) -> String {
        url.standardizedFileURL.resolvingSymlinksInPath().path
    }

    static func syncFile(_ url: URL) throws {
        let handle = try FileHandle(forWritingTo: url)
        defer { try? handle.close() }
        try handle.synchronize()
    }

    static func syncDirectory(_ url: URL) throws {
        let descriptor = open(url.path, O_RDONLY)
        guard descriptor >= 0 else { throw CocoaError(.fileWriteUnknown) }
        defer { close(descriptor) }
        guard fsync(descriptor) == 0 else { throw CocoaError(.fileWriteUnknown) }
    }

    private static func publish(_ temporary: URL, to destination: URL) throws {
        let fm = FileManager.default
        if fm.fileExists(atPath: destination.path) {
            _ = try fm.replaceItemAt(destination, withItemAt: temporary)
        } else {
            try fm.moveItem(at: temporary, to: destination)
        }
        try syncDirectory(destination.deletingLastPathComponent())
    }
}
