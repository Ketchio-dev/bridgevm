/// macOS repeats a held key on its own clock, not the guest's. A repeat is admitted only when
/// nothing waits unsent ahead of it; otherwise it is absorbed, so a slow guest route cannot
/// replay a held Backspace or arrow after release. A key's first press is never absorbed.
@MainActor
enum HvfDisplayKeyRepeat {
    static func send(_ action: String, repeat isRepeat: Bool, to session: HvfEngineSession) -> Bool {
        if isRepeat, !session.admitsDisplayKeyRepeat { return true }
        return session.sendDisplayInput(.key(action))
    }
}

extension HvfEngineSession {
    /// The ordered route holds at most the ticket in flight plus one; legacy writes never wait.
    var admitsDisplayKeyRepeat: Bool { legacyPaste.pending == nil && inputDriver.router.count <= 1 }
}
