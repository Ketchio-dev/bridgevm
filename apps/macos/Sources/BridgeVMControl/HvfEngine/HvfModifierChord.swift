import Foundation

/// One generic guest key action: `win`, `ctrl`, `alt` and `shift`, each at most once and in that
/// order, then one key; or a single modifier alone (a bare tap). The VMM builds any of them as one
/// boot-keyboard report (live_input/key_chords.rs); a guest agent builds them only after its
/// `BVINPUT_CHORDS <uuid> MODIFIER_CHORDS` marker (bvagent-key-input.cs). A character key alone or
/// with only Shift is text, which has its own path.
enum HvfModifierChord {
    static let token = "MODIFIER_CHORDS"
    static let modifiers = ["win", "ctrl", "alt", "shift"]
    static let navigation: Set<String> = ["left", "up", "right", "down", "home", "end", "pageup", "pagedown"]
    /// Keys that never type text.
    static let named = navigation.union(["backspace", "tab", "enter", "esc", "escape", "space", "insert", "delete"])
        .union((1...12).map { "f\($0)" })
    /// The US-layout punctuation keys, by name.
    static let punctuation: Set<String> = ["minus", "equal", "lbracket", "rbracket", "backslash", "semicolon",
                                           "quote", "grave", "comma", "period", "slash"]
    /// The Print Screen key (the Mac's F13). The VMM builds it; an agent only after its
    /// `BVINPUT_KEYS <uuid> PRINTSCREEN` marker (HvfInputCapabilityReceipt).
    static let printScreen = "printscreen"
    /// A held key: `down:<action>` is the first half of the action's events (its keys pressed, in order) and
    /// `up:<action>` the second (released in reverse), from the Mac's key-down to its key-up. The VMM keeps a held key
    /// in its HID report and Windows repeats it (live_input/key_actions.rs); an agent builds the halves only after its
    /// `BVINPUT_KEYS <uuid> HOLD` marker (bvagent-key-input.cs).
    static let down = "down:", up = "up:"

    /// The action a held key's `down:` or `up:` half names; nil for a whole tap.
    static func heldAction(_ command: String) -> String? {
        for half in [down, up] where command.hasPrefix(half) { return String(command.dropFirst(half.count)) }
        return nil
    }

    /// How many key events (each part pressed and released) a valid action inserts; nil if invalid.
    static func eventCount(_ action: String) -> Int? {
        guard !action.isEmpty, action.utf8.count <= 32 else { return nil }
        let parts = action.split(separator: "+", omittingEmptySubsequences: false).map(String.init)
        guard let key = parts.last else { return nil }
        var next = 0, chord = false
        for part in parts.dropLast() {
            guard let index = modifiers[next...].firstIndex(of: part) else { return nil }
            next = index + 1
            chord = chord || part != "shift"
        }
        let valid = parts.count == 1 && modifiers.contains(key) || named.contains(key) || key == printScreen
            || chord && isCharacter(key)
        return valid ? parts.count * 2 : nil
    }

    /// What every v3 resident builds before any keyset marker (bvagent-key-input.cs at v3).
    static func isBase(_ action: String) -> Bool {
        let parts = action.split(separator: "+", omittingEmptySubsequences: false)
        guard let last = parts.last else { return false }
        let key = String(last)
        switch parts.dropLast().joined(separator: "+") {
        case "": return named.contains(key)
        case "shift": return navigation.contains(key) || key == "tab"
        case "ctrl": return navigation.contains(key) || ["a", "c", "v", "x", "y", "z", "backspace", "delete"].contains(key)
        case "ctrl+shift": return navigation.contains(key)
        case "alt": return ["tab", "enter", "f4"].contains(key)
        default: return false
        }
    }

    private static func isCharacter(_ key: String) -> Bool {
        guard key.utf8.count == 1, let byte = key.utf8.first else { return punctuation.contains(key) }
        return (0x61...0x7a).contains(byte) || (0x30...0x39).contains(byte)
    }

    /// A key's chord name. Named keys, digits and punctuation go by key code (Shift changes their
    /// characters); a letter by its Latin character, so Dvorak or AZERTY letters stay themselves,
    /// and otherwise (Korean and other non-Latin input sources) by its ANSI key position.
    static func keyName(keyCode: UInt16, charactersIgnoringModifiers: String) -> String? {
        if let name = namedKeyCodes[keyCode] { return name }
        let scalars = charactersIgnoringModifiers.lowercased().unicodeScalars
        if scalars.count == 1, let value = scalars.first?.value, (0x61...0x7a).contains(value) {
            return String(UnicodeScalar(UInt8(value)))
        }
        return HvfControlChordKeys.letterPositions[keyCode] ?? characterKeyCodes[keyCode]
    }

    /// macOS virtual key codes (Carbon kVK_*) of the keys that never type text.
    private static let namedKeyCodes: [UInt16: String] = [
        0x24: "enter", 0x4c: "enter", 0x30: "tab", 0x31: "space", 0x33: "backspace", 0x35: "esc", 0x75: "delete",
        0x73: "home", 0x77: "end", 0x74: "pageup", 0x79: "pagedown", 0x7b: "left", 0x7c: "right", 0x7d: "down", 0x7e: "up",
        0x7a: "f1", 0x78: "f2", 0x63: "f3", 0x76: "f4", 0x60: "f5", 0x61: "f6",
        0x62: "f7", 0x64: "f8", 0x65: "f9", 0x6d: "f10", 0x67: "f11", 0x6f: "f12", 0x69: printScreen
    ]
    /// The ANSI digit and punctuation key positions (kVK_ANSI_*).
    private static let characterKeyCodes: [UInt16: String] = [
        0x12: "1", 0x13: "2", 0x14: "3", 0x15: "4", 0x17: "5", 0x16: "6", 0x1a: "7", 0x1c: "8", 0x19: "9", 0x1d: "0",
        0x1b: "minus", 0x18: "equal", 0x21: "lbracket", 0x1e: "rbracket", 0x2a: "backslash", 0x29: "semicolon",
        0x27: "quote", 0x32: "grave", 0x2b: "comma", 0x2f: "period", 0x2c: "slash"
    ]
}
