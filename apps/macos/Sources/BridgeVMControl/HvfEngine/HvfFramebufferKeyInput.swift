#if canImport(AppKit)
import AppKit

@MainActor
enum HvfFramebufferKeyInput {
    static func send(_ event: NSEvent, to session: HvfEngineSession, refused: () -> Void = { NSSound.beep() }) -> Bool {
        switch HvfHostKeyCommand.resolve(event: event, mapping: session.keyMapping) {
        case let .key(action): if !HvfDisplayKeyRepeat.send(action, repeat: event.isARepeat, to: session) { refused() }
        case let .text(text): if !session.sendDisplayInput(.text(text)) { refused() }
        case .ignored: return false
        }
        return true
    }

    /// A completed bare modifier tap (HvfModifierTap), only while the guest takes generic chords.
    static func sendTap(_ action: String, to session: HvfEngineSession, refused: () -> Void = { NSSound.beep() }) {
        guard session.keyMapping.modifierChords else { return }
        if !session.sendDisplayInput(.key(action)) { refused() }
    }
}

extension HvfEngineSession {
    /// The VM's 'Command 키' setting, and whether generic chords reach the guest on the current route.
    var keyMapping: HvfHostKeyMapping {
        HvfHostKeyMapping(commandKey: config.commandKeyMode, modifierChords: inputDriver.router.modifierChords)
    }
}
#endif
