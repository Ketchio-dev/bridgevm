import Foundation

/// Cleanup shares the serial agent transport but never replays a user request.
struct HvfRecoverableInputStream {
    private var stream = HvfNegotiatedInputStream()
    private var cleanup = HvfPointerCleanup()
    private var queued: [HvfOrderedInputQueue.Event] = [] // The stream's unsent events, a replaced move included.
    private var active: HvfOrderedInputQueue.Event?
    var isRecovering: Bool { cleanup.recovering && !cleanup.blocked }
    var state: HvfNegotiatedInputStream.State { cleanup.blocked ? .failed : (isRecovering ? .negotiating : stream.state) }
    var count: Int { stream.count + cleanup.count }

    mutating func enqueue(_ event: HvfOrderedInputQueue.Event, now: Date) -> HvfNegotiatedInputStream.Admission {
        guard !cleanup.blocked, !isRecovering else { return .refused }
        let held = stream.count, admission = stream.enqueue(event, now: now)
        if admission == .queued { if stream.count == held { _ = queued.popLast() }; queued.append(event) }
        return admission
    }

    mutating func poll(serviceReady: Bool, legacyQuiescent: Bool, lines: [String], now: Date,
                       send: (String) -> Bool) -> HvfAcknowledgedInputStream.Update? {
        guard !cleanup.blocked else { return nil }
        if isRecovering { return cleanup.poll(serviceReady: serviceReady, lines: lines, now: now, send: send) }
        let update = stream.poll(serviceReady: serviceReady, legacyQuiescent: legacyQuiescent,
                                 lines: lines, now: now, send: send)
        switch update {
        case .sent:
            guard !queued.isEmpty else { cleanup.blocked = true; return stream.reset(.transportFailed) }
            let event = queued.removeFirst()
            active = event
            cleanup.held.sent(event)
        case .inserted:
            if let active { cleanup.held.inserted(active) }
            active = nil
        case let .cancelled(reason, _):
            queued.removeAll()
            active = nil
            if reason == .transportFailed || reason == .expired { cleanup.begin(now: now) }
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
            cleanup.begin(now: now)
        } else {
            cleanup.reset(reason)
        }
        return update
    }
}
