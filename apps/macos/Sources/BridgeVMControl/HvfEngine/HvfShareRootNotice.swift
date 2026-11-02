import Combine

/// Informational log-derived feedback; it never authorizes a filesystem action.
@MainActor
final class HvfShareRootNotice: ObservableObject {
    @Published private(set) var isBlocked = false

    func consume(_ event: BvAgentEvent) {
        guard case let .unknown(raw) = event else { return }
        let line = raw.hasSuffix("\r") ? String(raw.dropLast()) : raw
        if HvfGuestResetRecord.matches(line) {
            reset()
            return
        }
        switch line {
        case "BVAGENT SHARE root-state blocked": update(true)
        case "BVAGENT SHARE root-state ready": update(false)
        default: break
        }
    }

    func reset() { update(false) }

    private func update(_ blocked: Bool) {
        guard isBlocked != blocked else { return }
        isBlocked = blocked
    }
}
