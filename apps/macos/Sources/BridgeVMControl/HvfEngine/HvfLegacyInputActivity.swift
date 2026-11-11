import Foundation

/// What the session last wrote to input.ctl, the VMM's HID route, as far as the ordered route's switch cares.
/// The two routes reach Windows through separate queues, so the agent route may take over only when nothing
/// written to input.ctl can still be ahead of it. Elapsed time is no evidence of that: the VMM takes at most one
/// command per tick and keeps it queued, everything behind it too, while its xHCI queue is busy. So the switch
/// waits until the VMM has taken every KEY and non-move POINTER line off its queue (its run.log line for each,
/// `observe`), and then a second after the last such line or write, for the guest to read the reports. A move
/// prints no such line (a late one would drag an ordered press back), so one written after the last counted line
/// needs a `barrier`; and never while a button pressed there, or a key held there (`KEY down:<key>`), is still down
/// (its release must take the same route: the HID mouse and keyboard cannot release a SendInput press, nor SendInput
/// an HID one). A VMM that prints no such lines (an older one) never lets the switch happen after HID input: the HID
/// route stays.
struct HvfLegacyInputActivity {
    static let settle: TimeInterval = 1
    private var last = Date.distantPast
    /// The VMM's HID button state (bridgevm-hvf xhci/pointer_input_report.rs): a press replaces it, a release
    /// or right click clears it, a left click leaves it.
    private var buttons = 0
    /// The keys held on the VMM's keyboard (live_input/key_actions.rs): `KEY down:<key>` until its `KEY up:<key>`.
    private(set) var keys: [String] = []
    /// KEY and non-move POINTER lines written, and how many of them the VMM has taken off its queue.
    private var written = 0, taken = 0
    /// The position of a move written after the last counted line: nothing shows yet that it left the VMM's queue.
    private var unconfirmedMove: String?

    mutating func wrote(_ line: String, now: Date) {
        if line.hasPrefix("KEY "), let key = HvfModifierChord.heldAction(String(line.dropFirst(4))) {
            keys.removeAll { $0 == key }
            if line.hasPrefix("KEY " + HvfModifierChord.down) { keys.append(key) }
        }
        if line.hasPrefix("POINTER ") {
            let verb = line.dropFirst(8).prefix(while: { $0 != ":" })
            switch verb {
            case "move":
                unconfirmedMove = String(line.dropFirst(8 + verb.count + 1).prefix(while: { $0 != ":" }))
                return
            case "press": buttons = 1
            case "right-press": buttons = 2
            case "release", "right-click": buttons = 0
            default: break
            }
            // The VMM takes a pointer line only after it queued the move before it (one FIFO) and once the move's
            // reports drained (queue_actions is busy until then); the second after it covers the rest. A KEY line
            // proves nothing about the mouse: the keyboard's reports have their own queue.
            unconfirmedMove = nil
        }
        written += 1
        last = now
    }

    /// The line to write behind an unconfirmed move once input.ctl is otherwise quiet: a release at the move's own
    /// position, with no button down a report the guest already has. The VMM takes it only after the move's reports
    /// drained, and it is counted like any line, so the switch then waits for its run.log line and the second. A
    /// later move needs another; nil while one is still untaken.
    func barrier(now: Date) -> String? {
        guard let move = unconfirmedMove, taken == written, buttons == 0,
              now.timeIntervalSince(last) >= Self.settle else { return nil }
        return "POINTER release:" + move
    }

    /// One run.log batch in order. A helper recreated for a guest reset starts at input.ctl's end and its
    /// predecessor's queue is gone, so its record ends what was still owed; an in-process reboot keeps the
    /// queue, so it changes nothing.
    mutating func observe(_ lines: [String], now: Date) {
        for line in lines {
            if line == HvfGuestResetRecord.processRecreation || line == HvfGuestResetRecord.processRecreation + "\r" {
                written = 0; taken = 0; keys = [] // a new VMM holds no key
            } else if Self.takesCommand(line) {
                taken = min(taken + 1, written) // a line never stands for one written later
                last = now
            }
        }
    }

    func quiescent(now: Date) -> Bool {
        taken == written && buttons == 0 && keys.isEmpty && unconfirmedMove == nil && now.timeIntervalSince(last) >= Self.settle
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
