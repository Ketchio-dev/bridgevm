import Combine

/// Informational log-derived feedback; it never authorizes a filesystem action.
@MainActor
final class HvfShareRootNotice: ObservableObject {
    @Published private(set) var isBlocked = false
    @Published private(set) var stopped: HvfShareStop?
    func consume(_ event: BvAgentEvent) {
        guard case let .unknown(raw) = event else { return }
        let line = raw.hasSuffix("\r") ? String(raw.dropLast()) : raw
        switch line {
        case _ where HvfGuestResetRecord.matches(line): reset()
        case "BVAGENT SHARE root-state blocked": update(true)
        case "BVAGENT SHARE root-state ready": update(false)
        default: if stopped == nil, let stop = HvfShareStop(line: line) { stopped = stop }
        }
    }

    func reset() {
        update(false)
        if stopped != nil { stopped = nil }
    }

    private func update(_ blocked: Bool) {
        guard isBlocked != blocked else { return }
        isBlocked = blocked
    }
}
