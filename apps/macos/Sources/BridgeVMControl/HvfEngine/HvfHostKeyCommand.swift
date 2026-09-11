import Foundation
import SwiftUI

enum HvfHostKeyCommand: Equatable {
    case key(String)
    case text(String)
    case ignored

    static func resolve(characters: String, modifiers: EventModifiers = []) -> HvfHostKeyCommand {
        guard !modifiers.contains(.command) else { return .ignored }
        if characters == "\u{7f}", modifiers.contains(.control), modifiers.contains(.option) {
            return .key("ctrl+alt+delete")
        }
        if let modified = resolveModified(characters, modifiers: modifiers) { return modified }
        switch characters {
        case "\u{1b}": return .key("esc")
        case "\u{7f}": return .key("backspace")
        case "\u{f728}": return .key("delete")
        case "\u{f700}": return .key("up")
        case "\u{f701}": return .key("down")
        case "\u{f702}": return .key("left")
        case "\u{f703}": return .key("right")
        case "\u{f729}": return .key("home")
        case "\u{f72b}": return .key("end")
        case "\u{f72c}": return .key("pageup")
        case "\u{f72d}": return .key("pagedown")
        case "\t": return .key("tab")
        case "\r", "\n": return .key("enter")
        // NSEvent reports F1..F12 as private-use scalars F704..F70F. The guest
        // side already understands the "f1".."f12" tokens
        // (xhci_hid_input/setup_input/actions.rs:118-174); only this mapping
        // was missing, so the keys silently fell through to .ignored.
        case "\u{f704}": return .key("f1")
        case "\u{f705}": return .key("f2")
        case "\u{f706}": return .key("f3")
        case "\u{f707}": return .key("f4")
        case "\u{f708}": return .key("f5")
        case "\u{f709}": return .key("f6")
        case "\u{f70a}": return .key("f7")
        case "\u{f70b}": return .key("f8")
        case "\u{f70c}": return .key("f9")
        case "\u{f70d}": return .key("f10")
        case "\u{f70e}": return .key("f11")
        case "\u{f70f}": return .key("f12")
        default:
            guard !characters.isEmpty,
                  !modifiers.contains(.control),
                  !modifiers.contains(.option) else { return .ignored }
            return .text(characters)
        }
    }
}

extension HvfHostKeyCommand {
    private static func resolveModified(_ characters: String, modifiers: EventModifiers) -> Self? {
        let flags = modifiers.intersection([.control, .option, .shift])
        guard !flags.isEmpty else { return nil }
        let navigation = [
            "\u{f700}": "up", "\u{f701}": "down", "\u{f702}": "left", "\u{f703}": "right",
            "\u{f729}": "home", "\u{f72b}": "end", "\u{f72c}": "pageup", "\u{f72d}": "pagedown"
        ]
        if flags == .shift {
            if let key = navigation[characters] { return .key("shift+" + key) }
            if characters == "\t" || characters == "\u{19}" { return .key("shift+tab") }
            guard !characters.isEmpty, characters.unicodeScalars.allSatisfy({
                !CharacterSet.controlCharacters.contains($0) && !(0xf700...0xf8ff).contains($0.value)
            }) else { return .ignored }
            return nil
        }
        if flags == .control || flags == [.control, .shift] {
            let prefix = flags.contains(.shift) ? "ctrl+shift+" : "ctrl+"
            if let key = navigation[characters] { return .key(prefix + key) }
            guard flags == .control else { return .ignored }
            if characters == "\u{7f}" { return .key("ctrl+backspace") }
            if characters == "\u{f728}" { return .key("ctrl+delete") }
            var letter = characters.lowercased()
            if characters.unicodeScalars.count == 1, let scalar = characters.unicodeScalars.first,
               (1...26).contains(scalar.value) {
                letter = String(UnicodeScalar(UInt8(scalar.value) + 96))
            }
            return ["a", "c", "v", "x", "y", "z"].contains(letter) ? .key("ctrl+" + letter) : .ignored
        }
        if flags == .option {
            switch characters {
            case "\u{f707}": return .key("alt+f4")
            case "\t": return .key("alt+tab")
            case "\r", "\n": return .key("alt+enter")
            default: return .ignored
            }
        }
        return .ignored
    }
}
