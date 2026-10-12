#if canImport(AppKit)
import AppKit
import SwiftUI

@MainActor
enum HvfFramebufferKeyInput {
    static func send(_ event: NSEvent, to session: HvfEngineSession, refused: () -> Void = { NSSound.beep() }) -> Bool {
        var modifiers: EventModifiers = []
        let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
        if flags.contains(.control) { modifiers.insert(.control) }
        if flags.contains(.option) { modifiers.insert(.option) }
        if flags.contains(.command) { modifiers.insert(.command) }
        if flags.contains(.shift) { modifiers.insert(.shift) }
        switch HvfHostKeyCommand.resolve(characters: event.characters ?? "", modifiers: modifiers) {
        case let .key(action): if !session.sendDisplayInput(.key(action)) { refused() } // full paste backlog
        case let .text(text): if !session.sendDisplayInput(.text(text)) { refused() }
        case .ignored: return false
        }
        return true
    }
}
#endif
