import Darwin
import Foundation

/// Bounded app pacing reads: the descriptor checked with fstat is the one read, even if its path changes.
enum T17AppPacingFile {
    static func read(_ url: URL, limit: Int) -> Data? {
        let descriptor = Darwin.open(url.path, O_RDONLY | O_NOFOLLOW | O_NONBLOCK | O_CLOEXEC)
        guard descriptor >= 0 else { return nil }
        defer { Darwin.close(descriptor) }
        return read(descriptor: descriptor, limit: limit)
    }

    /// Reads a caller-owned descriptor from its current offset. Never closes or reopens it by path.
    static func read(descriptor: Int32, limit: Int) -> Data? {
        var state = stat()
        guard limit >= 0, limit < Int.max, Darwin.fstat(descriptor, &state) == 0,
              state.st_mode & mode_t(S_IFMT) == mode_t(S_IFREG),
              state.st_size >= 0, state.st_size <= limit else { return nil }
        var data = Data(), buffer = [UInt8](repeating: 0, count: 4096)
        while data.count <= limit {
            let remaining = min(buffer.count, limit + 1 - data.count)
            let count = buffer.withUnsafeMutableBytes { Darwin.read(descriptor, $0.baseAddress, remaining) }
            if count < 0 {
                if errno == EINTR { continue }
                return nil
            }
            if count == 0 { return data }
            data.append(contentsOf: buffer.prefix(count))
        }
        return nil
    }
}
