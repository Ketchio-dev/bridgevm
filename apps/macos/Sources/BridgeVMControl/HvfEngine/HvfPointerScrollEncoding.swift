/// A coordinate move and wheel insertion belong to one acknowledged request.
enum HvfPointerScrollEncoding {
    static func eventCount(_ command: String) -> Int? {
        guard command.hasPrefix("scroll:"), command.utf8.count <= 64 else { return nil }
        let parts = command.dropFirst(7).split(separator: "@", omittingEmptySubsequences: false)
        guard parts.count == 2, let ticks = Int(parts[0]), String(ticks) == parts[0],
              ticks != 0, (-128...127).contains(ticks),
              HvfPointerInputEncoding.eventCount("move:" + parts[1]) == 1 else { return nil }
        return 2
    }
}
