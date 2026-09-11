import Foundation

struct HvfCommandReplyFrames {
    private var pending = Data()
    private var discarding = false
    private(set) var discarded = false
    private let lineLimit = 256 * 1024

    mutating func append(_ data: Data) { pending.append(data) }

    mutating func nextLine() -> String? {
        while let newline = pending.firstIndex(of: 10) {
            var bytes = Data(pending[..<newline])
            pending.removeSubrange(...newline)
            if discarding { discarding = false; continue }
            if bytes.last == 13 { bytes.removeLast() }
            guard bytes.count <= lineLimit, let line = String(data: bytes, encoding: .utf8) else {
                discarded = true
                continue
            }
            return line
        }
        if pending.count > lineLimit {
            pending.removeAll(keepingCapacity: true)
            discarding = true
            discarded = true
        }
        return nil
    }

    static func utf8Suffix(_ text: String, maximumBytes: Int) -> String {
        var bytes = text.utf8.suffix(maximumBytes)
        while let first = bytes.first, first & 0xc0 == 0x80 { bytes = bytes.dropFirst() }
        return String(decoding: bytes, as: UTF8.self)
    }
}
