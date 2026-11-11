import Foundation

/// The releases of what sent requests may have pressed (held keys, then the buttons) on the same serial transport,
/// never a replay of the failed request.
struct HvfPointerCleanup {
    private var stream = HvfAcknowledgedInputStream()
    var held = HvfPointerHeldState()
    /// The releases sent or still to send, in order; the stream acknowledges them one at a time.
    private var releasing: [HvfOrderedInputQueue.Event] = []
    private(set) var recovering = false
    var blocked = false
    var count: Int { stream.count }

    mutating func begin(now: Date) {
        guard !recovering, !blocked else { return }
        releasing = held.releases
        guard !releasing.isEmpty else { return }
        recovering = releasing.allSatisfy { stream.enqueue($0, now: now) }
        if !recovering { blocked = true; releasing = []; stream.cancel(.transportFailed) }
    }

    mutating func poll(serviceReady: Bool, lines: [String], now: Date,
                       send: (String) -> Bool) -> HvfAcknowledgedInputStream.Update {
        let update = serviceReady ? stream.advance(lines: lines, now: now, send: send) : stream.cancel(.sessionChanged)
        if case .inserted = update, !releasing.isEmpty {
            held.inserted(releasing.removeFirst())
            if releasing.isEmpty { recovering = false; held = HvfPointerHeldState() }
        }
        // A release Windows inserted nothing of (a secure desktop) is still owed, and so is every release after it: the
        // router's next attempt to leave its fallback, or the next target change, retries them.
        if case let .cancelled(reason, _) = update {
            releasing = []
            if reason == .notInserted { recovering = false } else { blocked = true }
        }
        return update
    }

    mutating func reset(_ reason: HvfOrderedInputQueue.Failure) {
        stream.cancel(reason)
        releasing = []
        recovering = false
        held = HvfPointerHeldState()
    }
}
