import Foundation

/// Why an activated router stopped routing. A guest reset record replaces the harness, which tails
/// agent.ctl from its own start, so nothing queued for the old agent can reach the next one. Failures
/// of the old agent, its service or its requests therefore end at a reset record, even a later one.
/// An agent restart within the same harness ends the old agent instance's failures too (`endsAtAgentRestart`).
/// A changed binding is another VM identity and stays failed until a new owned boot.
enum HvfSessionInputFailure: Equatable {
    case guestReset
    case bindingChanged
    /// READY, re-READY or SERVICE start without a reset record; re-armed in the same batch.
    case agentRestarted
    case serviceLost
    /// Transport failure, expiry, guest rejection or an invalid receipt; the request may still run.
    case requestFailed
    case unavailable

    /// The reason the activated generation ends in this batch, or nil while it continues.
    init?(reset: Bool, serviceReady: Bool, lines: [String]) {
        if reset { self = .guestReset; return }
        if lines.contains(where: Self.isAgentRestart) { self = .agentRestarted; return }
        guard !serviceReady else { return nil }
        self = .serviceLost
    }

    /// A new agent instance in the same harness: crashed and restarted, or updated within the boot.
    static func isAgentRestart(_ line: String) -> Bool {
        line.hasPrefix("BVAGENT READY") || line.hasPrefix("BVAGENT re-READY") || line.hasPrefix("BVAGENT SERVICE start")
    }

    var endsAtGuestReset: Bool {
        switch self {
        case .guestReset, .agentRestarted, .serviceLost, .requestFailed: return true
        case .bindingChanged, .unavailable: return false
        }
    }

    /// What failed with the old agent instance, its capabilities included, ends with it. Its requests are never sent
    /// again: the harness (agent_restart.rs) drops the one on the wire, and one it still queues reaches the new agent
    /// ahead of the next INPUTCAPS, so no later ordered request overtakes it; its receipt then names an old id. A lost
    /// service is the host's (a Stop), not the agent's.
    var endsAtAgentRestart: Bool {
        switch self {
        case .agentRestarted, .requestFailed, .unavailable: return true
        case .guestReset, .bindingChanged, .serviceLost: return false
        }
    }
}

extension HvfSessionInputRouter {
    /// The owner's poll of one run.log batch. `poll` first cancels the generation a reset record or an
    /// agent restart in this batch ends; a failed router may retain only its bounded pointer cleanup.
    mutating func pollBatch(binding: [String], serviceReady: Bool, lines: [String], now: Date, legacyPending: Bool = false,
                            send: (String) -> Bool) -> HvfAcknowledgedInputStream.Update? {
        let update = poll(binding: binding, serviceReady: serviceReady, lines: lines, now: now,
                          legacyPending: legacyPending, send: send)
        beginNextGeneration(binding: binding, lines: lines)
        return update
    }

    /// Re-arms a failed router when `lines` end its failure (a guest reset record, or an agent restart within the
    /// harness), for the same binding only, as a fresh owned boot (what input.ctl still owes carries over): legacy
    /// input until the new agent's own INPUTCAPS handshake and a quiet HID route, the handshake sent on a later poll
    /// with the service started. No old request is replayed, and a release the old agent instance's press still owes
    /// is dropped: the agent keeps no input state across a restart (a button the old instance pressed can stay down
    /// in Windows until the next click).
    /// Only an activated owned boot fails for such a reason, so attached or legacy routers never re-arm.
    mutating func beginNextGeneration(binding next: [String], lines: [String]) {
        guard let failure, binding == next else { return }
        let reset = failure.endsAtGuestReset && lines.contains { HvfGuestResetRecord.matches($0) }
        let restart = failure.endsAtAgentRestart && lines.contains(where: HvfSessionInputFailure.isAgentRestart)
        guard reset || restart else { return }
        beginNextOwnedBoot(binding: next)
    }
}
