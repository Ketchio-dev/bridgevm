import Foundation

/// Why an activated router stopped routing. A guest reset record replaces the harness, which tails
/// agent.ctl from its own start, so nothing queued for the old agent can reach the next one. Failures
/// of the old agent, its service or its requests therefore end at a reset record, even a later one.
/// A changed binding is another VM identity and a missing capability belongs to the guest; both stay
/// failed until a new owned boot.
enum HvfSessionInputFailure: Equatable {
    case guestReset
    case bindingChanged
    /// READY, re-READY or SERVICE start without a reset record. After re-READY the same harness
    /// keeps its queue, so cancelled requests may still reach the restarted agent.
    case agentRestarted
    case serviceLost
    /// Transport failure, expiry, guest rejection or an invalid receipt; the request may still run.
    case requestFailed
    case unavailable

    /// The reason the activated generation ends in this batch, or nil while it continues.
    init?(reset: Bool, serviceReady: Bool, lines: [String]) {
        if reset { self = .guestReset; return }
        if lines.contains(where: {
            $0.hasPrefix("BVAGENT READY") || $0.hasPrefix("BVAGENT re-READY") || $0.hasPrefix("BVAGENT SERVICE start")
        }) { self = .agentRestarted; return }
        guard !serviceReady else { return nil }
        self = .serviceLost
    }

    var endsAtGuestReset: Bool {
        switch self {
        case .guestReset, .agentRestarted, .serviceLost, .requestFailed: return true
        case .bindingChanged, .unavailable: return false
        }
    }
}

extension HvfSessionInputRouter {
    /// The owner's poll of one run.log batch. `poll` first cancels the generation a reset record in
    /// this batch ends; a failed router may retain only its bounded pointer cleanup.
    mutating func pollBatch(binding: [String], serviceReady: Bool, lines: [String], now: Date,
                            send: (String) -> Bool) -> HvfAcknowledgedInputStream.Update? {
        let update = poll(binding: binding, serviceReady: serviceReady, lines: lines, now: now, send: send)
        beginNextGeneration(binding: binding, lines: lines)
        return update
    }

    /// Re-arms a failed router when `lines` contain a guest reset record, for the same binding only,
    /// exactly as a fresh owned boot: legacy input until the next generation's own INPUTCAPS handshake,
    /// which the stream sends only after that generation's SERVICE start. No old request is replayed.
    /// Only an activated owned boot fails for such a reason, so attached or legacy routers never re-arm.
    mutating func beginNextGeneration(binding next: [String], lines: [String]) {
        guard failure?.endsAtGuestReset == true, binding == next,
              lines.contains(where: { HvfGuestResetRecord.matches($0) }) else { return }
        beginOwnedBoot(binding: next)
    }
}
