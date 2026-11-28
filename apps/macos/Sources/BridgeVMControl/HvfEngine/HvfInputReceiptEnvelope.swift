/// A line of guest text in run.log: the probe prints each line of a command's output, a share listing and the
/// serial tail after `| ` (guest_text.rs guest_lines), so none can pass for a host record such as `BVAGENT END`.
/// Host records are compared on the raw line; only body text is read through `text`. An older helper prints the
/// lines bare, which read the same.
enum HvfGuestLine {
    static let prefix = "| "

    static func text(_ line: String) -> String {
        let scalars = line.unicodeScalars
        return scalars.starts(with: prefix.unicodeScalars) ? String(scalars.dropFirst(prefix.unicodeScalars.count)) : line
    }
}

/// Supports legacy and payload-free receipts without mixing their envelopes.
struct HvfInputReceiptEnvelope {
    private let labels: [String]
    private var selected: String?

    init(command: String, redactedLabel: String? = nil) {
        labels = [command, redactedLabel ?? command.split(separator: " ", maxSplits: 2).prefix(2).joined(separator: " ")]
    }

    mutating func header(_ line: String) -> Bool? {
        for label in labels {
            let prefix = "BVAGENT CMD \(label) exit="
            if line.hasPrefix(prefix) {
                selected = label
                return line == prefix + "0"
            }
        }
        return nil
    }

    func isEnd(_ line: String) -> Bool { labels.contains { line == "BVAGENT END \($0)" } }
    func matchesEnd(_ line: String) -> Bool {
        selected.map { line == "BVAGENT END \($0)" } ?? false
    }
}
