#if canImport(AppKit)
import AppKit
import SwiftUI

@MainActor
enum HvfFramebufferKeyInput {
    static func send(_ event: NSEvent, to session: HvfEngineSession) -> Bool {
        var modifiers: EventModifiers = []
        let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
        if flags.contains(.control) { modifiers.insert(.control) }
        if flags.contains(.option) { modifiers.insert(.option) }
        if flags.contains(.command) { modifiers.insert(.command) }
        if flags.contains(.shift) { modifiers.insert(.shift) }
        switch HvfHostKeyCommand.resolve(characters: event.characters ?? "", modifiers: modifiers) {
        case let .key(action): session.sendKey(action)
        case let .text(text): session.sendText(text)
        case .ignored: return false
        }
        return true
    }
}
#endif
