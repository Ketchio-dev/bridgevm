import Foundation

enum T17ControlReply {
    static func succeeded(tail: String, command: String, marker: String) -> Bool {
        guard !marker.isEmpty else { return false }
        let lines = tail.replacingOccurrences(of: "\r\n", with: "\n")
            .components(separatedBy: "\n")
        if command.hasPrefix("CLIPSET ") {
            guard marker == "OK CLIPSET" else { return false }
            var labels = ["BVAGENT \(command)"]
            if let bytes = Data(base64Encoded: String(command.dropFirst("CLIPSET ".count))) {
                labels.append("BVAGENT CLIPSET \(T17ClipboardFingerprint.fields(of: bytes))")
            }
            return labels.contains(where: { lines.contains("\($0) -> OK CLIPSET") })
                && !lines.contains(where: { line in labels.contains { line.hasPrefix("\($0) -> ERR") } })
        }
        // The probe prints a CLIPGET's byte length and SHA-256 prefix, never the text.
        if command == "CLIPGET" {
            return lines.contains("BVAGENT CLIP \(command) \(T17ClipboardFingerprint.fields(of: marker))")
        }
        let header = "BVAGENT CMD \(command) exit=0"
        if lines.contains(where: { $0.hasPrefix("BVAGENT CMD \(command) exit=") && $0 != header }) {
            return false
        }
        guard let start = lines.firstIndex(of: header),
              let end = lines.indices.first(where: { $0 > start && lines[$0] == "BVAGENT END \(command)" })
        else { return false }
        return lines[(start + 1)..<end].joined(separator: "\n").contains(marker)
    }
}
