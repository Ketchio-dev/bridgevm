import Foundation

/// Negotiates encoding support only, not foreground access or text consumption.
struct HvfInputCapabilitiesRequest {
    enum Failure: Equatable { case expired, restarted, unsupported, invalidReceipt }
    enum Outcome: Equatable { case supported, failed(Failure) }
    private enum Phase { case header, marker, end, finished }
    let command: String
    private let marker: String
    private let markerPrefix: String
    private let deadline: Date
    private var phase = Phase.header

    init(now: Date, id: UUID = UUID()) {
        command = "INPUTCAPS \(id.uuidString)"
        markerPrefix = "BVINPUT_CAPS \(id.uuidString) "
        marker = markerPrefix + "1 TEXTINPUT KEYINPUT POINTERINPUT 65536"
        deadline = now.addingTimeInterval(30)
    }

    mutating func consume(lines: [String], now: Date) -> Outcome? {
        guard phase != .finished else { return nil }
        guard now < deadline else { return finish(.failed(.expired)) }
        let lines = lines.map { $0.hasSuffix("\r") ? String($0.dropLast()) : $0 }
        if lines.contains(where: {
            $0.hasPrefix("BVAGENT READY") || $0.hasPrefix("BVAGENT re-READY") ||
            $0.hasPrefix("BVAGENT SERVICE start") || $0.hasPrefix("PSCI_SYSTEM_RESET")
        }) { return finish(.failed(.restarted)) }
        let header = "BVAGENT CMD \(command) exit="
        let end = "BVAGENT END \(command)"
        for line in lines {
            if line.hasPrefix(header) {
                guard phase == .header else { return finish(.failed(.invalidReceipt)) }
                guard line == header + "0" else { return finish(.failed(.unsupported)) }
                phase = .marker
            } else if line.hasPrefix(markerPrefix) {
                guard phase == .marker, line == marker else { return finish(.failed(.invalidReceipt)) }
                phase = .end
            } else if line == end {
                guard phase == .end else { return finish(.failed(.invalidReceipt)) }
                return finish(.supported)
            }
        }
        return nil
    }

    private mutating func finish(_ outcome: Outcome) -> Outcome {
        phase = .finished
        return outcome
    }
}
