import Foundation

/// One release on the same serial transport, never a replay of the failed request.
struct HvfPointerCleanup {
    private var stream = HvfAcknowledgedInputStream()
    var held = HvfPointerHeldState()
    private(set) var recovering = false
    var blocked = false
    var count: Int { stream.count }

    mutating func begin(now: Date) {
        guard !recovering, !blocked, let release = held.release else { return }
        recovering = stream.enqueue(.pointer(release), now: now)
        if !recovering { blocked = true }
    }

    mutating func poll(serviceReady: Bool, lines: [String], now: Date,
                       send: (String) -> Bool) -> HvfAcknowledgedInputStream.Update {
        let update = serviceReady ? stream.advance(lines: lines, now: now, send: send) : stream.cancel(.sessionChanged)
        if case .inserted = update { recovering = false; held = HvfPointerHeldState() }
        if case .cancelled = update { blocked = true }
        return update
    }

    mutating func reset(_ reason: HvfOrderedInputQueue.Failure) {
        stream.cancel(reason)
        recovering = false
        held = HvfPointerHeldState()
    }
}
