import Foundation

final class HvfCommandReplyReader {
    private let command: String
    private var offset: UInt64
    private var pending = Data()
    private var collecting = false
    private var body: [String] = []
    private var exitCode: Int32 = -1

    init(command: String, offset: UInt64) {
        self.command = command
        self.offset = offset
    }

    func readReply(from url: URL) -> (output: String, code: Int32)? {
        guard let attributes = try? FileManager.default.attributesOfItem(atPath: url.path),
              let size = (attributes[.size] as? NSNumber)?.uint64Value else { return nil }
        if size < offset {
            offset = 0
            pending.removeAll(keepingCapacity: true)
            collecting = false
            body.removeAll(keepingCapacity: true)
            exitCode = -1
        }
        guard size > offset, let handle = try? FileHandle(forReadingFrom: url) else { return nil }
        defer { try? handle.close() }
        do {
            try handle.seek(toOffset: offset)
            let data = try handle.readToEnd() ?? Data()
            offset += UInt64(data.count)
            pending.append(data)
        } catch {
            return nil
        }

        while let newline = pending.firstIndex(of: 10) {
            var lineData = Data(pending[..<newline])
            if lineData.last == 13 { lineData.removeLast() }
            pending.removeSubrange(...newline)
            let line = String(data: lineData, encoding: .utf8) ?? ""
            if let reply = consume(line) { return reply }
        }
        return nil
    }

    private func consume(_ line: String) -> (output: String, code: Int32)? {
        if collecting {
            if line == "BVAGENT END \(command)" {
                return (body.joined(separator: "\n"), exitCode)
            }
            body.append(line)
            return nil
        }

        let prefix = "BVAGENT CMD \(command) exit="
        guard line.hasPrefix(prefix) else { return nil }
        let rawCode = line.dropFirst(prefix.count).prefix { $0 == "-" || $0.isNumber }
        exitCode = Int32(String(rawCode)) ?? -1
        collecting = true
        return nil
    }
}
