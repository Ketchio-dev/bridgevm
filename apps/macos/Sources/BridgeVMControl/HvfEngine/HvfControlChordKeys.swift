import Foundation

/// Ctrl+A–Z, Ctrl+Enter, Ctrl+Tab, Ctrl+Shift+Tab, Shift+F1–F12 and Shift+Delete. Base v3
/// residents build only Ctrl+A/C/V/X/Y/Z, so an ordered stream queues the rest of this set
/// only after the guest's separate `BVINPUT_CHORDS <uuid> CTRL_SHIFT_CHORDS` marker.
enum HvfControlChordKeys {
    static let token = "CTRL_SHIFT_CHORDS"
    static let extended: Set<String> = Set("bdefghijklmnopqrstuw".map { "ctrl+\($0)" } + (1...12).map { "shift+f\($0)" }
        + ["ctrl+enter", "ctrl+tab", "ctrl+shift+tab", "shift+delete"])
    /// NSEvent private-use function-key scalars plus forward delete, as Shift chord names.
    static let shiftKeys: [String: String] = [
        "\u{f704}": "f1", "\u{f705}": "f2", "\u{f706}": "f3", "\u{f707}": "f4", "\u{f708}": "f5", "\u{f709}": "f6",
        "\u{f70a}": "f7", "\u{f70b}": "f8", "\u{f70c}": "f9", "\u{f70d}": "f10", "\u{f70e}": "f11", "\u{f70f}": "f12",
        "\u{f728}": "delete"
    ]
    /// macOS ANSI virtual key positions (Carbon kVK_ANSI_A...kVK_ANSI_Z) for non-Latin input sources.
    static let letterPositions: [UInt16: String] = [
        0x00: "a", 0x0b: "b", 0x08: "c", 0x02: "d", 0x0e: "e", 0x03: "f", 0x05: "g", 0x04: "h", 0x22: "i",
        0x26: "j", 0x28: "k", 0x25: "l", 0x2e: "m", 0x2d: "n", 0x1f: "o", 0x23: "p", 0x0c: "q", 0x0f: "r",
        0x01: "s", 0x11: "t", 0x20: "u", 0x09: "v", 0x0d: "w", 0x07: "x", 0x10: "y", 0x06: "z"
    ]

    /// One ASCII letter, or its Ctrl control-character form U+0001–U+001A, as a lowercase letter.
    /// Without a key code, Ctrl+Return's CR is Ctrl+M and Ctrl+Tab's HT is Ctrl+I.
    static func letter(_ characters: String) -> String? {
        guard characters.unicodeScalars.count == 1, var value = characters.unicodeScalars.first?.value else { return nil }
        if (1...26).contains(value) { value += 96 } else if (65...90).contains(value) { value += 32 }
        return (97...122).contains(value) ? String(UnicodeScalar(UInt8(value))) : nil
    }

    /// For Ctrl or Ctrl+Shift alone: Return (kVK_Return), keypad Enter (kVK_ANSI_KeypadEnter) and
    /// Tab (kVK_Tab) win over the control characters they share with Ctrl+M, Ctrl+C and Ctrl+I.
    static func physical(shift: Bool, keyCode: UInt16) -> String? {
        if !shift, [0x24, 0x4c].contains(keyCode) { return "ctrl+enter" }
        guard keyCode == 0x30 else { return nil }
        return shift ? "ctrl+shift+tab" : "ctrl+tab"
    }
}
