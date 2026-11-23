import Foundation

/// Negotiates encoding support only, not foreground access or text consumption.
struct HvfInputCapabilitiesRequest {
    enum Failure: Equatable { case restarted, unsupported, invalidReceipt }
    enum Outcome: Equatable { case supported, failed(Failure) }
    var command: String { receipt.command }
    private var receipt: HvfInputCapabilityReceipt
    private var finished = false
    private var supported = false

    init(id: UUID = UUID()) {
        receipt = HvfInputCapabilityReceipt(id: id)
    }

    func supports(_ event: HvfOrderedInputQueue.Event) -> Bool { supported && receipt.supports(event) }
    var modifierChords: Bool { supported && receipt.modifierChords }
    var scrollUnits: Bool { supported && receipt.scrollUnits }
    var pointerModifiers: Bool { supported && receipt.pointerModifiers }
    var middleButton: Bool { supported && receipt.middleButton }
    var caret: Bool { supported && receipt.caret }
    var keyHold: Bool { supported && receipt.keyHold }
    var queuedTextDispatch: Bool { supported && receipt.queuedTextDispatch }

    mutating func consume(lines: [String]) -> Outcome? {
        guard !finished else { return nil }
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
