import Foundation

final class HvfCommandReplyReader {
    private let command: String
    private var offset: UInt64
    private var frames = HvfCommandReplyFrames()
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
            frames = HvfCommandReplyFrames()
            collecting = false
            body.removeAll(keepingCapacity: true)
            bodyBytes = 0
            outputTruncated = false
            exitCode = -1
        }
        if let reply = consumePendingLines() { return reply }
        guard size > offset, let handle = try? FileHandle(forReadingFrom: url) else { return nil }
        defer { try? handle.close() }
        do {
            try handle.seek(toOffset: offset)
            var remaining = 1024 * 1024
            while remaining > 0, let data = try handle.read(upToCount: min(64 * 1024, remaining)), !data.isEmpty {
                remaining -= data.count
                offset += UInt64(data.count)
                frames.append(data)
                if let reply = consumePendingLines() { return reply }
            }
        } catch {
            return nil
        }
        return consumePendingLines()
    }

    private func consumePendingLines() -> (output: String, code: Int32)? {
        while let line = frames.nextLine() {
            outputTruncated = outputTruncated || frames.discarded
            if let reply = consume(line) { return reply }
        }
        outputTruncated = outputTruncated || frames.discarded
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
        let rawCode = String(line.dropFirst(prefix.count))
        guard let parsed = Int32(rawCode), rawCode == String(parsed) else { return nil }
        exitCode = parsed
        collecting = true
        return nil
    }

    private func appendBody(_ line: String) {
        var retained = line
        let lineBytes = retained.utf8.count
        if lineBytes > outputLimitBytes {
            retained = HvfCommandReplyFrames.utf8Suffix(retained, maximumBytes: outputLimitBytes)
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
