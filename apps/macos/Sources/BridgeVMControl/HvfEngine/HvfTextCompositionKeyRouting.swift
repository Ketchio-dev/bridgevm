#if canImport(AppKit)
import AppKit
import SwiftUI

enum HvfTextCompositionKeyRouting {
    static func isPhysical(_ event: NSEvent) -> Bool {
        var modifiers: EventModifiers = []
        if event.modifierFlags.contains(.option) { modifiers.insert(.option) }
        if event.modifierFlags.contains(.shift) { modifiers.insert(.shift) }
        if case .key = HvfHostKeyCommand.resolve(characters: event.characters ?? "", modifiers: modifiers) { return true }
        return false
    }
}
#endif
