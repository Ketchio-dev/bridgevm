import Foundation

/// The records after a bound final report's footer: only host teardown records
/// may follow it, and none of them is a footer, so the last footer is the
/// report's own. Guest copies before the report or inside its counted tail are
/// not among them. scripts/live-gates/hvf_terminal_evidence.py reads the same.
extension HvfTerminalReport {
    static func hostTail(in data: Data) -> Data? {
        guard stop(in: data) != nil,
              let footer = data.range(of: Data("\n--- end ---\n".utf8), options: .backwards) else { return nil }
        return Data(data[footer.upperBound...])
    }
}
