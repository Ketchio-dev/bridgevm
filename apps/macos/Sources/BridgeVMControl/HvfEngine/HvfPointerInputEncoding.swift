import Foundation
/// Matches the guest's strict primary-desktop pointer grammar and INPUT count.
enum HvfPointerInputEncoding {
    static func eventCount(_ command: String) -> Int? {
        if command.hasPrefix("scroll:") { return HvfPointerScrollEncoding.eventCount(command) }
        guard !command.isEmpty, command.utf8.count <= 64 else { return nil }
        let fields = command.split(separator: ":", omittingEmptySubsequences: false)
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
        case "move", "press", "release", "releaseall", "rightpress", "rightrelease": return 1
        case "click", "rightclick": return 2
        default: return nil
        }
    }
}

extension HvfUnicodeInputRequest {
    init?(text: String, now: Date, id: UUID = UUID()) {
        self.init(event: .text(text), now: now, id: id)
    }
}
