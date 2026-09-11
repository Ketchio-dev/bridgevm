import Darwin
import Foundation

/// Validate and consume the same descriptor; size metadata is only an early check.
enum HvfWindowsInstallRegularFile {
    static func read(_ url: URL, maximumBytes: Int?) throws -> Data {
        try validateLimit(maximumBytes)
        let descriptor = open(url.path, O_RDONLY | O_NOFOLLOW | O_NONBLOCK | O_CLOEXEC)
        guard descriptor >= 0 else { throw HvfWindowsInstallFinalizationError.unsafePath(url.path) }
        defer { close(descriptor) }
        var status = stat()
        guard fstat(descriptor, &status) == 0,
              status.st_mode & S_IFMT == S_IFREG, status.st_size >= 0 else {
            throw HvfWindowsInstallFinalizationError.unsafePath(url.path)
        }
        if let maximumBytes, status.st_size > Int64(maximumBytes) { throw exceedsLimit() }
        let handle = FileHandle(fileDescriptor: descriptor, closeOnDealloc: false)
        return try collect(maximumBytes: maximumBytes) { count in
            try handle.read(upToCount: count) ?? Data()
        }
    }

    /// Probe one byte beyond the limit so growth cannot silently truncate a file.
    static func collect(maximumBytes: Int?, next: (Int) throws -> Data) throws -> Data {
        try validateLimit(maximumBytes)
        var output = Data()
        while true {
            let count: Int
            if let maximumBytes {
                let remaining = maximumBytes - output.count
                count = remaining >= 65_536 ? 65_536 : remaining + 1
            } else {
                count = 65_536
            }
            let chunk = try next(count)
            if chunk.isEmpty { return output }
            if let maximumBytes, chunk.count > maximumBytes - output.count { throw exceedsLimit() }
            output.append(chunk)
        }
    }

    private static func validateLimit(_ maximumBytes: Int?) throws {
        if let maximumBytes, maximumBytes < 0 { throw exceedsLimit() }
    }

    private static func exceedsLimit() -> HvfWindowsInstallFinalizationError {
        .invalidState("Windows installation metadata exceeds its allowed byte limit.")
    }
}
