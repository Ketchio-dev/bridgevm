import Foundation

extension HvfClipboardPaste {
    /// cmd.exe refuses command lines over 8,191 characters. The relay sends this command as
    /// `RUN <base64>` and bvagent.ps1 (Invoke-B64) runs it as `& cmd.exe /c $cmd`; Windows
    /// PowerShell quotes that spaced argument, so cmd.exe's line is
    /// `"C:\Windows\system32\cmd.exe" /c "<command>"`. Subtracting that whole 35-character
    /// wrapper fails closed whether or not cmd.exe counts its own image path.
    static let maximumCommandCharacters = 8_191 - #""C:\Windows\system32\cmd.exe" /c """#.count

    /// Refuses before the control write so the caller keeps the draft rather than losing the
    /// text to a guest failure. The command is ASCII: UTF-8 bytes equal cmd.exe characters.
    @MainActor func fitsGuestCommandLine(reportingTo session: HvfEngineSession) -> Bool {
        let length = command.utf8.count
        guard length > Self.maximumCommandCharacters else { return true }
        session.append(.unknown("clipboard paste refused: text too long for the guest cmd.exe command line "
            + "(\(length)/\(Self.maximumCommandCharacters) characters); send shorter text"))
        return false
    }
}
