import Foundation

/// Oversized logical lines are discarded whole, so their suffix cannot become
/// an independent protocol record. Ordinary incomplete UTF-8 stays as bytes.
struct HvfTailLineAccumulator {
    static let maximumLineBytes = 2 * 1_048_576
    private(set) var pending = Data()
    private var discarding = false

    mutating func reset() {
        pending = Data()
        discarding = false
    }

    mutating func consume(_ data: Data) -> [String] {
        var lines: [String] = []
        var start = data.startIndex
        while start < data.endIndex {
            let newline = data[start...].firstIndex(of: 10)
            let end = newline ?? data.endIndex
            let fragment = data[start..<end]
            if !discarding {
                if fragment.count > Self.maximumLineBytes - pending.count {
                    pending = Data()
                    discarding = true
                } else {
                    pending.append(fragment)
                }
            }
            guard let newline else { break }
            if !discarding {
                var line = pending[...]
                if line.last == 13 { line = line.dropLast() }
                lines.append(String(decoding: line, as: UTF8.self))
            }
            pending = Data()
            discarding = false
            start = data.index(after: newline)
        }
        return lines
    }
}
