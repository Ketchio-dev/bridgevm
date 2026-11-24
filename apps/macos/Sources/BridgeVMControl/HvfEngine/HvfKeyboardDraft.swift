@MainActor
enum HvfKeyboardDraft {
    @discardableResult
    static func submit(_ draft: inout String, to session: HvfEngineSession,
                       onPasteSent: (@MainActor () -> Void)? = nil) -> HvfTextInputSubmission {
        let result = session.sendText(draft)
        switch result {
        case .refused: break
        case .acceptedForProcessing where session.legacyPaste.pending != nil:
            session.legacyPaste.pending?.onPasteSent = onPasteSent
        case .acceptedForProcessing, .legacyAttempted: draft = ""
        }
        return result
    }
}
