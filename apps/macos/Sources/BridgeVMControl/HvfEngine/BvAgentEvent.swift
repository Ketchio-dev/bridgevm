import Foundation

enum BvAgentDirection: String, Equatable {
    case hostToGuest
    case guestToHost
}

enum BvAgentShareKind: String, Equatable {
    case hostToGuest
    case guestToHost
    case delete
}

enum BvAgentEvent: Equatable, Identifiable {
    case ready(host: String, tMs: Int)
    case serviceStart(tMs: Int)
    case aliveHeartbeat(tMs: Int)
    /// `copy` numbers the clipboard copy the line is about (the probe's ` copy=N`).
    case clipSync(direction: BvAgentDirection, bytes: Int, tMs: Int, copy: Int? = nil)
    case shareEvent(kind: BvAgentShareKind, path: String, bytes: Int?, tMs: Int)
    case overdue(kind: String, awaitingReply: Bool, tMs: Int)
    case commandOutput(label: String, body: String)
    /// Structured host storage diagnostic, not the human-readable error log.
    case diskDurabilityFailed
    case diskFinalSaveFailed
    case unknown(String)

    var id: String { "\(Date().timeIntervalSince1970)-\(displayText)" }

    var displayText: String {
        switch self {
        case let .ready(host, tMs): return "READY host=\(host) t=\(tMs)"
        case let .serviceStart(tMs): return "SERVICE start t=\(tMs)"
        case let .aliveHeartbeat(tMs): return "SERVICE alive t=\(tMs)"
        case let .clipSync(direction, bytes, tMs, _): return "CLIPSYNC \(direction.rawValue) bytes=\(bytes) t=\(tMs)"
        case let .shareEvent(kind, path, bytes, tMs):
            let byteText = bytes.map { " bytes=\($0)" } ?? ""
            return "SHARE \(kind.rawValue) \(path)\(byteText) t=\(tMs)"
        case let .overdue(kind, awaitingReply, tMs):
            return "SERVICE overdue \(kind) awaiting-reply=\(awaitingReply) t=\(tMs)"
        case let .commandOutput(label, body): return "CMD \(label)\n\(body)"
        case .diskDurabilityFailed: return "BVSTORAGE durability-failed"
        case .diskFinalSaveFailed: return "BVSTORAGE final-save-failed"
        case let .unknown(line): return line
        }
    }

    /// Parses a run.log as it grows. A command's output can reach the reader over several
    /// polls (each reads at most 1 MiB), so an open command stays open until its END line.
    struct Stream {
        private var commandLabel: String?
        private var commandBody: [String] = []

        /// The events these lines complete.
        mutating func parse(_ lines: [String]) -> [BvAgentEvent] {
            var events: [BvAgentEvent] = []
            for line in lines {
                if let label = commandLabel {
                    if line == "BVAGENT END \(label)" {
                        events += finish()
                    } else {
                        commandBody.append(line)
                    }
                } else if let label = BvAgentEvent.commandStartLabel(line) {
                    commandLabel = label
                } else {
                    events.append(BvAgentEvent.parseSingle(line))
                }
            }
            return events
        }

        /// The open command's output so far, if a command is open.
        mutating func finish() -> [BvAgentEvent] {
            guard let label = commandLabel else { return [] }
            defer { commandLabel = nil; commandBody = [] }
            return [.commandOutput(label: label, body: commandBody.joined(separator: "\n"))]
        }
    }

    /// Complete lines; a command without its END line ends with them.
    static func parse(lines: [String]) -> [BvAgentEvent] {
        var stream = Stream()
        return stream.parse(lines) + stream.finish()
    }

