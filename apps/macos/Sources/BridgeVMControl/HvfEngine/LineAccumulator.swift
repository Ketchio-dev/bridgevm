import Foundation

/// Thread-safe installer pipe framing. Oversized records are discarded through
/// their newline, never returned as independent suffix records.
final class LineAccumulator: @unchecked Sendable {
    static let maximumLineBytes = 2 * 1_048_576
    private var buffer = Data()
    private var discarding = false
    private let lock = NSLock()

    func append(_ data: Data) -> [String] {
        lock.lock()
        defer { lock.unlock() }
        var lines: [String] = []
        var start = data.startIndex
        while start < data.endIndex {
            let newline = data[start...].firstIndex(of: 0x0a)
            let end = newline ?? data.endIndex
            let fragment = data[start..<end]
            if !discarding {
                if fragment.count > Self.maximumLineBytes - buffer.count {
                    buffer = Data()
                    discarding = true
                } else {
                    buffer.append(fragment)
                }
            }
            guard let newline else { break }
            if !discarding, let line = String(data: buffer, encoding: .utf8), !line.isEmpty {
                lines.append(line)
            }
            buffer = Data()
            discarding = false
            start = data.index(after: newline)
        }
        return lines
    }
}
