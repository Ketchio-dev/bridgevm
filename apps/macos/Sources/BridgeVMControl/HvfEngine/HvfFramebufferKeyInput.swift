#if canImport(AppKit)
import AppKit

@MainActor
enum HvfFramebufferKeyInput {
    /// A key-down or key-up for the guest. A named key or chord is held from its key-down to its key-up
    /// (HvfEngineSessionHeldKeys.swift); text from the Mac's input method stays text. A Command chord stays a tap:
    /// the Mac sends no key-up for a key released while Command is down.
    static func send(_ event: NSEvent, to session: HvfEngineSession, refused: () -> Void = { NSSound.beep() }) -> Bool {
        guard event.type != .keyUp else { session.sendKeyUp(keyCode: event.keyCode); return true }
        let command = HvfHostKeyCommand.resolve(event: event, mapping: session.keyMapping)
        if case let .key(action) = command, !event.modifierFlags.contains(.command) {
            if !session.sendKeyDown(action, keyCode: event.keyCode, repeat: event.isARepeat) { refused() }
            return true
        }
        session.sendKeyUp(keyCode: event.keyCode) // a held key that now types text, nothing or a Command chord goes up
        switch command {
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
