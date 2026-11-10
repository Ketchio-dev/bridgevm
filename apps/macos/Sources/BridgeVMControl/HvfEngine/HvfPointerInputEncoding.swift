import Foundation
/// Matches the guest's strict primary-desktop pointer grammar and INPUT count.
enum HvfPointerInputEncoding {
    /// The optional third field: modifiers held for this one action, `w` Win, `c` Ctrl, `a` Alt and `s` Shift,
    /// each at most once and in this order (the key chords' order). The agent presses them before the action's
    /// events and releases them after, in reverse, in the same batch (scripts/win-assets/bvagent-pointer-input.cs).
    static let modifierOrder = "wcas"

    static func eventCount(_ command: String) -> Int? {
        guard !command.isEmpty, command.utf8.count <= 64 else { return nil }
        let fields = command.split(separator: ":", omittingEmptySubsequences: false)
        if fields.count == 3 {
            guard isHeldModifiers(fields[2]), let count = eventCount(fields[0] + ":" + fields[1]) else { return nil }
            return count + 2 * fields[2].count
        }
        if command.hasPrefix("scroll") { return HvfPointerScrollEncoding.eventCount(command) }
        guard fields.count == 2 else { return nil }
        if fields[0] == "wheel" {
            guard let ticks = Int(fields[1]), String(ticks) == fields[1],
                  ticks != 0, (-127...127).contains(ticks) else { return nil }
            return 1
        }
        let point = fields[1].split(separator: "x", omittingEmptySubsequences: false)
        guard point.count == 2, point.allSatisfy({ value in
            guard let coordinate = Int(value) else { return false }
            return (0...32767).contains(coordinate) && String(coordinate) == value
        }) else { return nil }
        switch fields[0] {
        case "move", "press", "release", "releaseall", "rightpress", "rightrelease", "middlepress", "middlerelease": return 1
        case "click", "rightclick", "middleclick": return 2
        default: return nil
        }
    }

    /// A non-empty run of `modifierOrder` letters in that order, each at most once.
    static func isHeldModifiers(_ value: Substring) -> Bool {
        var rest = modifierOrder[...]
        for name in value {
            guard let index = rest.firstIndex(of: name) else { return false }
            rest = rest[rest.index(after: index)...]
        }
        return !value.isEmpty
    }
}

extension HvfUnicodeInputRequest {
    init?(text: String, now: Date, id: UUID = UUID()) {
        self.init(event: .text(text), now: now, id: id)
    }
}
