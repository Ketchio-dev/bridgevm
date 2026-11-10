#if canImport(AppKit)
import AppKit

/// A bare Command or Option tap: pressed with no other modifier down and released before any key,
/// click, other modifier or focus loss. Left Command is the Windows key in both 'Command 키' modes
/// and Option is Alt. Right Command (0x36) never taps: Korean users often make it the 한/영 key, and
/// each language switch would open Start; its chords still map as usual. Guest chords, and the
/// modifiers a click or scroll carries (HvfPointerModifiers.swift), are whole press-and-release
/// requests, so the guest never keeps a modifier down; losing focus only has to forget a pending tap here.
struct HvfModifierTap: Equatable {
    private var armed: String?
    private static let taps: [UInt16: String] = [0x37: "win", 0x3a: "alt", 0x3d: "alt"]

    /// The guest action this flags change completes, if any.
    mutating func changed(keyCode: UInt16, flags: NSEvent.ModifierFlags) -> String? {
        let down = flags.intersection([.command, .option, .control, .shift])
        let tap = Self.taps[keyCode], pending = armed
        armed = nil
        if let tap, down == (tap == "win" ? .command : .option) {
            armed = tap
            return nil
        }
        return tap != nil && pending == tap && down.isEmpty ? tap : nil
    }

    mutating func cancel() { armed = nil }
}
#endif
