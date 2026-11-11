import Foundation

/// Cleanup shares the serial agent transport but never replays a user request.
struct HvfRecoverableInputStream {
    private var stream = HvfNegotiatedInputStream()
    private var cleanup = HvfPointerCleanup()
    private var queued: [HvfOrderedInputQueue.Event] = [] // The stream's unsent events, a replaced move included.
    private var active: HvfOrderedInputQueue.Event?
    private var heldBeforeActive = HvfPointerHeldState()
    /// A `notInserted` cancellation's events, the awaited one first, in their original order (`takeNotInserted`).
    private var notInserted: [HvfOrderedInputQueue.Event] = []
    var isRecovering: Bool { cleanup.recovering && !cleanup.blocked }
    var state: HvfNegotiatedInputStream.State { cleanup.blocked ? .failed : (isRecovering ? .negotiating : stream.state) }
    var count: Int { stream.count + cleanup.count }
    var modifierChords: Bool { stream.modifierChords }
    var scrollUnits: Bool { stream.scrollUnits }
    var pointerModifiers: Bool { stream.pointerModifiers }
    var middleButton: Bool { stream.middleButton }
    var caret: Bool { stream.caret }
    var keyHold: Bool { stream.keyHold }
    /// A press Windows inserted whose release it has not, a held key's included: a `notInserted` release leaves it
    /// held here.
    var owesRelease: Bool { !cleanup.held.releases.isEmpty }
    /// The keys sent `down:` halves may have pressed in Windows and no `up:` has released (HvfPointerHeldState).
    var heldKeys: [String] { cleanup.held.keys }

    /// Sends that release (HvfPointerCleanup) ahead of anything else; the next polls report its outcome.
    mutating func releaseOwed(now: Date) { cleanup.begin(now: now) }

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
            heldBeforeActive = cleanup.held
            cleanup.held.sent(event)
        case .inserted:
            if let active { cleanup.held.inserted(active) }
            active = nil
        case let .cancelled(reason, _):
            if reason == .notInserted {
                // Nothing of the awaited event reached Windows, so it pressed nothing either.
                notInserted += (active.map { [$0] } ?? []) + queued
                cleanup.held = heldBeforeActive
            }
            queued.removeAll()
            active = nil
            if reason == .transportFailed || reason == .expired { cleanup.begin(now: now) }
        default: break
        }
        return update
    }

    mutating func takeNotInserted() -> [HvfOrderedInputQueue.Event] {
        defer { notInserted.removeAll() }
        return notInserted
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
