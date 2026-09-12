/// Supports legacy and payload-free receipts without mixing their envelopes.
struct HvfInputReceiptEnvelope {
    private let labels: [String]
    private var selected: String?

    init(command: String) {
        labels = [command, command.split(separator: " ", maxSplits: 2).prefix(2).joined(separator: " ")]
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
