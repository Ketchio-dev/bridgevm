import Foundation

extension HvfEngineSession {
    func clipboardPaste(base64: String, preferRelay: Bool = false) -> HvfClipboardPaste {
        // An attachment may still use an older helper. An owned boot starts this app's relay.
        let transport: HvfClipboardPaste.Transport = inputDriver.router.eligible
            ? (preferRelay ? .relay : .relayForLongText) : .shell
        return HvfClipboardPaste(base64: base64, now: Date(), transport: transport)
    }

    /// Live-display admission. False means a refused route, failed write, or full paste backlog.
    func sendDisplayInput(_ input: HvfLegacyPasteState.Input) -> Bool {
        guard legacyPaste.pending != nil else { return deliverDisplayInput(input) }
        if case let .text(text, draft) = input, let base64 = HvfTextInputPlan.clipboardBase64(for: text) {
            let request = clipboardPaste(base64: base64, preferRelay: draft?.multiline == true)
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

    /// Text other than printable ASCII reaches Windows only through the agent: typed on the ordered route,
    /// or as a clipboard paste, while a Stop waits for Windows too. Before the agent runs (the sign-in screen after
    /// a boot) it is refused, and on a secure desktop or after a request Windows never confirmed (`fallback`: the
    /// lock screen, a UAC prompt, basic input) it would go through the Windows clipboard. The display view then
    /// keeps the Mac's input source to Roman layouts.
    var guestTakesNonASCIIText: Bool {
        guard serviceStarted, guestTakesInput else { return false }
        return !inputDriver.router.fallback
    }

    /// Whether `text` sent now reaches the guest before a pointer event sent after it: on the ordered
    /// route, or as HID keys in input.ctl. A clipboard paste completes later and a press cancels it.
    func displayTextPrecedesPointer(_ text: String) -> Bool {
        guard legacyPaste.pending == nil else { return false }
        if inputDriver.router.eligible, !inputDriver.router.fallback, inputDriver.router.state == .ready { return true }
        return HvfTextInputPlan.clipboardBase64(for: text) == nil
    }

    @discardableResult
    private func deliverDisplayInput(_ input: HvfLegacyPasteState.Input) -> Bool {
        switch input {
        case let .key(action): return sendKey(action)
        case let .text(text, draft):
            let result = sendText(text, draft: draft)
            if result == .legacyAttempted { draft?.onSent?() }
            return result != .refused
        }
    }
}
