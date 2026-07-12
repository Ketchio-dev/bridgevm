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
    case clipSync(direction: BvAgentDirection, bytes: Int, tMs: Int)
    case shareEvent(kind: BvAgentShareKind, path: String, bytes: Int?, tMs: Int)
    case overdue(kind: String, awaitingReply: Bool, tMs: Int)
    case commandOutput(label: String, body: String)
    case unknown(String)

    var id: String { "\(Date().timeIntervalSince1970)-\(displayText)" }

    var displayText: String {
        switch self {
        case let .ready(host, tMs): return "READY host=\(host) t=\(tMs)"
        case let .serviceStart(tMs): return "SERVICE start t=\(tMs)"
        case let .aliveHeartbeat(tMs): return "SERVICE alive t=\(tMs)"
        case let .clipSync(direction, bytes, tMs): return "CLIPSYNC \(direction.rawValue) bytes=\(bytes) t=\(tMs)"
        case let .shareEvent(kind, path, bytes, tMs):
            let byteText = bytes.map { " bytes=\($0)" } ?? ""
            return "SHARE \(kind.rawValue) \(path)\(byteText) t=\(tMs)"
        case let .overdue(kind, awaitingReply, tMs):
            return "SERVICE overdue \(kind) awaiting-reply=\(awaitingReply) t=\(tMs)"
        case let .commandOutput(label, body): return "CMD \(label)\n\(body)"
        case let .unknown(line): return line
        }
    }

    static func parse(lines: [String]) -> [BvAgentEvent] {
        var events: [BvAgentEvent] = []
        var commandLabel: String?
        var commandBody: [String] = []

        func flushCommand() {
            guard let label = commandLabel else { return }
            events.append(.commandOutput(label: label, body: commandBody.joined(separator: "\n")))
            commandLabel = nil
            commandBody = []
        }

        for line in lines {
            if let label = commandLabel {
                if line == "BVAGENT END \(label)" {
                    flushCommand()
                } else {
                    commandBody.append(line)
                }
                continue
            }

            if let label = commandStartLabel(line) {
                commandLabel = label
                commandBody = []
                continue
            }

            events.append(parseSingle(line))
        }
        flushCommand()
        return events
    }

    private static func parseSingle(_ line: String) -> BvAgentEvent {
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
        return .clipSync(direction: direction, bytes: bytes, tMs: tMs)
    }

    private static func parseShare(_ line: String) -> BvAgentEvent? {
        guard let tMs = tMs(in: line) else { return nil }
        let body = String(line.dropFirst("BVAGENT SHARE ".count))
        let parts = body.split(separator: " ", omittingEmptySubsequences: true).map(String.init)
        if parts.count >= 3, parts[0] == "del" {
            return .shareEvent(kind: .delete, path: parts[2], bytes: nil, tMs: tMs)
        }
        guard parts.count >= 2 else { return nil }
        let kind: BvAgentShareKind
        if parts[0] == "host->guest" {
            kind = .hostToGuest
        } else if parts[0] == "guest->host" {
            kind = .guestToHost
        } else {
            return nil
        }
        return .shareEvent(kind: kind, path: parts[1], bytes: value(after: "bytes=", in: line), tMs: tMs)
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
