import Foundation

/// Owns one UI input channel; file emptiness is never used as drain evidence.
struct HvfSessionInputRouter {
    private var binding: [String]?
    private var eligible = false
    private var activated = false
    private(set) var failed = false
    private var stream = HvfNegotiatedInputStream()
    var state: HvfNegotiatedInputStream.State { stream.state }
    var count: Int { stream.count }

    mutating func beginOwnedBoot(binding: [String]) {
        self = Self()
        self.binding = binding
        eligible = true
    }

    mutating func attachUnknown(binding: [String]) {
        self = Self()
        self.binding = binding
    }

    mutating func route(_ event: HvfOrderedInputQueue.Event, binding: [String], now: Date) -> HvfNegotiatedInputStream.Admission {
        guard matches(binding), !failed else { return .refused }
        guard eligible else { return .legacy }
        let admission = stream.enqueue(event, now: now)
        if admission == .legacy {
            if activated { return .refused }
            eligible = false
            stream.reset(.targetChanged)
        }
        return admission
    }

    /// Called before every legacy write, including clipboard-backed paste.
    mutating func allowLegacyWrite(binding: [String]) -> Bool {
        guard matches(binding), !failed, !activated else { return false }
        eligible = false
        stream.reset(.targetChanged)
        return true
    }

    mutating func poll(binding: [String], serviceReady: Bool, lines: [String], now: Date,
                       send: (String) -> Bool) -> HvfAcknowledgedInputStream.Update? {
        guard matches(binding), !failed, eligible else { return nil }
        let restarted = lines.contains {
            $0.hasPrefix("BVAGENT READY") || $0.hasPrefix("BVAGENT re-READY") ||
            $0.hasPrefix("BVAGENT SERVICE start") || $0.hasPrefix("PSCI_SYSTEM_RESET")
        }
        if activated && (!serviceReady || restarted) { return fail(.sessionChanged) }
        let update = stream.poll(serviceReady: serviceReady, legacyQuiescent: true,
                                 lines: lines, now: now, send: send)
        if stream.state == .ready { activated = true }
        if case .cancelled = update, activated { failed = true }
        if stream.state == .unavailable {
            if activated { failed = true } else { eligible = false }
        }
        return update
    }

    mutating func cancelTarget() -> HvfAcknowledgedInputStream.Update {
        stream.reset(.targetChanged)
    }

    private mutating func matches(_ next: [String]) -> Bool {
        guard binding == nil || binding == next else {
            _ = fail(.sessionChanged)
            return false
        }
        return true
    }

    private mutating func fail(_ reason: HvfOrderedInputQueue.Failure) -> HvfAcknowledgedInputStream.Update {
        failed = true
        eligible = false
        return stream.reset(reason)
    }
}
