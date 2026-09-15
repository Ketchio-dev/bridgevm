/// Local submission outcomes do not establish guest delivery or consumption.
enum HvfTextInputSubmission: Equatable {
    case refused
    case acceptedForProcessing
    /// The existing ASCII path attempted its writes without an acknowledgment.
    case legacyAttempted
}

@MainActor
enum HvfKeyboardDraft {
    static func submit(_ draft: inout String, to session: HvfEngineSession) {
        switch session.sendText(draft) {
        case .refused: return
        case .acceptedForProcessing, .legacyAttempted: draft = ""
        }
    }
}
