import SwiftUI

/// The per-VM 'Command 키' setting. Mac-like (the default): a Command chord is the same Ctrl chord
/// (⌘C is Ctrl+C), text navigation follows the Mac (`macTextNavigation`) and a bare Command tap is
/// the Windows key. Windows key: Command is the Windows key and Option is Alt in every chord.
/// Saved in vm.json as `commandKeyMode`, absent for the default.
enum HvfCommandKeyMode: String, CaseIterable, Equatable {
    case mac
    case windows

    init(saved: String?) { self = Self(rawValue: saved ?? "") ?? .mac }
}

/// How the live display maps Command, Control and Option chords. Without the guest's generic chord
/// support (`modifierChords`), keys resolve exactly as before it existed and Command stays with the Mac.
struct HvfHostKeyMapping: Equatable {
    var commandKey = HvfCommandKeyMode.mac
    var modifierChords = false
}

extension HvfHostKeyCommand {
    /// A Command, Control or Option chord as one generic action (HvfModifierChord), Option as Alt.
    /// Nil leaves the event to the earlier resolver: no generic support, or no such modifier held.
    static func resolveChord(keyCode: UInt16, charactersIgnoringModifiers: String,
                             modifiers: EventModifiers, mapping: HvfHostKeyMapping) -> Self? {
        let flags = modifiers.intersection([.command, .control, .option, .shift])
        guard mapping.modifierChords, !flags.isDisjoint(with: [.command, .control, .option]) else { return nil }
        guard let key = HvfModifierChord.keyName(keyCode: keyCode, charactersIgnoringModifiers: charactersIgnoringModifiers),
              !isReservedForMac(key, flags: flags) else { return .ignored }
        // Ctrl+Option+Delete (the Mac's Backspace) stays the secure attention sequence.
        if key == "backspace", flags.isSuperset(of: [.control, .option]), !flags.contains(.command) { return .key("ctrl+alt+delete") }
        let held = [flags.contains(.command) ? "cmd" : nil, flags.contains(.control) ? "ctrl" : nil,
                    flags.contains(.option) ? "opt" : nil, flags.contains(.shift) ? "shift" : nil].compactMap { $0 }
        if mapping.commandKey == .mac, let action = macTextNavigation[(held + [key]).joined(separator: "+")] { return .key(action) }
        let command = flags.contains(.command)
        let parts = [command && mapping.commandKey == .windows ? "win" : nil,
                     flags.contains(.control) || command && mapping.commandKey == .mac ? "ctrl" : nil,
                     flags.contains(.option) ? "alt" : nil,
                     flags.contains(.shift) ? "shift" : nil].compactMap { $0 }
        return .key((parts + [key]).joined(separator: "+"))
    }

    /// 'Mac처럼' text navigation does what these keys do on a Mac, not the literal Ctrl or Alt chord
    /// (Alt+Left is Back in Windows). Exact modifiers only; the Shift variants select.
    static let macTextNavigation: [String: String] = [
        "cmd+left": "home", "cmd+right": "end", "cmd+up": "ctrl+home", "cmd+down": "ctrl+end",
        "cmd+shift+left": "shift+home", "cmd+shift+right": "shift+end",
        "cmd+shift+up": "ctrl+shift+home", "cmd+shift+down": "ctrl+shift+end",
        "opt+left": "ctrl+left", "opt+right": "ctrl+right", "opt+backspace": "ctrl+backspace",
        "opt+shift+left": "ctrl+shift+left", "opt+shift+right": "ctrl+shift+right"
    ]

    /// App shortcuts the Mac keeps in both modes: Quit, Hide, Minimize, Settings, Full Screen and the
    /// app switcher (which the system takes anyway).
    static func isReservedForMac(_ key: String, flags: EventModifiers) -> Bool {
        guard flags.contains(.command) else { return false }
        let others = flags.intersection([.control, .option, .shift])
        switch key {
        case "tab": return true
        case "q", "h", "m", "comma": return others.isEmpty
        case "f": return others == .control
        default: return false
        }
    }
}
