import Foundation

/// What the session last wrote to input.ctl, the VMM's HID route, as far as the ordered route's switch cares.
/// The two routes reach Windows through separate queues, so the agent route may take over only when nothing
/// written to input.ctl can still be ahead of it: a second after the last key, text, click or scroll; a
/// quarter second after the last absolute move, which only has to land before the first ordered event (that
/// event carries its own position; a late move would drag an ordered press back), while the VMM reads
/// input.ctl every 16 ms; and never while a button pressed there is still down (its release must take the
/// same route: the HID mouse cannot release a SendInput press, nor SendInput an HID one).
struct HvfLegacyInputActivity {
    static let settle: TimeInterval = 1
    static let moveSettle: TimeInterval = 0.25
    private var last = Date.distantPast, lastMove = Date.distantPast
    /// The VMM's HID button state (bridgevm-hvf xhci/pointer_input_report.rs): a press replaces it, a release
    /// or right click clears it, a left click leaves it.
    private var buttons = 0

    mutating func wrote(_ line: String, now: Date) {
        guard line.hasPrefix("POINTER ") else { last = now; return } // KEY lines are whole taps
        switch line.dropFirst(8).prefix(while: { $0 != ":" }) {
        case "move": lastMove = now; return
        case "press": buttons = 1
        case "right-press": buttons = 2
        case "release", "right-click": buttons = 0
        default: break
        }
        last = now
    }

    func quiescent(now: Date) -> Bool {
        buttons == 0 && now.timeIntervalSince(last) >= Self.settle && now.timeIntervalSince(lastMove) >= Self.moveSettle
    }
}
