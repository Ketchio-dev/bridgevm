import Foundation

extension HvfEngineSession {
    /// Live-display admission. False means a refused route, failed write, or full paste backlog.
    func sendDisplayInput(_ input: HvfLegacyPasteState.Input) -> Bool {
        guard legacyPaste.pending != nil else { return deliverDisplayInput(input) }
        if case let .text(text) = input, let base64 = HvfTextInputPlan.clipboardBase64(for: text) {
            let request = HvfClipboardPaste(base64: base64, now: Date())
            guard request.fitsGuestCommandLine(reportingTo: self) else { return false }
        }
        if legacyPaste.hold(input) { return true }
        append(.unknown("display input refused: \(HvfLegacyPasteState.maximumBacklog) entries already wait for a clipboard paste"))
        return false
    }

    /// Runs right after the acknowledged paste's `KEY ctrl+v`. Text that starts another paste holds
    /// the rest behind it. A refused replay drops the rest: later keys such as Backspace or Enter
    /// would otherwise act on text the guest never received.
    func replayDisplayInput(_ backlog: [HvfLegacyPasteState.Input]) {
        for (index, input) in backlog.enumerated() {
            if legacyPaste.pending != nil {
                for rest in backlog[index...] { _ = legacyPaste.hold(rest) } // never more than it held
                return
            }
            guard deliverDisplayInput(input) else {
                append(.unknown("display input replay stopped" + HvfLegacyPasteState.discarded(backlog[(index + 1)...])))
                return
            }
        }
    }

    @discardableResult
    private func deliverDisplayInput(_ input: HvfLegacyPasteState.Input) -> Bool {
        switch input {
        case let .key(action): return sendKey(action)
        case let .text(text): return sendText(text) != .refused
        }
    }
}
