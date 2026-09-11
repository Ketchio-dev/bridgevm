import Foundation

/// Confirms guest input-stream insertion, not application text consumption.
struct HvfUnicodeInputRequest {
    enum Failure: Equatable { case expired, restarted, guestRejected, invalidReceipt }
    enum Outcome: Equatable { case inserted, failed(Failure) }
    private enum Phase { case header, marker, end, finished }

    let command: String
    let insertedEventCount: Int
    private let deadline: Date
    private let marker: String
    private let markerPrefix: String
    private var phase = Phase.header

    init?(event: HvfOrderedInputQueue.Event, now: Date, id: UUID = UUID()) {
        guard let encoding = HvfGuestInputEncoding(event) else { return nil }
        command = "\(encoding.verb) \(id.uuidString) \(encoding.base64)"
        insertedEventCount = encoding.insertedEventCount
        markerPrefix = "BVINPUT_INSERTED \(id.uuidString) "
        marker = markerPrefix + String(insertedEventCount)
        deadline = now.addingTimeInterval(30)
    }

    mutating func consume(lines: [String], now: Date) -> Outcome? {
        guard phase != .finished else { return nil }
        guard now < deadline else { return finish(.failed(.expired)) }
        let lines = lines.map { $0.hasSuffix("\r") ? String($0.dropLast()) : $0 }
        // Inspect the entire batch before accepting an earlier completion in it.
        if lines.contains(where: {
            $0.hasPrefix("BVAGENT READY") || $0.hasPrefix("BVAGENT re-READY") ||
            $0.hasPrefix("BVAGENT SERVICE start") || $0.hasPrefix("PSCI_SYSTEM_RESET")
        }) { return finish(.failed(.restarted)) }
        let header = "BVAGENT CMD \(command) exit="
        let end = "BVAGENT END \(command)"
        for line in lines {
            if line.hasPrefix(header) {
                guard phase == .header else { return finish(.failed(.invalidReceipt)) }
                guard line == header + "0" else { return finish(.failed(.guestRejected)) }
                phase = .marker
            } else if line.hasPrefix(markerPrefix) {
                guard phase == .marker, line == marker else { return finish(.failed(.invalidReceipt)) }
                phase = .end
            } else if line == end {
                guard phase == .end else { return finish(.failed(.invalidReceipt)) }
                return finish(.inserted)
            }
        }
        return nil
    }

    private mutating func finish(_ result: Outcome) -> Outcome {
        phase = .finished
        return result
    }
}
