@MainActor
enum HvfKeyboardDraft {
    @discardableResult
    static func submit(_ draft: inout String, to session: HvfEngineSession,
                       onPasteSent: (@MainActor () -> Void)? = nil) -> HvfTextInputSubmission {
        guard session.submittedKeyboardDraft == nil else {
            session.append(.unknown("text input refused: keyboard draft pending"))
            return .refused
        }
        if draft.utf8.contains(where: { $0 == 10 || $0 == 13 }) {
            let delivery = HvfTextDraftDelivery(onSent: onPasteSent)
            session.submittedKeyboardDraft = delivery
            return session.sendText(draft, draft: delivery)
        }
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
