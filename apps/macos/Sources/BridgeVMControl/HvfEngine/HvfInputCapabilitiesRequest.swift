import Foundation

/// Negotiates encoding support only, not foreground access or text consumption.
struct HvfInputCapabilitiesRequest {
    enum Failure: Equatable { case expired, restarted, unsupported, invalidReceipt }
    enum Outcome: Equatable { case supported, failed(Failure) }
    var command: String { receipt.command }
    private var receipt: HvfInputCapabilityReceipt
    private let deadline: Date
    private var finished = false
    private var supported = false

    init(now: Date, id: UUID = UUID()) {
        receipt = HvfInputCapabilityReceipt(id: id)
        deadline = now.addingTimeInterval(30)
    }

    func supports(_ event: HvfOrderedInputQueue.Event) -> Bool { supported && receipt.supports(event) }

    mutating func consume(lines: [String], now: Date) -> Outcome? {
        guard !finished else { return nil }
        guard now < deadline else { return finish(.failed(.expired)) }
        let lines = lines.map { $0.hasSuffix("\r") ? String($0.dropLast()) : $0 }
        if lines.contains(where: {
            $0.hasPrefix("BVAGENT READY") || $0.hasPrefix("BVAGENT re-READY") ||
            $0.hasPrefix("BVAGENT SERVICE start") || HvfGuestResetRecord.matches($0)
        }) { return finish(.failed(.restarted)) }
        for line in lines {
            if let outcome = receipt.consume(line) { return finish(outcome) }
        }
        return nil
    }

    private mutating func finish(_ outcome: Outcome) -> Outcome {
        finished = true
        supported = outcome == .supported
        return outcome
    }
}
