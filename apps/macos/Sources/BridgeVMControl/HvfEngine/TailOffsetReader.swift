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

    private func drainLines() -> [String] {
        var lines: [String] = []
        while let newline = pending.firstIndex(of: 10) {
            let raw = pending[..<newline]
            var lineData = Data(raw)
            if lineData.last == 13 { lineData.removeLast() }
            lines.append(String(data: lineData, encoding: .utf8) ?? "")
            pending.removeSubrange(...newline)
        }
        return lines
    }
}
