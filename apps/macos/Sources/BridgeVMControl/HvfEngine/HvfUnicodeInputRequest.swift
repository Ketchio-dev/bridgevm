import Foundation

/// Confirms guest input-stream insertion, not application text consumption.
struct HvfUnicodeInputRequest {
    /// `notInserted`: the agent's SendInput inserted none of the request's events, as on a Windows secure
    /// desktop (lock or sign-in screen, UAC prompt); unlike any other failure nothing of it reached Windows.
    enum Failure: Equatable { case expired, restarted, guestRejected, invalidReceipt, notInserted }
    enum Outcome: Equatable { case inserted, failed(Failure) }
    private enum Phase { case header, marker, end, rejected, finished }

    let command: String
    let insertedEventCount: Int
    private let deadline: Date
    private let marker: String
    private let markerPrefix: String
    private let failurePrefix: String
    private var phase = Phase.header
    private var envelope: HvfInputReceiptEnvelope

    init?(event: HvfOrderedInputQueue.Event, now: Date, id: UUID = UUID()) {
        guard let encoding = HvfGuestInputEncoding(event) else { return nil }
        command = "\(encoding.verb) \(id.uuidString) \(encoding.base64)"
        envelope = HvfInputReceiptEnvelope(command: command)
        insertedEventCount = encoding.insertedEventCount
        markerPrefix = "BVINPUT_INSERTED \(id.uuidString) "
        marker = markerPrefix + String(insertedEventCount)
        failurePrefix = "BVINPUT_FAILED \(id.uuidString) "
        deadline = now.addingTimeInterval(30)
    }

    mutating func consume(lines: [String], now: Date) -> Outcome? {
        guard phase != .finished else { return nil }
        guard now < deadline else { return finish(.failed(.expired)) }
        let lines = lines.map { $0.hasSuffix("\r") ? String($0.dropLast()) : $0 }
        // Inspect the entire batch before accepting an earlier completion in it.
        if lines.contains(where: {
            $0.hasPrefix("BVAGENT READY") || $0.hasPrefix("BVAGENT re-READY") ||
            $0.hasPrefix("BVAGENT SERVICE start") || HvfGuestResetRecord.matches($0)
        }) { return finish(.failed(.restarted)) }
        for line in lines {
            if let accepted = envelope.header(line) {
                guard phase == .header else { return finish(.failed(.invalidReceipt)) }
                phase = accepted ? .marker : .rejected
            } else if phase == .rejected {
                // bvagent-input.ps1: `BVINPUT_FAILED <id> <exception message>`, then END; SendInput's shortfall
                // reads `... unicode-input-incomplete inserted=<n> requested=<m>` (bvagent-unicode-input.cs).
                if line.hasPrefix(failurePrefix) {
                    return finish(.failed(Self.insertedNothing(line) ? .notInserted : .guestRejected))
                }
                if envelope.isEnd(line) { return finish(.failed(.guestRejected)) }
            } else if line.hasPrefix(markerPrefix) {
                guard phase == .marker, line == marker else { return finish(.failed(.invalidReceipt)) }
                phase = .end
            } else if envelope.isEnd(line) {
                guard phase == .end, envelope.matchesEnd(line) else { return finish(.failed(.invalidReceipt)) }
                return finish(.inserted)
            }
        }
        return nil
    }

    private static func insertedNothing(_ line: String) -> Bool {
        guard let range = line.range(of: "unicode-input-incomplete inserted=0 requested=") else { return false }
        let requested = line[range.upperBound...].prefix { $0.isASCII && $0.isNumber }
        return Int(requested).map { $0 > 0 } ?? false
    }

    private mutating func finish(_ result: Outcome) -> Outcome {
        phase = .finished
        return result
    }
}
