import Foundation

/// Thread-safe newline splitter for Process pipe callbacks.
final class LineAccumulator: @unchecked Sendable {
    private var buffer = Data()
    private let lock = NSLock()

    /// Split on newlines in one pass, dropping the consumed prefix once. This
    /// avoids rebuilding every remaining byte for each line in a large burst.
    func append(_ data: Data) -> [String] {
        lock.lock()
        defer { lock.unlock() }
        buffer.append(data)
        var lines: [String] = []
        var start = buffer.startIndex
        while let newline = buffer[start...].firstIndex(of: 0x0a) {
            let lineData = buffer[start..<newline]
            if let line = String(data: lineData, encoding: .utf8), !line.isEmpty {
                lines.append(line)
            }
            start = buffer.index(after: newline)
        }
        buffer.removeSubrange(..<start)
        return lines
    }
}
