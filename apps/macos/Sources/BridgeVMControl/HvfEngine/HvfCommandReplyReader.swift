import Foundation

final class HvfCommandReplyReader {
    private let command: String
    private var offset: UInt64
    private var pending = Data()
    private var collecting = false
    private var body: [String] = []
    private var bodyBytes = 0
    private var outputTruncated = false
    private var exitCode: Int32 = -1
    private let outputLimitBytes: Int

    init(command: String, offset: UInt64, outputLimitBytes: Int = 4 * 1024 * 1024) {
        self.command = command
        self.offset = offset
        self.outputLimitBytes = max(1, outputLimitBytes)
    }

    func readReply(from url: URL) -> (output: String, code: Int32)? {
        guard let attributes = try? FileManager.default.attributesOfItem(atPath: url.path),
              let size = (attributes[.size] as? NSNumber)?.uint64Value else { return nil }
        if size < offset {
            offset = 0
            pending.removeAll(keepingCapacity: true)
            collecting = false
            body.removeAll(keepingCapacity: true)
            bodyBytes = 0
            outputTruncated = false
            exitCode = -1
        }
        guard size > offset, let handle = try? FileHandle(forReadingFrom: url) else { return nil }
        defer { try? handle.close() }
        do {
            try handle.seek(toOffset: offset)
            while let data = try handle.read(upToCount: 64 * 1024), !data.isEmpty {
                offset += UInt64(data.count)
                pending.append(data)
                if let reply = consumePendingLines() { return reply }
                let pendingLimit = min(outputLimitBytes, 256 * 1024)
                if pending.count > pendingLimit {
                    pending.removeFirst(pending.count - pendingLimit)
                    outputTruncated = true
                }
            }
        } catch {
            return nil
        }
        return consumePendingLines()
    }

    private func consumePendingLines() -> (output: String, code: Int32)? {
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
                let output = body.joined(separator: "\n")
                return (outputTruncated ? "[출력 일부 생략]\n" + output : output, exitCode)
            }
            appendBody(line)
            return nil
        }

        let prefix = "BVAGENT CMD \(command) exit="
        guard line.hasPrefix(prefix) else { return nil }
        let rawCode = line.dropFirst(prefix.count).prefix { $0 == "-" || $0.isNumber }
        exitCode = Int32(String(rawCode)) ?? -1
        collecting = true
        return nil
    }

    private func appendBody(_ line: String) {
        var retained = line
        let lineBytes = retained.utf8.count
        if lineBytes > outputLimitBytes {
            retained = String(retained.suffix(outputLimitBytes))
            outputTruncated = true
        }
        body.append(retained)
        bodyBytes += retained.utf8.count + (body.count > 1 ? 1 : 0)
        while bodyBytes > outputLimitBytes, body.count > 1 {
            let removed = body.removeFirst()
            bodyBytes -= removed.utf8.count + 1
            outputTruncated = true
        }
    }
}