    private static func parseSingle(_ line: String) -> BvAgentEvent {
        if line == "BVSTORAGE durability-failed" || line == "BVSTORAGE durability-failed\r" {
            return .diskDurabilityFailed
        }
        if line == "BVSTORAGE final-save-failed" || line == "BVSTORAGE final-save-failed\r" {
            return .diskFinalSaveFailed
        }
        guard line.hasPrefix("BVAGENT ") else { return .unknown(line) }
        if line.hasPrefix("BVAGENT READY host="), let tMs = tMs(in: line) {
            let prefix = "BVAGENT READY host="
            let rest = String(line.dropFirst(prefix.count))
            let host = rest.components(separatedBy: " t=").first ?? rest
            return .ready(host: host, tMs: tMs)
        }
        if line.hasPrefix("BVAGENT SERVICE start"), let tMs = tMs(in: line) {
            return .serviceStart(tMs: tMs)
        }
        if line.hasPrefix("BVAGENT SERVICE alive"), let tMs = tMs(in: line) {
            return .aliveHeartbeat(tMs: tMs)
        }
        if line.hasPrefix("BVAGENT SERVICE overdue "),
           let tMs = tMs(in: line),
           let awaitingReply = boolValue(after: "awaiting-reply=", in: line) {
            let body = String(line.dropFirst("BVAGENT SERVICE overdue ".count))
            let kind = body.components(separatedBy: " awaiting-reply=").first ?? body
            return .overdue(kind: kind, awaitingReply: awaitingReply, tMs: tMs)
        }
        if line.hasPrefix("BVAGENT SERVICE timeout "), let tMs = tMs(in: line) {
            let body = String(line.dropFirst("BVAGENT SERVICE timeout ".count))
            let kind = body.components(separatedBy: " t=").first ?? body
            return .overdue(kind: kind, awaitingReply: false, tMs: tMs)
        }
        if line.hasPrefix("BVAGENT CLIPSYNC "), let event = parseClipSync(line) {
            return event
        }
        if line.hasPrefix("BVAGENT SHARE "), let event = parseShare(line) {
            return event
        }
        return .unknown(line)
    }

    private static func commandStartLabel(_ line: String) -> String? {
        guard line.hasPrefix("BVAGENT CMD ") else { return nil }
        let body = String(line.dropFirst("BVAGENT CMD ".count))
        guard let range = body.range(of: " exit=") else { return nil }
        return String(body[..<range.lowerBound])
    }

    private static func parseClipSync(_ line: String) -> BvAgentEvent? {
        let direction: BvAgentDirection
        if line.contains(" host->guest ") {
            direction = .hostToGuest
        } else if line.contains(" guest->host ") {
            direction = .guestToHost
        } else {
            return nil
        }
        guard let bytes = value(after: "bytes=", in: line), let tMs = tMs(in: line) else { return nil }
        return .clipSync(direction: direction, bytes: bytes, tMs: tMs, copy: value(after: " copy=", in: line))
    }

    private static func parseShare(_ line: String) -> BvAgentEvent? {
        guard let tMs = tMs(in: line) else { return nil }
        let body = String(line.dropFirst("BVAGENT SHARE ".count))
        let parts = body.split(separator: " ", omittingEmptySubsequences: true).map(String.init)
        if parts.count >= 3, parts[0] == "del" {
            return .shareEvent(kind: .delete, path: parts[2], bytes: nil, tMs: tMs)
        }
        let kind: BvAgentShareKind
        switch parts.first {
        case "host->guest": kind = .hostToGuest
        case "guest->host": kind = .guestToHost
        default: return nil
        }
        guard let file = shareFile(in: line, after: "BVAGENT SHARE \(parts[0]) ") else { return nil }
        return .shareEvent(kind: kind, path: file.name, bytes: file.bytes, tMs: tMs)
    }

    /// The name and size in `<prefix><name> bytes=<ASCII digits>[ ...]`. A name may hold spaces,
    /// even ` bytes=`: it runs to the last one.
    static func shareFile(in line: String, after prefix: String) -> (name: String, bytes: Int)? {
        guard line.hasPrefix(prefix) else { return nil }
        // Only after the prefix: a line that is all marker must not slice backwards.
        let rest = line.dropFirst(prefix.count)
        guard let marker = rest.range(of: " bytes=", options: .backwards) else { return nil }
        let tail = rest[marker.upperBound...]
        let digits = tail.prefix { $0.isASCII && $0.isNumber }
        let after = tail.dropFirst(digits.count)
        guard let bytes = Int(digits), after.isEmpty || after.hasPrefix(" ") else { return nil }
        let name = rest[..<marker.lowerBound]
        return name.isEmpty ? nil : (String(name), bytes)
    }

    private static func tMs(in line: String) -> Int? {
        value(after: " t=", in: line)
    }

    private static func value(after marker: String, in line: String) -> Int? {
        guard let range = line.range(of: marker) else { return nil }
        let tail = line[range.upperBound...]
        let digits = tail.prefix { $0.isNumber }
        return Int(digits)
    }

    private static func boolValue(after marker: String, in line: String) -> Bool? {
        guard let range = line.range(of: marker) else { return nil }
        let value = line[range.upperBound...].prefix { !$0.isWhitespace }
        switch value {
        case "true": return true
        case "false": return false
        default: return nil
        }
    }
}
