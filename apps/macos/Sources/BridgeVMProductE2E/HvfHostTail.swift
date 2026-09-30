import Foundation

/// The host records that may follow the final report's footer, as
/// scripts/live-gates/hvf_host_tail.py states them: CoreAudio callback-enqueue,
/// lifecycle and stats records, at most one CoreAudio continuity record printed
/// after a stats record, and for HvfTerminalReport the render server's
/// disconnect line. The bounds hold with the continuity record counted, so a
/// guest cannot extend a forged footer; a log printed before that record
/// existed reads exactly as before, and nothing requires one.
enum HvfHostTail {
    static let maxBytes = 4 * 1024
    static let maxLines = 16
    static let continuityFields = [
        "active_callbacks", "underrun_callbacks", "underrun_frames", "contention_callbacks", "gaps",
        "max_gap_frames", "stream_stops", "callback_frames",
    ]
    static let audioLines = [
        #"^hda CoreAudio callback enqueue: state=stopping reason=[a-z-]+ osstatus=-?[0-9]+ expected=(true|false)$"#,
        #"^hda CoreAudio lifecycle: operation=(stop|dispose) osstatus=-?[0-9]+ success=(true|false)$"#,
        #"^hda CoreAudio stats: [a-z][a-z0-9_]*=[0-9]+( [a-z][a-z0-9_]*=[0-9]+)*$"#,
        "^hda CoreAudio continuity:" + continuityFields.map { " \($0)=(0|[1-9][0-9]{0,19})" }.joined() + "$",
    ]
    static let lines = audioLines + [
        #"^[A-Z][a-z]{2} [ 0-9][0-9] [0-9]{2}:[0-9]{2}:[0-9]{2}  virgl_render_server\[[0-9]+\] <Debug>: socket disconnected$"#,
    ]

    /// At most one continuity record, and only after a stats record.
    static func continuityPlaced<S: Sequence>(_ records: S) -> Bool where S.Element: StringProtocol {
        var stats = false, continuity = false
        for record in records {
            if record.hasPrefix("hda CoreAudio stats:") {
                stats = true
            } else if record.hasPrefix("hda CoreAudio continuity:") {
                guard stats, !continuity else { return false }
                continuity = true
            }
        }
        return true
    }

    /// A T17 terminal report's post-footer text: empty, or bounded audio records.
    static func isBoundedShutdownTail(_ tail: String) -> Bool {
        if tail.isEmpty { return true }
        guard tail.utf8.count <= maxBytes, tail.hasSuffix("\n") else { return false }
        let lines = tail.split(separator: "\n", omittingEmptySubsequences: false)
        guard lines.count <= maxLines + 1, lines.last?.isEmpty == true else { return false }
        let records = lines.dropLast().map(String.init)
        return continuityPlaced(records) && records.allSatisfy { record in
            audioLines.contains { record.range(of: $0, options: .regularExpression) != nil }
        }
    }
}
