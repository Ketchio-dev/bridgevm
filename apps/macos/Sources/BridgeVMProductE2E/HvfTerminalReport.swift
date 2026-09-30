import Foundation

/// The HVF final report's stop record, bound to host framing rather than text.
/// final_report.rs prints the banner, `stop: {reason}`, host records, the count
/// `serial raw bytes: R output bytes: M`, `--- serial (tail) ---`, exactly M
/// bytes of rendered guest serial and the `--- end ---` footer; only host
/// teardown records follow. Guest bytes can precede the report and fill the
/// counted tail, so the stop record follows the last banner before the only
/// count whose tail ends at the final footer; a second one rejects the log. With
/// 8-digit count fields that holds only below `logLimit` bytes and only over the
/// whole run.log. scripts/live-gates/hvf_terminal_report.py applies the grammar.
enum HvfTerminalReport {
    private static let banner = Data("\n=== EDK2 boot probe (with Apple hv_gic) ===\n".utf8)
    private static let serial = Data("\n--- serial (tail) ---\n".utf8)
    private static let footer = Data("\n--- end ---\n".utf8)
    private static let newline = Data([10])
    static let logLimit = 100_000_000
    private static let hostTail = T17TerminalReportTail.hostLines + [
        #"^[A-Z][a-z]{2} [ 0-9][0-9] [0-9]{2}:[0-9]{2}:[0-9]{2}  virgl_render_server\[[0-9]+\] <Debug>: socket disconnected$"#,
    ]

    static func stop(in text: String) -> (offset: Int, line: String)? { stop(in: Data(text.utf8)) }
    static func stop(in data: Data) -> (offset: Int, line: String)? {
        let log = Data(data)
        guard log.count < logLimit, let tailEnd = tailEnd(log) else { return nil }
        var counts: [Int] = [], from = 0
        while let marker = log.range(of: serial, in: from..<tailEnd) {
            let start = log.range(of: newline, options: .backwards, in: 0..<marker.lowerBound)?.upperBound ?? 0
            let count = String(data: log[start..<marker.lowerBound], encoding: .ascii)
                .flatMap(T17TerminalReportTail.serialCount)
            if let count, !count.legacy, count.output == tailEnd - marker.upperBound { counts.append(start) }
            from = marker.lowerBound + 1
        }
        guard counts.count == 1,
              let found = log.range(of: banner, options: .backwards, in: 0..<counts[0]),
              let end = log.range(of: newline, in: found.upperBound..<counts[0])?.lowerBound,
              log[found.upperBound..<end].starts(with: Data("stop: ".utf8)),
              let line = String(data: log[found.upperBound..<end], encoding: .utf8) else { return nil }
        return (found.upperBound, line)
    }

    private static func tailEnd(_ log: Data) -> Int? {
        var end = log.count
        for _ in 0...T17TerminalReportTail.maxTailLines {
            if end >= footer.count, log[(end - footer.count)..<end] == footer {
                return log.count - end <= T17TerminalReportTail.maxTailBytes ? end - footer.count : nil
            }
            guard end > 0, log[end - 1] == 10 else { return nil }
            let start = log.range(of: newline, options: .backwards, in: 0..<(end - 1))?.upperBound ?? 0
            guard let record = String(data: log[start..<(end - 1)], encoding: .ascii),
                  hostTail.contains(where: { record.range(of: $0, options: .regularExpression) != nil }) else { return nil }
            end = start
        }
        return nil
    }
}
