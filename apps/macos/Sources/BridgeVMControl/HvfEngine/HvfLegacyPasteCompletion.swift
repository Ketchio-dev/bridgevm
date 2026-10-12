import Foundation

extension HvfEngineSession {
    func finishLegacyPaste(lines: [String]) {
        guard let ready = legacyPaste.pending?.consume(lines: lines, now: Date()) else { return }
        let backlog = legacyPaste.finish()
        if ready, case .connected = connectionState {
            if appendLiveInput("KEY ctrl+v") { replayDisplayInput(backlog) }
            else { append(.unknown("display input replay stopped" + HvfLegacyPasteState.discarded(backlog))) }
        } else { append(.unknown("clipboard paste failed, expired or canceled" + HvfLegacyPasteState.discarded(backlog))) }
    }
}
