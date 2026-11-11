import Combine

/// Informational log-derived feedback; it never authorizes a filesystem action.
@MainActor
final class HvfShareRootNotice: ObservableObject {
    @Published private(set) var isBlocked = false
    @Published private(set) var stopped: HvfShareStop?
    /// The runtime holds every change while the guest folder's listing is over its limit.
    @Published private(set) var paused: HvfSharePaused?
    /// Files the runtime left out for being over the size limit, this run.
    @Published private(set) var tooLarge: HvfShareTooLarge?
    /// Windows files left on the guest because the Mac had too little space for them, until they
    /// arrive.
    @Published private(set) var lowSpace: HvfShareLowSpace?
    func consume(_ event: BvAgentEvent) {
        // A whole file's arrival; a large one logs this line too.
        if case let .shareEvent(.guestToHost, name, _, _) = event { return arrived(name) }
        guard case let .unknown(raw) = event else { return }
        let line = raw.hasSuffix("\r") ? String(raw.dropLast()) : raw
        switch line {
        case _ where HvfGuestResetRecord.matches(line): reset()
        case "BVAGENT SHARE root-state blocked": update(true)
        case "BVAGENT SHARE root-state ready": update(false)
        case "BVAGENT SHARE resumed": if paused != nil { paused = nil }
        default:
            if stopped == nil, let stop = HvfShareStop(line: line) {
                stopped = stop
            } else if let pause = HvfSharePaused(line: line) {
                if paused != pause { paused = pause }
            } else if let name = HvfShareTooLarge.name(in: line) {
                tooLarge = HvfShareTooLarge(count: (tooLarge?.count ?? 0) + 1, last: name)
            } else if let name = HvfShareLowSpace.name(in: line) {
                lowSpace = lowSpace?.reported(name) ?? HvfShareLowSpace(name)
            } else if let file = BvAgentEvent.shareFile(in: line, after: "BVAGENT SHARE transfer-done guest->host ") {
                arrived(file.name)
            }
        }
    }

    /// `name` reached the Mac. Every synced file says so: only a waiting one changes the notice.
    private func arrived(_ name: String) {
        guard let lowSpace, lowSpace.isWaiting(for: name) else { return }
        self.lowSpace = lowSpace.arrived(name)
    }

    func reset() {
        update(false)
        if stopped != nil { stopped = nil }
        if paused != nil { paused = nil }
        if tooLarge != nil { tooLarge = nil }
        if lowSpace != nil { lowSpace = nil }
    }

    private func update(_ blocked: Bool) {
        guard isBlocked != blocked else { return }
        isBlocked = blocked
    }
}
