import Foundation

/// What the session last wrote to input.ctl, the VMM's HID route, as far as the ordered route's switch cares.
/// The two routes reach Windows through separate queues, so the agent route may take over only when nothing
/// written to input.ctl can still be ahead of it. Elapsed time is no evidence of that: the VMM takes at most one
/// command per tick and keeps it queued, everything behind it too, while its xHCI queue is busy. So the switch
/// waits until the VMM has taken every KEY and non-move POINTER line off its queue (its run.log line for each,
/// `observe`), and then a second after the last such line or write, for the guest to read the reports; a
/// quarter second after the last absolute move, which only has to land before the first ordered event (that
/// event carries its own position; a late move would drag an ordered press back), while the VMM reads
/// input.ctl every 16 ms; and never while a button pressed there is still down (its release must take the
/// same route: the HID mouse cannot release a SendInput press, nor SendInput an HID one). A VMM that prints no
/// such lines (an older one) never lets the switch happen after HID input: the HID route stays.
struct HvfLegacyInputActivity {
    static let settle: TimeInterval = 1
    static let moveSettle: TimeInterval = 0.25
    private var last = Date.distantPast, lastMove = Date.distantPast
    /// The VMM's HID button state (bridgevm-hvf xhci/pointer_input_report.rs): a press replaces it, a release
    /// or right click clears it, a left click leaves it.
    private var buttons = 0
    /// KEY and non-move POINTER lines written, and how many of them the VMM has taken off its queue.
    private var written = 0, taken = 0

    mutating func wrote(_ line: String, now: Date) {
        if line.hasPrefix("POINTER ") { // KEY lines are whole taps
            switch line.dropFirst(8).prefix(while: { $0 != ":" }) {
            case "move": lastMove = now; return
            case "press": buttons = 1
            case "right-press": buttons = 2
            case "release", "right-click": buttons = 0
            default: break
            }
        }
        written += 1
        last = now
    }

    /// One run.log batch in order. A helper recreated for a guest reset starts at input.ctl's end and its
    /// predecessor's queue is gone, so its record ends what was still owed; an in-process reboot keeps the
    /// queue, so it changes nothing.
    mutating func observe(_ lines: [String], now: Date) {
        for line in lines {
            if line == HvfGuestResetRecord.processRecreation || line == HvfGuestResetRecord.processRecreation + "\r" {
                written = 0; taken = 0
            } else if Self.takesCommand(line) {
                taken = min(taken + 1, written) // a line never stands for one written later
                last = now
            }
        }
    }

    func quiescent(now: Date) -> Bool {
        taken == written && buttons == 0
            && now.timeIntervalSince(last) >= Self.settle && now.timeIntervalSince(lastMove) >= Self.moveSettle
    }

    /// Nothing was written, but the next switch waits a full second from now: a fallback whose cleanup release
    /// inserted nothing retries it only then.
    mutating func restart(now: Date) { last = max(last, now) }

    /// The one line hvf_gic_boot_probe live_input/device_apply.rs prints when it takes a KEY or POINTER command off
    /// its queue; a move prints it only when rejected (an accepted one only every 1024th, as a count), so never count.
    static func takesCommand(_ line: String) -> Bool {
        if line.hasPrefix("live input rejected: kind=key parse_error=")
            || line.hasPrefix("live input rejected: kind=pointer parse_error=") { return true }
        let command: Substring
        if line.hasPrefix("live input accepted: command=") {
            command = line.dropFirst("live input accepted: command=".count)
        } else if line.hasPrefix("live input rejected: command="), line.hasSuffix(" queue_error=invalid") {
            command = line.dropFirst("live input rejected: command=".count)
        } else {
            return false
        }
        return command.hasPrefix("Key(") || (command.hasPrefix("Pointer(") && !command.hasPrefix("Pointer(\"move:"))
    }
}
