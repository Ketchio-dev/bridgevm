@MainActor
enum HvfKeyboardDraft {
    @discardableResult
    static func submit(_ draft: inout String, to session: HvfEngineSession,
                       onSent: (@MainActor () -> Void)? = nil) -> HvfTextInputSubmission {
        guard session.submittedKeyboardDraft == nil else {
            let pending = session.legacyPaste.pending == nil ? "keyboard draft" : "clipboard paste"
            session.append(.unknown("text input refused: \(pending) pending"))
            return .refused
        }
        let multiline = draft.utf8.contains { $0 == 10 || $0 == 13 }
        let delivery = HvfTextDraftDelivery(multiline: multiline, onSent: onSent)
        session.submittedKeyboardDraft = delivery
        let result = session.sendText(draft, draft: delivery)
        if result == .legacyAttempted { draft = "" }
        return result
    }
}
