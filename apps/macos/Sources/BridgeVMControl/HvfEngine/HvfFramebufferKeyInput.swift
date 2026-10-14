#if canImport(AppKit)
import AppKit

@MainActor
enum HvfFramebufferKeyInput {
    static func send(_ event: NSEvent, to session: HvfEngineSession, refused: () -> Void = { NSSound.beep() }) -> Bool {
        switch HvfHostKeyCommand.resolve(event: event) {
        case let .key(action): if !session.sendDisplayInput(.key(action)) { refused() }
        case let .text(text): if !session.sendDisplayInput(.text(text)) { refused() }
        case .ignored: return false
        }
        return true
    }
}
#endif
