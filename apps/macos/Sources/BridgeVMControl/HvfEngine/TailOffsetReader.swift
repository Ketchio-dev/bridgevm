import Foundation

final class TailOffsetReader {
    private var offset: UInt64 = 0
    private var pending = Data()

    func readNewLines(from url: URL) -> [String] {
        guard let attrs = try? FileManager.default.attributesOfItem(atPath: url.path),
              let size = attrs[.size] as? NSNumber else { return [] }
        let fileSize = size.uint64Value
        if fileSize < offset {
            offset = 0
            pending.removeAll(keepingCapacity: true)
        }
        guard fileSize != offset else { return [] }
        guard let handle = try? FileHandle(forReadingFrom: url) else { return [] }
        defer { try? handle.close() }
        do {
            try handle.seek(toOffset: offset)
            let data = try handle.readToEnd() ?? Data()
            offset += UInt64(data.count)
            pending.append(data)
        } catch {
            return []
        }
        return drainLines()
    }
    /// Consume whole lines in one pass and drop the consumed prefix once.
    ///
    /// Removing each line from the front as it was parsed re-copied every
    /// remaining byte per line, so cost grew with the square of the buffer.
    /// That is paid on the main actor every poll, and a session attaching to an
    /// existing run reads the whole log in a single call: a real 275 KB / 2143
    /// line run.log took 14 ms that way against 0.9 ms here.
    private func drainLines() -> [String] {
        var lines: [String] = []
        var start = pending.startIndex
        while let newline = pending[start...].firstIndex(of: 10) {
            var slice = pending[start..<newline]
            if slice.last == 13 { slice = slice.dropLast() }
            lines.append(String(decoding: slice, as: UTF8.self))
            start = pending.index(after: newline)
        }
        pending.removeSubrange(..<start)
        return lines
    }
}
