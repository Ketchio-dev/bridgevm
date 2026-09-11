import Foundation

/// The owner must prove legacy input is drained before enabling this stream.
struct HvfNegotiatedInputStream {
    enum State: Equatable {
        case disconnected, negotiating, waitingForLegacy, ready, unavailable, failed
    }
    enum Admission: Equatable { case queued, legacy, refused }
    private(set) var state = State.disconnected
    private var capability: HvfInputCapabilitiesRequest?
    private var stream = HvfAcknowledgedInputStream()
    var count: Int { stream.count }

    mutating func enqueue(_ event: HvfOrderedInputQueue.Event, now: Date) -> Admission {
        switch state {
        case .ready: return stream.enqueue(event, now: now) ? .queued : .refused
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
            $0.hasPrefix("BVAGENT SERVICE start") || $0.hasPrefix("PSCI_SYSTEM_RESET")
        }) { return reset(.sessionChanged) }
        switch state {
        case .disconnected:
            let request = HvfInputCapabilitiesRequest(now: now)
            capability = request
            if send(request.command) {
                state = .negotiating
            } else {
                capability = nil
                state = .unavailable
            }
        case .negotiating:
            guard var request = capability else { state = .failed; return nil }
            guard let result = request.consume(lines: lines, now: now) else {
                capability = request
                return nil
            }
            capability = nil
            switch result {
            case .supported: state = .waitingForLegacy
            case .failed: state = .unavailable
            }
        case .waitingForLegacy:
            if legacyQuiescent { state = .ready }
        case .ready:
            guard legacyQuiescent else {
                state = .failed
                return stream.cancel(.transportFailed)
            }
            let update = stream.advance(lines: lines, now: now, send: send)
            if case .cancelled = update { state = .failed }
            return update
        case .unavailable, .failed:
            break
        }
        return nil
    }

    /// Cancels host-side ownership; cannot undo input already inserted in Windows.
    @discardableResult
    mutating func reset(_ reason: HvfOrderedInputQueue.Failure) -> HvfAcknowledgedInputStream.Update {
        capability = nil
        state = .disconnected
        return stream.cancel(reason)
    }
}
