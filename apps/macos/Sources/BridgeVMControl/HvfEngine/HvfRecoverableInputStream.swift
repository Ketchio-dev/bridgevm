import Foundation

/// Cleanup shares the serial agent transport but never replays a user request.
struct HvfRecoverableInputStream {
    private var stream = HvfNegotiatedInputStream()
    private var cleanup = HvfAcknowledgedInputStream()
    private var held = HvfPointerHeldState()
    private var queued: [HvfOrderedInputQueue.Event] = []
    private var active: HvfOrderedInputQueue.Event?
    private var recovering = false
    private var blocked = false
    var state: HvfNegotiatedInputStream.State { blocked ? .failed : (recovering ? .negotiating : stream.state) }
    var count: Int { stream.count + cleanup.count }

    mutating func enqueue(_ event: HvfOrderedInputQueue.Event, now: Date) -> HvfNegotiatedInputStream.Admission {
        guard !blocked, !recovering else { return .refused }
        let admission = stream.enqueue(event, now: now)
        if admission == .queued { queued.append(event) }
        return admission
    }

    mutating func poll(serviceReady: Bool, legacyQuiescent: Bool, lines: [String], now: Date,
                       send: (String) -> Bool) -> HvfAcknowledgedInputStream.Update? {
        guard !blocked else { return nil }
        if recovering {
            let update = serviceReady ? cleanup.advance(lines: lines, now: now, send: send) : cleanup.cancel(.sessionChanged)
            if case .inserted = update { recovering = false; held = HvfPointerHeldState() }
            if case .cancelled = update { blocked = true }
            return update
        }
        let update = stream.poll(serviceReady: serviceReady, legacyQuiescent: legacyQuiescent,
                                 lines: lines, now: now, send: send)
        switch update {
        case .sent:
            guard !queued.isEmpty else { blocked = true; return stream.reset(.transportFailed) }
            let event = queued.removeFirst()
            active = event
            held.sent(event)
        case .inserted:
            if let active { held.inserted(active) }
            active = nil
        case .cancelled:
            queued.removeAll()
            active = nil
        default: break
        }
        return update
    }

    @discardableResult
    mutating func reset(_ reason: HvfOrderedInputQueue.Failure, now: Date = Date()) -> HvfAcknowledgedInputStream.Update {
        let update = stream.reset(reason)
        queued.removeAll()
        active = nil
        if reason == .targetChanged {
            if !recovering, !blocked, let release = held.release {
                recovering = cleanup.enqueue(.pointer(release), now: now)
                if !recovering { blocked = true }
            }
        } else {
            cleanup.cancel(reason)
            recovering = false
            held = HvfPointerHeldState()
        }
        return update
    }
}
