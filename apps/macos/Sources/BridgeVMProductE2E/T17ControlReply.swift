import Foundation

enum T17ControlReply {
    enum Outcome { case pending, succeeded, rejected }

    static func succeeded(tail: String, command: String, marker: String) -> Bool {
        outcome(tail: tail, command: command, marker: marker) == .succeeded
    }

    static func outcome(tail: String, command: String, marker: String) -> Outcome {
        guard !marker.isEmpty else { return .pending }
        let lines = tail.replacingOccurrences(of: "\r\n", with: "\n")
            .components(separatedBy: "\n")
        if command.hasPrefix("CLIPSET ") {
            guard marker == "OK CLIPSET" else { return .pending }
            var labels = ["BVAGENT \(command)"]
            if let bytes = Data(base64Encoded: String(command.dropFirst("CLIPSET ".count))) {
                labels.append("BVAGENT CLIPSET \(T17ClipboardFingerprint.fields(of: bytes))")
            }
            for label in labels {
                let error = "\(label) -> ERR"
                if lines.contains(where: { $0 == error || $0.hasPrefix(error + " ") }) { return .rejected }
            }
            return labels.contains(where: { lines.contains("\($0) -> OK CLIPSET") }) ? .succeeded : .pending
        }
        // The probe prints a CLIPGET's byte length and SHA-256 prefix, never the text.
        if command == "CLIPGET" {
            let fingerprint = "BVAGENT CLIP \(command) \(T17ClipboardFingerprint.fields(of: marker))"
            return lines.contains(fingerprint) ? .succeeded : .pending
        }
        let headerPrefix = "BVAGENT CMD \(command) exit="
        let header = headerPrefix + "0", endLine = "BVAGENT END \(command)"
        if let failed = lines.firstIndex(where: { $0.hasPrefix(headerPrefix) && $0 != header }) {
            // A log poll can end halfway through the exit code or before its END.
            guard let code = Int32(lines[failed].dropFirst(headerPrefix.count)), code != 0,
                  lines.dropFirst(failed + 1).contains(endLine) else { return .pending }
            return .rejected
        }
        guard let start = lines.firstIndex(of: header),
              let end = lines.indices.first(where: { $0 > start && lines[$0] == endLine })
        else { return .pending }
        return lines[(start + 1)..<end].joined(separator: "\n").contains(marker) ? .succeeded : .pending
    }
}
