@MainActor
enum HvfKeyboardDraft {
    @discardableResult
    static func submit(_ draft: inout String, to session: HvfEngineSession) -> HvfTextInputSubmission {
        let result = session.sendText(draft)
        switch result {
        case .refused: break
        case .acceptedForProcessing, .legacyAttempted: draft = ""
        }
        return result
    }
}
