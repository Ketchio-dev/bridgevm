#if canImport(AppKit)
import AppKit
import SwiftUI

extension HvfHostKeyCommand {
    static func resolve(event: NSEvent, mapping: HvfHostKeyMapping = HvfHostKeyMapping()) -> Self {
        let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
        var modifiers: EventModifiers = []
        if flags.contains(.control) { modifiers.insert(.control) }
        if flags.contains(.option) { modifiers.insert(.option) }
        if flags.contains(.command) { modifiers.insert(.command) }
        if flags.contains(.shift) { modifiers.insert(.shift) }
        if let chord = resolveChord(keyCode: event.keyCode, charactersIgnoringModifiers: event.charactersIgnoringModifiers ?? "",
                                    modifiers: modifiers, mapping: mapping) { return chord }
        // kVK_ANSI_KeypadEnter produces ETX, also used by Ctrl+C. Its physical identity must win:
        // Ctrl alone is Ctrl+Enter (resolved by key code), never a guest Copy action.
        guard event.keyCode != 0x4c || !modifiers.contains(.control) || modifiers == .control else { return .ignored }
        return resolve(characters: event.characters ?? "", modifiers: modifiers, keyCode: event.keyCode)
    }
}
#endif
