import Foundation

extension HvfEngineSession {
    /// Writes events Windows inserted nothing of on the ordered route (a lock, sign-in or UAC screen, where the
    /// agent's SendInput inserts nothing but the VMM's keyboard and mouse still work) to input.ctl, in their
    /// original order and before any later input, through the paths legacy input takes: text as HID keys or a
    /// clipboard paste, input that arrives behind a pending paste held for it, a press cancelling that paste.
    func resendOnLegacy(_ events: [HvfOrderedInputQueue.Event]) {
        var lost = 0
        for event in events {
            let sent: Bool
            switch event {
            case let .key(key):
                sent = legacyPaste.pending != nil ? legacyPaste.hold(.key(key)) : appendLiveInput("KEY " + key)
            case let .text(text):
                sent = legacyPaste.pending != nil ? legacyPaste.hold(.text(text)) : sendLegacyText(text) != .refused
            case let .pointer(pointer):
                guard let line = Self.legacyPointerLine(pointer) else { lost += 1; continue }
                if ["click:", "press:", "right-click:", "right-press:"].contains(where: line.hasPrefix) {
                    cancelPendingPaste("pointer pressed")
                }
                sent = appendLiveInput("POINTER " + line)
            }
            if !sent { lost += 1 }
        }
        if lost > 0 { append(.unknown("ordered input fallback: \(lost) of \(events.count) events could not be re-sent")) }
    }

    /// The input.ctl form of an ordered pointer event without its held modifiers (the HID route has none), or nil
    /// for what the VMM's mouse cannot do: the middle button, and a scroll of less than one vertical notch.
    static func legacyPointerLine(_ pointer: String) -> String? {
        let fields = pointer.split(separator: ":", omittingEmptySubsequences: false)
        guard fields.count >= 2 else { return nil }
        if fields[0] == "scrollunits" {
            let parts = fields[1].split(separator: "@")
            guard parts.count == 2, let vertical = parts[0].split(separator: ",").first.flatMap({ Int($0) }) else { return nil }
            let notches = max(-127, min(127, vertical / HvfScrollAccumulator.notch))
            return notches == 0 ? nil : "scroll:\(notches)@\(parts[1])"
        }
        let verbs = ["move": "move", "click": "click", "press": "press", "releaseall": "release",
                     "rightclick": "right-click", "rightpress": "right-press", "scroll": "scroll"]
        return verbs[String(fields[0])].map { "\($0):\(fields[1])" }
    }
}
