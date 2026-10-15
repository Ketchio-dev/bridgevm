#if canImport(AppKit)
import AppKit
import SwiftUI

extension HvfHostKeyCommand {
    static func resolve(event: NSEvent) -> Self {
        let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
        // kVK_ANSI_KeypadEnter produces ETX, also used by Ctrl+C. Its physical
        // identity must win: Ctrl+Enter is unsupported, not a guest Copy action.
        guard event.keyCode != 0x4c || !flags.contains(.control) else { return .ignored }
        var modifiers: EventModifiers = []
        if flags.contains(.control) { modifiers.insert(.control) }
        if flags.contains(.option) { modifiers.insert(.option) }
        if flags.contains(.command) { modifiers.insert(.command) }
        if flags.contains(.shift) { modifiers.insert(.shift) }
        return resolve(characters: event.characters ?? "", modifiers: modifiers)
    }
}
#endif
