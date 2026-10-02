import Foundation

final class TailOffsetReader {
    private var cursor: HvfTailFileCursor
    private var accumulator = HvfTailLineAccumulator()
    init(startingAt offset: UInt64 = 0) { cursor = HvfTailFileCursor(startingAt: offset) }
    func readNewLines(from url: URL) -> [String] {
        guard let handle = HvfTailFileCursor.openFile(url) else { return [] }
        defer { try? handle.close() }
        guard let state = cursor.admit(handle, path: url.standardizedFileURL.path) else { return [] }
        if state.reset { accumulator.reset() }
        guard state.size != cursor.offset else { return [] }
        do {
            try handle.seek(toOffset: cursor.offset)
            let data = try handle.read(upToCount: 1_048_576) ?? Data()
            cursor.advance(data.count)
            return accumulator.consume(data)
        } catch {
            return []
        }
    }
}
