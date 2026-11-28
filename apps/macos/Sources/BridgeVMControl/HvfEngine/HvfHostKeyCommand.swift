import Foundation
import SwiftUI
import AppKit

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
        case "\r", "\n", "\u{3}": return .key("enter")
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
        // F13 (U+F710) sits where a PC keyboard has Print Screen; a Mac keyboard has no PrtSc key.
        case "\u{f710}": return .key(HvfModifierChord.printScreen)
        // A PC keyboard's Insert key reaches macOS as Help (U+F746); Insert itself is U+F727.
        case "\u{f746}", "\u{f727}": return .key("insert")
        default:
            guard !characters.isEmpty,
                  !modifiers.contains(.control),
                  !modifiers.contains(.option),
                  // Other function keys (Num Lock as Clear, F14-F35, media keys) have no Windows key
                  // here: typed as text, they were a junk character.
                  !characters.unicodeScalars.contains(where: { (0xf700...0xf8ff).contains($0.value) }) else { return .ignored }
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
            if let key = navigation[characters] ?? HvfControlChordKeys.shiftKeys[characters] { return .key("shift+" + key) }
            if let name = ["\t": "tab", "\u{19}": "tab", "\r": "enter", "\n": "enter", "\u{3}": "enter", " ": "space"][characters] { return .key("shift+" + name) }
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
            // Every Ctrl+letter; the ordered route gates letters beyond A/C/V/X/Y/Z on the guest marker.
            guard let letter = HvfControlChordKeys.letter(characters) else { return .ignored }
            return .key("ctrl+" + letter)
        }
        if flags == .option {
            switch characters {
            case "\u{f707}": return .key("alt+f4")
            case "\t": return .key("alt+tab")
            case "\r", "\n", "\u{3}": return .key("alt+enter")
            default: return .ignored
            }
        }
        return .ignored
    }
}

extension HvfHostKeyCommand {
    /// Physical Ctrl+Return/Tab first, then logical commands; non-Latin Ctrl letters may instead identify an ANSI letter position.
    static func resolve(characters: String, modifiers: EventModifiers, keyCode: UInt16) -> Self {
        let flags = modifiers.intersection([.control, .option, .shift, .command])
        if flags == .control || flags == [.control, .shift],
           let key = HvfControlChordKeys.physical(shift: flags.contains(.shift), keyCode: keyCode) { return .key(key) }
        let logical = resolve(characters: characters, modifiers: modifiers)
        guard logical == .ignored, modifiers == .control, characters.unicodeScalars.count == 1,
              let scalar = characters.unicodeScalars.first, !scalar.isASCII,
              nonLatinControlLetters.contains(scalar.properties.generalCategory),
              let key = HvfControlChordKeys.letterPositions[keyCode] else { return logical }
        return .key("ctrl+" + key)
    }

    private static let nonLatinControlLetters: [Unicode.GeneralCategory] = [.uppercaseLetter, .lowercaseLetter, .titlecaseLetter, .modifierLetter, .otherLetter]
}

#if canImport(AppKit)

extension HvfHostKeyCommand {
    static func modifiers(_ event: NSEvent) -> EventModifiers {
        let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
        var modifiers: EventModifiers = []
        if flags.contains(.control) { modifiers.insert(.control) }
        if flags.contains(.option) { modifiers.insert(.option) }
        if flags.contains(.command) { modifiers.insert(.command) }
        if flags.contains(.shift) { modifiers.insert(.shift) }
        return modifiers
    }

    static func optionLayerASCII(event: NSEvent) -> String? {
        optionLayerASCII(characters: event.characters ?? "",
                         charactersIgnoringModifiers: event.charactersIgnoringModifiers ?? "", modifiers: modifiers(event))
    }

    /// Right Option alone (Shift allowed) on a key that types a character is AltGr, as on a Windows keyboard:
    /// the key goes to the Mac's input method and its character is typed (€ is ⌥E on a German keyboard, ⌥E
    /// starts an accent on a US one). Left Option stays Alt; Space, Return, Tab, arrows and other named keys
    /// keep their Option chords on either side.
    static func isAltGr(_ event: NSEvent) -> Bool {
        let raw = event.modifierFlags.rawValue
        guard event.modifierFlags.intersection([.command, .control, .option]) == .option,
              raw & rightOptionDown != 0, raw & leftOptionDown == 0,
              let base = event.charactersIgnoringModifiers?.unicodeScalars.first else { return false }
        return base.value > 0x20 && base.value != 0x7f && !(0xf700...0xf8ff).contains(base.value)
    }

    /// NX_DEVICELALTKEYMASK and NX_DEVICERALTKEYMASK (IOLLEvent.h): which Option key is down.
    private static let leftOptionDown: UInt = 0x20, rightOptionDown: UInt = 0x40

    static func resolve(event: NSEvent, mapping: HvfHostKeyMapping = HvfHostKeyMapping()) -> Self {
        let modifiers = modifiers(event)
        if let chord = resolveChord(keyCode: event.keyCode, charactersIgnoringModifiers: event.charactersIgnoringModifiers ?? "",
                                    modifiers: modifiers, mapping: mapping) { return chord }
        // kVK_ANSI_KeypadEnter produces ETX, also used by Ctrl+C. Its physical identity must win:
        // Ctrl alone is Ctrl+Enter (resolved by key code), never a guest Copy action.
        guard event.keyCode != 0x4c || !modifiers.contains(.control) || modifiers == .control else { return .ignored }
        return resolve(characters: event.characters ?? "", modifiers: modifiers, keyCode: event.keyCode)
    }
}
#endif
