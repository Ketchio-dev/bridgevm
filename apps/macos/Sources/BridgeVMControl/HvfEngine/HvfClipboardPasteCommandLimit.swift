import Foundation

extension HvfClipboardPaste {
    /// Refuses before the control write so the caller keeps the draft rather than losing the
    /// text to a guest failure. CLIPPASTE uses the guest control line without a shell.
    @MainActor func fitsGuestCommandLine(reportingTo session: HvfEngineSession) -> Bool {
        let length = command.utf8.count
        let direct = command.hasPrefix("CLIPPASTE ")
        let limit = direct ? HvfGuestCommand.maximumBytes : Self.maximumCommandCharacters
        guard length > limit else { return true }
        let line = direct ? "guest control line" : "guest cmd.exe command line"
        session.append(.unknown("clipboard paste refused: text too long for the \(line) (\(length)/\(limit) characters); send shorter text"))
        return false
    }
}
