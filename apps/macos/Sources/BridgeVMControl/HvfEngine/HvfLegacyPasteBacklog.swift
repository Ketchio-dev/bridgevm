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
