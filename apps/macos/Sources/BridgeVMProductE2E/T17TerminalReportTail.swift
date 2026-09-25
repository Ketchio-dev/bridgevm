import Foundation

/// Recognizes the final host report and only the bounded audio shutdown tail.
enum T17TerminalReportTail {
    static let maxTailBytes = 4 * 1024
    static let maxTailLines = 16
    private static let footer = Data("\n--- end ---\n".utf8)
    private static let hostLines = [
        #"^hda CoreAudio callback enqueue: state=stopping reason=[a-z-]+ osstatus=-?[0-9]+ expected=(true|false)$"#,
        #"^hda CoreAudio lifecycle: operation=(stop|dispose) osstatus=-?[0-9]+ success=(true|false)$"#,
        #"^hda CoreAudio stats: [a-z][a-z0-9_]*=[0-9]+( [a-z][a-z0-9_]*=[0-9]+)*$"#,
    ]

    static func isComplete(rawSuffix: Data, nonce: String, generation: UInt64) -> Bool {
        let ack = Data("HOST-DIAGNOSTIC-STOP: generation=\(generation) nonce=\(nonce) request consumed; ending run through final report".utf8)
        let serialMarker = Data("\n--- serial (tail) ---\n".utf8)
        let bannerMarker = Data("\n=== EDK2 boot probe (with Apple hv_gic) ===\n".utf8)
        let stopMarker = Data("stop: host diagnostic stop requested\n".utf8)
        guard let ackRange = rawSuffix.range(of: ack),
              (ackRange.lowerBound == 0 || rawSuffix[ackRange.lowerBound - 1] == 10),
              ackRange.upperBound < rawSuffix.count, rawSuffix[ackRange.upperBound] == 10,
              let serial = rawSuffix.range(of: serialMarker, in: ackRange.upperBound..<rawSuffix.count),
              let banner = rawSuffix.range(of: bannerMarker, in: ackRange.upperBound..<serial.lowerBound),
              let stop = rawSuffix.range(of: stopMarker, in: banner.upperBound..<serial.lowerBound) else {
            return false
        }
        let countStart = rawSuffix.range(of: Data([10]), options: .backwards,
                                         in: stop.upperBound..<serial.lowerBound)?.upperBound ?? stop.upperBound
        guard let countLine = String(data: rawSuffix[countStart..<serial.lowerBound], encoding: .ascii),
              let count = serialCount(countLine),
              count.output <= rawSuffix.count - serial.upperBound else { return false }
        let footerStart = serial.upperBound + count.output
        guard rawSuffix.count - footerStart >= footer.count,
              rawSuffix[footerStart..<footerStart + footer.count] == footer,
              let displayed = String(data: rawSuffix[serial.upperBound..<footerStart], encoding: .utf8),
              (!count.legacy || !displayed.contains("\u{FFFD}")) else { return false }
        let tail = rawSuffix[footerStart + footer.count..<rawSuffix.count]
        guard let text = String(data: tail, encoding: .utf8) else { return false }
        return isBoundedHostShutdownTail(text)
    }

    private static func serialCount(_ line: String) -> (output: Int, legacy: Bool)? {
        let modern = "serial raw bytes: "
        if line.hasPrefix(modern) {
            let fields = String(line.dropFirst(modern.count)).components(separatedBy: " output bytes: ")
            guard fields.count == 2, let raw = decimal(fields[0]), let output = decimal(fields[1]),
                  output >= raw else { return nil }
            return (output, false)
        }
        let old = "serial bytes: "
        guard line.hasPrefix(old), let output = decimal(String(line.dropFirst(old.count))) else { return nil }
        return (output, true)
    }

    private static func decimal(_ value: String) -> Int? {
        let bytes = value.utf8
        guard !bytes.isEmpty, bytes.count <= 8,
              bytes.allSatisfy({ (48...57).contains($0) }) else { return nil }
        return Int(value)
    }

    private static func isBoundedHostShutdownTail(_ tail: String) -> Bool {
        if tail.isEmpty { return true }
        guard tail.utf8.count <= maxTailBytes, tail.hasSuffix("\n") else { return false }
        let lines = tail.split(separator: "\n", omittingEmptySubsequences: false)
        guard lines.count <= maxTailLines + 1, lines.last?.isEmpty == true else { return false }
        return lines.dropLast().allSatisfy { line in
            let value = String(line)
            return hostLines.contains { value.range(of: $0, options: .regularExpression) != nil }
        }
    }
}
