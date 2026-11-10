/// A coordinate move and wheel insertion belong to one acknowledged request.
enum HvfPointerScrollEncoding {
    static func eventCount(_ command: String) -> Int? {
        if command.hasPrefix("scrollunits:") { return unitsEventCount(command) }
        guard command.hasPrefix("scroll:"), command.utf8.count <= 64 else { return nil }
        let parts = command.dropFirst(7).split(separator: "@", omittingEmptySubsequences: false)
        guard parts.count == 2, let ticks = Int(parts[0]), String(ticks) == parts[0],
              ticks != 0, (-128...127).contains(ticks),
              HvfPointerInputEncoding.eventCount("move:" + parts[1]) == 1 else { return nil }
        return 2
    }

    /// `scrollunits:<v>,<h>@<x>x<y>`, signed Windows wheel units: a move, then a WHEEL when v != 0 and
    /// an HWHEEL when h != 0 (scripts/win-assets/bvagent-pointer-input.cs).
    private static func unitsEventCount(_ command: String) -> Int? {
        guard command.utf8.count <= 64 else { return nil }
        let parts = command.dropFirst(12).split(separator: "@", omittingEmptySubsequences: false)
        guard parts.count == 2 else { return nil }
        let fields = parts[0].split(separator: ",", omittingEmptySubsequences: false)
        let axes = fields.compactMap { field in
            Int(field).flatMap { String($0) == field && abs($0) <= HvfScrollAccumulator.maxUnitsPerEvent ? $0 : nil }
        }
        guard fields.count == 2, axes.count == 2, axes != [0, 0],
              HvfPointerInputEncoding.eventCount("move:" + parts[1]) == 1 else { return nil }
        return 1 + axes.filter { $0 != 0 }.count
    }
}
