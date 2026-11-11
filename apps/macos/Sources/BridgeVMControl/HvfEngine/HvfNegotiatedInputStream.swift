import Foundation

/// The owner must prove legacy input is drained before enabling this stream (`legacyQuiescent`); once ready, the
/// owner keeps the two routes apart itself (HvfSessionInputRouter).
struct HvfNegotiatedInputStream {
    enum State: Equatable {
        case disconnected, negotiating, waitingForLegacy, ready, unavailable, failed
    }
    enum Admission: Equatable { case queued, legacy, refused }
    private(set) var state = State.disconnected
    private var capability: HvfInputCapabilitiesRequest?
    private var stream = HvfAcknowledgedInputStream()
    var count: Int { stream.count }
    /// Generic chords reach the guest through the VMM until this stream is ready, then only with the agent's marker.
    var modifierChords: Bool { state != .ready || capability?.modifierChords == true }
    /// Only a ready stream to an agent with the marker takes raw-unit scrolls; everything else gets notches.
    var scrollUnits: Bool { state == .ready && capability?.scrollUnits == true }
    /// Held modifiers on a pointer action and the middle button likewise reach only an agent with their markers.
    var pointerModifiers: Bool { state == .ready && capability?.pointerModifiers == true }
    var middleButton: Bool { state == .ready && capability?.middleButton == true }
    /// And only such an agent is asked where its text caret is.
    var caret: Bool { state == .ready && capability?.caret == true }

    mutating func enqueue(_ event: HvfOrderedInputQueue.Event, now: Date) -> Admission {
        switch state {
        case .ready: return capability?.supports(event) == true && stream.enqueue(event, now: now) ? .queued : .refused
        case .failed: return .refused
        case .disconnected, .negotiating, .waitingForLegacy, .unavailable: return .legacy
        }
    }

    mutating func poll(serviceReady: Bool, legacyQuiescent: Bool, lines: [String],
                       now: Date, send: (String) -> Bool) -> HvfAcknowledgedInputStream.Update? {
        if !serviceReady {
            return state == .disconnected ? nil : reset(.sessionChanged)
        }
        if lines.contains(where: {
            $0.hasPrefix("BVAGENT READY") || $0.hasPrefix("BVAGENT re-READY") ||
            $0.hasPrefix("BVAGENT SERVICE start") || HvfGuestResetRecord.matches($0)
        }) { return reset(.sessionChanged) }
        switch state {
        case .disconnected:
            let request = HvfInputCapabilitiesRequest(now: now)
            capability = request
            if send(request.command) {
                state = .negotiating
            } else {
                capability = nil; state = .unavailable
            }
        case .negotiating:
            guard var request = capability else { state = .failed; return nil }
            guard let result = request.consume(lines: lines, now: now) else {
                capability = request
                return nil
            }
            capability = request
            switch result {
            case .supported: state = .waitingForLegacy
            case .failed: state = .unavailable
            }
        case .waitingForLegacy:
            if legacyQuiescent { state = .ready }
        case .ready:
            let update = stream.advance(lines: lines, now: now, send: send)
            // Nothing of a not-inserted event reached Windows: the stream stays usable for a later retry.
            if case let .cancelled(reason, _) = update, reason != .notInserted { state = .failed }
            return update
        case .unavailable, .failed:
            break
        }
        return nil
    }

    /// Cancels host-side ownership; cannot undo input already inserted in Windows. A host
    /// target change keeps the negotiated encodings, which describe the guest agent (T17 r54).
    @discardableResult
    mutating func reset(_ reason: HvfOrderedInputQueue.Failure) -> HvfAcknowledgedInputStream.Update {
        if reason == .targetChanged, state == .ready || state == .waitingForLegacy { return stream.cancel(reason) }
        capability = nil; state = .disconnected
        return stream.cancel(reason)
    }
}
