import Foundation

/// A legacy clipboard paste awaiting its guest marker, and the live-display input typed behind it.
///
/// Nothing may overtake the paste's delayed `KEY ctrl+v`, so the session refuses input meanwhile.
/// The draft box keeps refused text; a display keystroke has nowhere to stay, so it waits here and
/// replays in order after that `KEY ctrl+v`. Any other end of the paste (failure, expiry, guest
/// reset, pointer press, target change, stop, attach, owned boot) discards it with the paste.
struct HvfLegacyPasteState {
    enum Input: Equatable { case key(String), text(String) }

    /// Entries after merging; far more than a paste round trip of typing (a paste expires in 30 s).
    static let maximumBacklog = 256

    var pending: HvfClipboardPaste?
    private(set) var backlog: [Input] = []

    /// Adjacent text merges into one later paste while that paste still fits the guest cmd.exe
    /// line, and therefore the 64 KiB control line. False when a new entry would exceed the bound.
    mutating func hold(_ input: Input) -> Bool {
        if case let .text(next) = input, case let .text(last)? = backlog.last, Self.fits(last + next) {
            backlog[backlog.count - 1] = .text(last + next)
            return true
        }
        guard backlog.count < Self.maximumBacklog else { return false }
        backlog.append(input)
        return true
    }

    /// Ends the pending paste and returns its backlog; only a successful `KEY ctrl+v` replays it.
    mutating func finish() -> [Input] {
        defer { self = Self() }
        return backlog
    }

    /// The log line for a paste dropped before its `KEY ctrl+v`; nil when none was pending.
    mutating func cancel(_ reason: String) -> String? {
        guard pending != nil else { return nil }
        return "clipboard paste canceled: \(reason)" + Self.discarded(finish())
    }

    static func discarded(_ backlog: some Collection) -> String {
        backlog.isEmpty ? "" : "; discarded \(backlog.count) held display input entries"
    }

    private static func fits(_ text: String) -> Bool {
        let paste = HvfClipboardPaste(base64: Data(text.utf8).base64EncodedString(), now: Date())
        return paste.command.utf8.count <= HvfClipboardPaste.maximumCommandCharacters
    }
}

extension HvfEngineSession {
    /// Live-display input. False only when it had to wait behind a paste and the backlog is full.
    func sendDisplayInput(_ input: HvfLegacyPasteState.Input) -> Bool {
        guard legacyPaste.pending != nil else { deliverDisplayInput(input); return true }
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
        case let .key(action): sendKey(action); return true
        case let .text(text): return sendText(text) != .refused
        }
    }
}
