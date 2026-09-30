import Foundation

/// Counts ordered pointer requests that the guest agent reports as inserted. A
/// display click sends a press and a release, preceded by at most one move, so
/// two insertions include the press.
enum T17PointerReceipt {
    static let clickInsertions = 2

    static func inserted(in tail: String) -> Int {
        var open: String?, marked = false, count = 0
        for line in tail.replacingOccurrences(of: "\r", with: "").split(separator: "\n").map(String.init) {
            let fields = line.split(separator: " ").map(String.init)
            if fields.count == 5, fields[0] == "BVAGENT", fields[1] == "CMD", fields[2] == "POINTERINPUT", fields[4] == "exit=0" {
                open = fields[3]; marked = false
            } else if let id = open, fields.count == 3, fields[0] == "BVINPUT_INSERTED", fields[1] == id,
                      let events = Int(fields[2]), events > 0 {
                marked = true
            } else if let id = open, line == "BVAGENT END POINTERINPUT \(id)" {
                if marked { count += 1 }
                open = nil
            }
        }
        return count
    }

    /// Runs the click, then requires its insertions in the run log written after it began.
    static func require(_ runLog: URL, timeout: TimeInterval, click: () throws -> Void) throws {
        let offset = (try? Data(contentsOf: runLog).count) ?? 0
        try click()
        let deadline = Date().addingTimeInterval(timeout)
        repeat {
            if let data = try? Data(contentsOf: runLog), data.count > offset,
               inserted(in: String(decoding: data.suffix(from: offset), as: UTF8.self)) >= clickInsertions { return }
            RunLoop.current.run(until: Date().addingTimeInterval(0.2))
        } while Date() < deadline
        throw T17Blocker(code: "guest-evidence-missing", detail: "display click was not inserted as guest pointer input")
    }
}
