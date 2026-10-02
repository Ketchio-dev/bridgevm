import Foundation

final class TailOffsetReader {
    private var offset: UInt64 = 0
    private var accumulator = HvfTailLineAccumulator()
    init(startingAt offset: UInt64 = 0) { self.offset = offset }
    func readNewLines(from url: URL) -> [String] {
        guard let attrs = try? FileManager.default.attributesOfItem(atPath: url.path),
              let size = attrs[.size] as? NSNumber else { return [] }
        let fileSize = size.uint64Value
        if fileSize < offset {
            offset = 0
            accumulator.reset()
        }
        guard fileSize != offset else { return [] }
        guard let handle = try? FileHandle(forReadingFrom: url) else { return [] }
        defer { try? handle.close() }
        do {
            try handle.seek(toOffset: offset)
            let data = try handle.read(upToCount: 1_048_576) ?? Data()
            offset += UInt64(data.count)
            return accumulator.consume(data)
        } catch {
            return []
        }
    }
}
