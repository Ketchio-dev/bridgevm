import Foundation

enum T17ControlReply {
    static func succeeded(tail: String, command: String, marker: String) -> Bool {
        guard !marker.isEmpty else { return false }
        let lines = tail.replacingOccurrences(of: "\r\n", with: "\n")
            .components(separatedBy: "\n")
        if command.hasPrefix("CLIPSET ") {
            return marker == "OK CLIPSET"
                && lines.contains("BVAGENT \(command) -> OK CLIPSET")
                && !lines.contains(where: { $0.hasPrefix("BVAGENT \(command) -> ERR") })
        }
        let header: String
        if command == "CLIPGET" {
            header = "BVAGENT CLIP \(command)"
        } else {
            header = "BVAGENT CMD \(command) exit=0"
            if lines.contains(where: { $0.hasPrefix("BVAGENT CMD \(command) exit=") && $0 != header }) {
                return false
            }
        }
        guard let start = lines.firstIndex(of: header),
              let end = lines.indices.first(where: { $0 > start && lines[$0] == "BVAGENT END \(command)" })
        else { return false }
        return lines[(start + 1)..<end].joined(separator: "\n").contains(marker)
    }
}
